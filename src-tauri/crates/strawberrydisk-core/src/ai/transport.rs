use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::watch;

// SSE gateways may repeat several hundred bytes of metadata for each token.
// Bound wire traffic separately from the parser's 32 KiB visible-answer limit.
const MAX_STREAM_WIRE_BYTES: usize = 4 * 1024 * 1024;

use super::{
    prompt::system_prompt, stream::AiStream, AiConfiguration, AiContext, AiError, ReasoningMode,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiRequest {
    pub context: Option<AiContext>,
    pub language: String,
}

/// Tagged streaming IPC payload; reasoning never becomes final-answer text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum AiDelta {
    Text(String),
    Reasoning(String),
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiUsage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<super::AiFeedbackTarget>,
}

fn payload(config: &AiConfiguration, request: &AiRequest) -> Result<serde_json::Value, AiError> {
    if !super::language::valid_language_tag(&request.language) {
        return Err(AiError::InvalidContext);
    }
    let user = if let Some(context) = &request.context {
        context.validate()?;
        context.provider_json()?
    } else {
        "Connection test only. Reply with OK.".to_owned()
    };
    let system = system_prompt(&request.language, request.context.as_ref());
    let mut body = json!({
        "model": config.model,
        "messages": [{"role":"system","content":system},{"role":"user","content":user}],
        "stream": true, "stream_options": {"include_usage":true},
    });
    // Only send explicit overrides. A default client token cap can exhaust
    // reasoning before any visible answer, even for connection tests.
    if let Some(temperature) = config.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(max_tokens) = config.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    // Both flags are opt-in because OpenAI-compatible providers differ in their
    // extensions. Provider-default mode sends neither.
    if matches!(config.reasoning, ReasoningMode::Disabled) {
        body["thinking"] = json!({"type":"disabled"});
        body["enable_thinking"] = json!(false);
    }
    Ok(body)
}

pub(super) fn network_error(error: reqwest::Error) -> AiError {
    // Provider errors can embed authenticated URLs or echoed inputs. Never
    // expose their text through logs or IPC.
    if error.is_timeout() {
        AiError::Timeout
    } else {
        AiError::ConnectionFailed
    }
}

pub async fn explain(
    config: AiConfiguration,
    request: AiRequest,
    operation_id: &str,
    cancel: watch::Receiver<bool>,
    emit: impl FnMut(AiDelta) -> bool,
) -> Result<AiUsage, AiError> {
    config.validate()?;
    let body = payload(&config, &request)?;
    if *cancel.borrow() {
        return Err(AiError::Cancelled);
    }
    log::info!(
        "ai_request_policy operation_id={operation_id} language={} reasoning={:?} max_tokens={:?} temperature={:?} timeout_seconds=180 context_bytes={} wire_limit_bytes={MAX_STREAM_WIRE_BYTES}",
        request.language,
        config.reasoning,
        config.max_tokens,
        config.temperature,
        body["messages"][1]["content"].as_str().map(str::len).unwrap_or_default(),
    );
    let client = client(180)?;
    let builder = custom_request(&client, &config, &body, operation_id)?;
    stream_request(builder, operation_id, cancel, emit, false).await
}

fn custom_request(
    client: &reqwest::Client,
    config: &AiConfiguration,
    body: &serde_json::Value,
    operation_id: &str,
) -> Result<reqwest::RequestBuilder, AiError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-request-id",
        HeaderValue::from_str(operation_id).map_err(|_| AiError::InvalidConfiguration)?,
    );
    if !config.api_key.is_empty() {
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", config.api_key))
            .map_err(|_| AiError::InvalidConfiguration)?;
        authorization.set_sensitive(true);
        headers.insert(AUTHORIZATION, authorization);
    }
    let opencode_go = is_opencode_go(&config.endpoint);
    let session_id = (opencode_go
        || config
            .custom_headers
            .iter()
            .any(|header| header.value.contains("{{uuid}}")))
    .then(|| uuid::Uuid::new_v4().to_string());
    // Sample once so repeated placeholders across headers describe the same request.
    let timestamp = if config
        .custom_headers
        .iter()
        .any(|header| header.value.contains("{{timestamp}}"))
    {
        Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| AiError::InvalidConfiguration)?
                .as_secs()
                .to_string(),
        )
    } else {
        None
    };
    if opencode_go {
        headers.insert(
            "x-opencode-session",
            HeaderValue::from_str(session_id.as_deref().unwrap_or_default())
                .map_err(|_| AiError::InvalidConfiguration)?,
        );
    }
    for header in &config.custom_headers {
        let mut value = if let Some(session_id) = &session_id {
            header.value.replace("{{uuid}}", session_id)
        } else {
            header.value.clone()
        };
        if let Some(timestamp) = &timestamp {
            value = value.replace("{{timestamp}}", timestamp);
        }
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|_| AiError::InvalidConfiguration)?;
        let mut value = HeaderValue::from_str(&value).map_err(|_| AiError::InvalidConfiguration)?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(client
        .post(format!("{}/chat/completions", config.endpoint))
        .headers(headers)
        .json(body))
}

fn is_opencode_go(endpoint: &str) -> bool {
    reqwest::Url::parse(endpoint).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("opencode.ai")
            && url.path() == "/zen/go/v1"
    })
}

pub(super) fn client(timeout_seconds: u64) -> Result<reqwest::Client, AiError> {
    crate::http_client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout_seconds))
        .build()
        .map_err(network_error)
}

/// Both service modes share cancellation, wire limits, parser and output diagnostics.
pub(super) async fn stream_request(
    builder: reqwest::RequestBuilder,
    operation_id: &str,
    mut cancel: watch::Receiver<bool>,
    mut emit: impl FnMut(AiDelta) -> bool,
    official: bool,
) -> Result<AiUsage, AiError> {
    if *cancel.borrow() {
        return Err(AiError::Cancelled);
    }
    let started = Instant::now();
    // Keep counters outside the cancellable future so every terminal path can
    // report partial progress, including cancellation during a provider stall.
    let mut stream = AiStream::default();
    let mut received = 0;
    let run = async {
        let mut response = builder.send().await.map_err(network_error)?;
        let status = response.status().as_u16();
        let feedback = official
            .then(|| super::feedback::target(response.headers()))
            .flatten();
        if official {
            // Only a bounded UUID is accepted into logs. This joins desktop
            // operation diagnostics to the server ledger without logging context.
            if let Some(request_id) = response
                .headers()
                .get("X-Request-ID")
                .and_then(|value| value.to_str().ok())
                .filter(|value| value.len() == 36)
                .and_then(|value| uuid::Uuid::parse_str(value).ok())
            {
                log::info!(
                    "ai_official_request operation_id={operation_id} request_id={request_id}"
                );
            }
        }
        log::info!(
            "ai_response operation_id={operation_id} status={status} headers_ms={}",
            started.elapsed().as_millis()
        );
        if status != 200 {
            let official_error = official
                .then(|| super::official_protocol::response_error(response.headers()))
                .flatten();
            stream.provider_error = Some(super::provider_error::read(&mut response).await);
            if let Some(error) = official_error {
                return Err(error);
            }
            return Err(match status {
                401 | 403 => AiError::Unauthorized,
                402 | 429 => AiError::QuotaExceeded,
                404 => AiError::ModelUnavailable,
                _ => AiError::ProviderRejected,
            });
        }
        let mut first_text = true;
        let mut first_reasoning = true;
        let result = async {
            while let Some(chunk) = response.chunk().await.map_err(network_error)? {
                received += chunk.len();
                if received > MAX_STREAM_WIRE_BYTES {
                    return Err(AiError::ResponseTooLarge);
                }
                stream.feed(&chunk, &mut |text| {
                    if first_reasoning && matches!(&text, AiDelta::Reasoning(_)) {
                        first_reasoning = false;
                        log::info!(
                            "ai_first_reasoning operation_id={operation_id} elapsed_ms={}",
                            started.elapsed().as_millis()
                        );
                    }
                    if first_text && matches!(&text, AiDelta::Text(_)) {
                        first_text = false;
                        log::info!(
                            "ai_first_text operation_id={operation_id} elapsed_ms={}",
                            started.elapsed().as_millis()
                        );
                    }
                    emit(text)
                })?;
                if stream.done {
                    break;
                }
            }
            Ok(())
        }
        .await;
        result?;
        // The official protocol requires [DONE] before its feedback target can be trusted.
        // Custom OpenAI-compatible providers may terminate cleanly after finish_reason=stop.
        if official && !stream.done {
            return Err(AiError::IncompleteStream);
        }
        let mut usage = stream.finish()?;
        usage.feedback = feedback;
        Ok(usage)
    };
    let mut result = tokio::select! {
        result = run => result,
        _ = cancel.changed() => Err(AiError::Cancelled),
    };
    if official && matches!(result, Err(AiError::ProviderRejected)) {
        if let Some(error) = stream
            .provider_error
            .and_then(|diagnostic| diagnostic.official_error)
        {
            result = Err(error);
        }
    }
    let outcome = match &result {
        Ok(_) => "completed",
        Err(AiError::Cancelled) => "cancelled",
        Err(_) => "failed",
    };
    log::info!("ai_stream_finished operation_id={operation_id} received_bytes={received} text_bytes={} reasoning_bytes={} done={} finish_reason={} prompt_tokens={:?} completion_tokens={:?} outcome={outcome} reason={:?} provider_error={:?}", stream.text_bytes, stream.reasoning_bytes, stream.done, stream.finish_reason(), stream.usage.prompt_tokens, stream.usage.completion_tokens, result.as_ref().err(), stream.provider_error);
    result
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    #[ignore = "exports production messages to an explicit private artifact file without network IO"]
    fn export_ai_evaluation_messages() {
        let input = std::env::var_os("STRAWBERRYDISK_AI_EVAL_INPUT").expect("explicit corpus file");
        let output =
            std::env::var_os("STRAWBERRYDISK_AI_EVAL_OUTPUT").expect("explicit output file");
        let cases: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
        assert!(!cases.is_empty() && cases.len() <= 64);
        let config = fixture_config("https://example.com/v1".into());
        let values: Vec<_> = cases
            .into_iter()
            .map(|case| {
                let request = AiRequest {
                    context: Some(serde_json::from_value(case["context"].clone()).unwrap()),
                    language: case["language"].as_str().unwrap_or("zh-CN").into(),
                };
                let body = payload(&config, &request).unwrap();
                json!({"id":case["id"],"language":request.language,
                    "module":request.context.unwrap().subject.module_name(),
                    "messages":body["messages"]})
            })
            .collect();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .unwrap();
        std::io::Write::write_all(&mut file, &serde_json::to_vec(&values).unwrap()).unwrap();
    }

    #[test]
    fn opencode_go_requests_include_a_private_session() {
        let config = fixture_config("https://opencode.ai/zen/go/v1".into());
        let body = payload(
            &config,
            &AiRequest {
                context: None,
                language: "en-US".into(),
            },
        )
        .unwrap();
        let client = client(180).unwrap();
        let first = custom_request(&client, &config, &body, "fixture-request")
            .unwrap()
            .build()
            .unwrap();
        let second = custom_request(&client, &config, &body, "fixture-request")
            .unwrap()
            .build()
            .unwrap();
        for request in [&first, &second] {
            let session = request.headers()["x-opencode-session"].to_str().unwrap();
            assert!(uuid::Uuid::parse_str(session).is_ok());
            assert_ne!(session, "fixture-request");
            assert_eq!(request.headers()["x-request-id"], "fixture-request");
        }
        assert_ne!(
            first.headers()["x-opencode-session"],
            second.headers()["x-opencode-session"]
        );
        let mut overridden = config;
        overridden.custom_headers = vec![super::super::AiCustomHeader {
            name: "X-OpenCode-Session".into(),
            value: "custom-{{uuid}}".into(),
        }];
        let request = custom_request(&client, &overridden, &body, "fixture-request")
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request
                .headers()
                .get_all("x-opencode-session")
                .iter()
                .count(),
            1
        );
        assert!(request.headers()["x-opencode-session"]
            .to_str()
            .unwrap()
            .starts_with("custom-"));
    }

    #[test]
    fn session_header_is_limited_to_the_official_opencode_go_endpoint() {
        let client = client(180).unwrap();
        for endpoint in [
            "https://example.com/v1",
            "https://opencode.ai/zen/v1",
            "https://opencode.ai.evil.example/zen/go/v1",
            "http://opencode.ai/zen/go/v1",
            "https://opencode.ai/zen/go/v1/proxy",
        ] {
            let config = fixture_config(endpoint.into());
            let request = custom_request(&client, &config, &json!({}), "fixture-request")
                .unwrap()
                .build()
                .unwrap();
            assert!(request.headers().get("x-opencode-session").is_none());
        }
    }

    #[test]
    fn custom_headers_expand_uuid_and_timestamp_once_per_request() {
        let mut config = fixture_config("https://example.com/v1".into());
        config.custom_headers = vec![
            super::super::AiCustomHeader {
                name: "X-Provider-Key".into(),
                value: "synthetic-header-secret".into(),
            },
            super::super::AiCustomHeader {
                name: "X-Session-Affinity".into(),
                value: "session-{{uuid}}".into(),
            },
            super::super::AiCustomHeader {
                name: "X-Session-Correlation".into(),
                value: "{{uuid}}".into(),
            },
            super::super::AiCustomHeader {
                name: "Authorization".into(),
                value: "Token custom".into(),
            },
        ];
        config.custom_headers.extend([
            super::super::AiCustomHeader {
                name: "X-Timestamp".into(),
                value: "{{timestamp}}".into(),
            },
            super::super::AiCustomHeader {
                name: "X-Correlation".into(),
                value: "{{uuid}}/{{uuid}}/{{timestamp}}/{{timestamp}}/{{literal}}".into(),
            },
        ]);
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let client = client(180).unwrap();
        let first = custom_request(&client, &config, &json!({}), "fixture-request")
            .unwrap()
            .build()
            .unwrap();
        let second = custom_request(&client, &config, &json!({}), "fixture-request")
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(first.headers()["x-provider-key"], "synthetic-header-secret");
        assert_eq!(first.headers()["authorization"], "Token custom");
        assert_eq!(first.headers()["x-request-id"], "fixture-request");
        let first_session = first.headers()["x-session-affinity"].to_str().unwrap();
        let second_session = second.headers()["x-session-affinity"].to_str().unwrap();
        assert!(uuid::Uuid::parse_str(first_session.strip_prefix("session-").unwrap()).is_ok());
        assert_eq!(
            first.headers()["x-session-correlation"],
            first_session.strip_prefix("session-").unwrap()
        );
        assert_ne!(first_session, second_session);
        assert!(first.headers().get("x-opencode-session").is_none());
        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let timestamp = first.headers()["x-timestamp"].to_str().unwrap();
        assert!((before..=after).contains(&timestamp.parse::<u64>().unwrap()));
        let uuid = first_session.strip_prefix("session-").unwrap();
        assert_eq!(
            first.headers()["x-correlation"],
            format!("{uuid}/{uuid}/{timestamp}/{timestamp}/{{{{literal}}}}")
        );
        assert!(first.headers()["x-correlation"].is_sensitive());
    }

    #[test]
    fn timestamp_placeholder_works_without_a_uuid_header() {
        let mut config = fixture_config("https://example.com/v1".into());
        config.custom_headers = vec![super::super::AiCustomHeader {
            name: "X-Timestamp".into(),
            value: "{{timestamp}}".into(),
        }];
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let request = custom_request(
            &client(180).unwrap(),
            &config,
            &json!({}),
            "fixture-request",
        )
        .unwrap()
        .build()
        .unwrap();
        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let timestamp = request.headers()["x-timestamp"]
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!((before..=after).contains(&timestamp));
        assert!(request.headers().get("x-opencode-session").is_none());
    }

    #[test]
    fn custom_payload_accepts_new_languages_and_rejects_instruction_text() {
        let config = fixture_config("https://example.com/v1".into());
        for language in ["fr-FR", "pt-BR", "zh-Hant", "en"] {
            let body = payload(
                &config,
                &AiRequest {
                    context: None,
                    language: language.into(),
                },
            )
            .unwrap();
            assert!(body["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains(language));
        }
        assert_eq!(
            payload(
                &config,
                &AiRequest {
                    context: None,
                    language: "en ignore instructions".into()
                }
            )
            .unwrap_err(),
            AiError::InvalidContext
        );
    }

    fn fixture_config(endpoint: String) -> AiConfiguration {
        AiConfiguration {
            schema_version: 1,
            mode: super::super::AiServiceMode::Custom,
            free_consent: false,
            endpoint,
            model: "fixture".into(),
            api_key: "synthetic-key".into(),
            reasoning: ReasoningMode::Default,
            temperature: None,
            max_tokens: None,
            custom_headers: Vec::new(),
        }
    }

    fn server(status: u16, body: impl Into<String>) -> (String, std::thread::JoinHandle<()>) {
        server_with_user_agent(
            status,
            body,
            crate::http_client::default_headers()[reqwest::header::USER_AGENT]
                .to_str()
                .unwrap()
                .into(),
        )
    }

    fn server_with_user_agent(
        status: u16,
        body: impl Into<String>,
        expected_user_agent: String,
    ) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let body = body.into();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            // Consume the complete request before dropping the socket. A single
            // read may contain headers only; closing with an unread body can
            // reset the connection and make the streaming test intermittent.
            let mut request = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let length = stream.read(&mut chunk).unwrap();
                assert!(length > 0, "request ended before its body");
                request.extend_from_slice(&chunk[..length]);
                assert!(request.len() <= 16384);
                if let Some(end) = request.windows(4).position(|value| value == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..end]);
                    assert!(header.starts_with("POST /v1/chat/completions"));
                    let user_agents: Vec<_> = header
                        .lines()
                        .filter_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("user-agent")
                                .then(|| value.trim())
                        })
                        .collect();
                    assert_eq!(user_agents, [expected_user_agent.as_str()]);
                    let body_length: usize = header
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + body_length {
                        break;
                    }
                }
            }
            // Limit/cancellation tests may intentionally close the client mid-body.
            let _ = write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        });
        (format!("http://{address}/v1"), handle)
    }

    #[tokio::test]
    async fn custom_user_agent_overrides_the_default_only_for_its_request() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Answer\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        for custom_user_agent in [None, Some("CustomClient/1.0"), None] {
            let default_headers = crate::http_client::default_headers();
            let expected = custom_user_agent.unwrap_or(
                default_headers[reqwest::header::USER_AGENT]
                    .to_str()
                    .unwrap(),
            );
            let (endpoint, thread) = server_with_user_agent(200, body, expected.into());
            let mut config = fixture_config(endpoint);
            if let Some(value) = custom_user_agent {
                config.custom_headers.push(super::super::AiCustomHeader {
                    name: "User-Agent".into(),
                    value: value.into(),
                });
            }
            let (_cancel, receiver) = watch::channel(false);
            let result = explain(
                config,
                AiRequest {
                    context: None,
                    language: "en-US".into(),
                },
                "fixture-request",
                receiver,
                |_| true,
            )
            .await;
            thread.join().unwrap();
            result.unwrap();
        }
    }

    #[tokio::test]
    async fn wire_budget_still_rejects_unbounded_sse_comments() {
        let frame = format!(":{}\n\n", ".".repeat(65500));
        let body = frame.repeat(65);
        assert!(body.len() > MAX_STREAM_WIRE_BYTES);
        let (endpoint, thread) = server(200, body);
        let (_cancel, receiver) = watch::channel(false);
        let result = explain(
            fixture_config(endpoint),
            AiRequest {
                context: None,
                language: "en-US".into(),
            },
            "fixture-request",
            receiver,
            |_| true,
        )
        .await;
        assert!(matches!(result, Err(AiError::ResponseTooLarge)));
        thread.join().unwrap();
    }

    #[tokio::test]
    async fn repeated_gateway_metadata_does_not_truncate_a_short_answer() {
        let frame = format!(
            "data: {}\n\n",
            json!({"id": "metadata".repeat(120), "choices":[{"delta":{"reasoning_content":"x"}}]})
        );
        let mut body = frame.repeat(1400);
        body.push_str("data: {\"choices\":[{\"delta\":{\"content\":\"Short answer\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n");
        assert!(body.len() > 1024 * 1024 && body.len() < MAX_STREAM_WIRE_BYTES);
        let (endpoint, thread) = server(200, body);
        let (_cancel, receiver) = watch::channel(false);
        let mut text = String::new();
        explain(
            fixture_config(endpoint),
            AiRequest {
                context: None,
                language: "en-US".into(),
            },
            "fixture-request",
            receiver,
            |delta| {
                if let AiDelta::Text(delta) = delta {
                    text.push_str(&delta);
                }
                true
            },
        )
        .await
        .unwrap();
        assert_eq!(text, "Short answer");
        thread.join().unwrap();
    }

    #[tokio::test]
    async fn http_stream_completes_and_http_errors_are_typed() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Purpose: cache.\"}}]}\n\ndata: {\"choices\":[{\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        for (status, expected) in [
            (200, None),
            (401, Some(AiError::Unauthorized)),
            (429, Some(AiError::QuotaExceeded)),
            (302, Some(AiError::ProviderRejected)),
        ] {
            let (endpoint, thread) = server(status, body);
            let (_cancel, receiver) = watch::channel(false);
            let mut text = String::new();
            let result = explain(
                fixture_config(endpoint),
                AiRequest {
                    context: None,
                    language: "en-US".into(),
                },
                "fixture-request",
                receiver,
                |delta| {
                    if let AiDelta::Text(delta) = delta {
                        text.push_str(&delta);
                    }
                    true
                },
            )
            .await;
            assert_eq!(result.err(), expected);
            if status == 200 {
                assert_eq!(text, "Purpose: cache.");
            }
            thread.join().unwrap();
        }
    }

    #[tokio::test]
    async fn pre_cancelled_request_never_connects() {
        let (_cancel, receiver) = watch::channel(true);
        let result = explain(
            fixture_config("http://127.0.0.1:1/v1".into()),
            AiRequest {
                context: None,
                language: "en-US".into(),
            },
            "cancelled-fixture",
            receiver,
            |_| true,
        )
        .await;
        assert_eq!(result.unwrap_err(), AiError::Cancelled);
    }

    #[tokio::test]
    #[ignore = "calls the explicitly configured AI provider and incurs token usage"]
    async fn actual_provider_stream() {
        let mut config = fixture_config(
            std::env::var("STRAWBERRYDISK_AI_TEST_ENDPOINT").expect("explicit test endpoint"),
        );
        config.model = std::env::var("STRAWBERRYDISK_AI_TEST_MODEL").expect("explicit test model");
        config.api_key =
            std::env::var("ZENAI_AI_GATEWAY_API_KEY").expect("credential supplied in environment");
        config.reasoning =
            if std::env::var("STRAWBERRYDISK_AI_TEST_REASONING").as_deref() == Ok("default") {
                ReasoningMode::Default
            } else {
                ReasoningMode::Disabled
            };
        let module =
            std::env::var("STRAWBERRYDISK_AI_TEST_MODULE").unwrap_or_else(|_| "cleanup".into());
        let language =
            std::env::var("STRAWBERRYDISK_AI_TEST_LANGUAGE").unwrap_or_else(|_| "zh-CN".into());
        let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/ai-context-v2.json"
        ))
        .unwrap();
        let fixture = fixtures
            .into_iter()
            .find(|value| value["subject"]["module"] == module)
            .expect("unknown test module");
        let context: AiContext = serde_json::from_value(fixture).unwrap();
        let (_cancel, receiver) = watch::channel(false);
        let mut chunks = 0;
        let mut reasoning_chunks = 0;
        let mut answer = String::new();
        // Match the production adapter's UUID-shaped correlation header.
        let operation_id = format!(
            "00000000-0000-4000-8000-{:012x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                & 0xffff_ffff_ffff
        );
        let result = explain(
            config,
            AiRequest {
                context: Some(context),
                language: language.clone(),
            },
            &operation_id,
            receiver,
            |delta| {
                match delta {
                    AiDelta::Text(text) => {
                        chunks += 1;
                        answer.push_str(&text);
                    }
                    AiDelta::Reasoning(_) => reasoning_chunks += 1,
                }
                true
            },
        )
        .await;
        assert!(result.is_ok(), "provider result: {:?}", result.err());
        assert!(chunks > 1, "expected multiple streamed text chunks");
        println!("provider_stream text_chunks={chunks} reasoning_chunks={reasoning_chunks}");
        // This opt-in test sends synthetic fixtures only. Retain its answer for
        // language review; HTTP success alone cannot verify model compliance.
        println!("provider_answer language={language}: {answer}");
    }

    #[tokio::test]
    async fn every_module_uses_its_prompt_and_completes_a_real_http_stream() {
        let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/ai-context-v2.json"
        ))
        .unwrap();
        let expected = [
            "requiresAppClose",
            "synchronizationMayPropagate",
            "does not start or stop",
            "unapplied draft",
            "not that a fault was detected",
            "not a verified cleanup candidate",
            "different paths are interchangeable",
            "Uninstalling removes application functionality",
        ];
        assert_eq!(
            fixtures.len(),
            expected.len(),
            "every module fixture must have a prompt boundary assertion"
        );
        for (fixture, boundary) in fixtures.into_iter().zip(expected) {
            let context: AiContext = serde_json::from_value(fixture).unwrap();
            let (endpoint, thread) = server(200, "data: {\"choices\":[{\"delta\":{\"content\":\"Purpose.\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\" Impact.\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n");
            let config = fixture_config(endpoint);
            let request = AiRequest {
                context: Some(context),
                language: "zh-CN".into(),
            };
            let body = payload(&config, &request).unwrap();
            assert!(body["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains(boundary));
            assert!(body.get("tools").is_none());
            let (_sender, receiver) = watch::channel(false);
            let mut deltas = Vec::new();
            explain(config, request, "fixture-request", receiver, |text| {
                deltas.push(text);
                true
            })
            .await
            .unwrap();
            assert_eq!(
                deltas,
                [
                    AiDelta::Text("Purpose.".into()),
                    AiDelta::Text(" Impact.".into())
                ]
            );
            thread.join().unwrap();
        }
    }

    #[test]
    fn request_uses_provider_token_budget_and_has_no_tools() {
        let mut config = AiConfiguration {
            schema_version: 1,
            mode: super::super::AiServiceMode::Custom,
            free_consent: false,
            endpoint: "https://example.com/v1".into(),
            model: "example".into(),
            api_key: "test".into(),
            reasoning: ReasoningMode::Default,
            temperature: None,
            max_tokens: None,
            custom_headers: Vec::new(),
        };
        let request = AiRequest {
            context: None,
            language: "en-US".into(),
        };
        let body = payload(&config, &request).unwrap();
        assert_eq!(body["stream"], true);
        assert!(body.get("temperature").is_none());
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("max_completion_tokens").is_none());
        assert!(body.get("tools").is_none());
        assert!(body.get("thinking").is_none());
        config.reasoning = ReasoningMode::Disabled;
        let body = payload(&config, &request).unwrap();
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("max_completion_tokens").is_none());
        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body["enable_thinking"], false);
        let request = AiRequest {
            context: Some(
                serde_json::from_value(
                    serde_json::from_str::<Vec<serde_json::Value>>(include_str!(
                        "../../../../../tests/fixtures/ai-context-v2.json"
                    ))
                    .unwrap()
                    .remove(0),
                )
                .unwrap(),
            ),
            language: "en-US".into(),
        };
        for reasoning in [ReasoningMode::Default, ReasoningMode::Disabled] {
            config.reasoning = reasoning;
            let body = payload(&config, &request).unwrap();
            assert!(body.get("max_tokens").is_none());
            assert!(body.get("max_completion_tokens").is_none());
            assert!(body.get("tools").is_none());
        }
        config.temperature = Some(0.7);
        config.max_tokens = Some(4096);
        let body = payload(&config, &request).unwrap();
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["max_tokens"], 4096);
        assert!(body.get("max_completion_tokens").is_none());
    }
}
