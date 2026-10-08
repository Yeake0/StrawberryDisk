//! Ratings reference completed official replies; no prompt or answer is uploaded again.
use super::{official, official_protocol, transport, AiClientMetadata, AiError};
use reqwest::{header::HeaderMap, Method};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AiFeedbackRating {
    Positive,
    Negative,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiFeedbackTarget {
    pub schema_version: u8,
    pub request_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiFeedback {
    pub schema_version: u8,
    pub request_id: String,
    pub rating: Option<AiFeedbackRating>,
    pub updated_at: Option<u64>,
}

pub(super) fn target(headers: &HeaderMap) -> Option<AiFeedbackTarget> {
    if headers.get("x-mangodisk-ai-feedback")?.to_str().ok()? != "v1" {
        return None;
    }
    let id = headers.get("x-request-id")?.to_str().ok()?;
    if id.len() != 36 {
        return None;
    }
    Uuid::parse_str(id).ok()?;
    Some(AiFeedbackTarget {
        schema_version: 1,
        request_id: id.to_owned(),
    })
}

pub async fn official_feedback(
    metadata: AiClientMetadata,
    request_id: String,
    rating: Option<AiFeedbackRating>,
) -> Result<AiFeedback, AiError> {
    if request_id.len() != 36 || Uuid::parse_str(&request_id).is_err() {
        return Err(AiError::InvalidContext);
    }
    let operation_id = Uuid::new_v4().to_string();
    let started = std::time::Instant::now();
    let result = async {
        let path = format!("/api/v1/ai/explanations/{request_id}/feedback");
        let body = serde_json::to_vec(&serde_json::json!({"rating": rating}))
            .map_err(|_| AiError::InvalidContext)?;
        let mut response = official::request(&metadata, &operation_id, Method::PUT, &path, body)?
            .send()
            .await
            .map_err(transport::network_error)?;
        if response.status().as_u16() != 200 {
            return Err(official_protocol::response_error(response.headers())
                .unwrap_or(AiError::ProviderRejected));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport::network_error)? {
            if bytes.len() + chunk.len() > 4096 {
                return Err(AiError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        #[derive(Deserialize)]
        struct Envelope {
            success: bool,
            data: AiFeedback,
        }
        let value: Envelope = serde_json::from_slice(&bytes).map_err(|_| AiError::InvalidStream)?;
        if !value.success
            || value.data.schema_version != 1
            || value.data.request_id != request_id
            || value.data.rating != rating
        {
            return Err(AiError::InvalidStream);
        }
        Ok(value.data)
    }
    .await;
    log::info!("ai_feedback_completed operation_id={operation_id} request_id={request_id} elapsed_ms={} success={} reason={:?}", started.elapsed().as_millis(), result.is_ok(), result.as_ref().err());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feedback_requires_a_supported_server_and_valid_reply_id() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-request-id",
            "2c7fa490-6ab3-4c3f-b4b4-d6cb6e3d135f".parse().unwrap(),
        );
        assert!(target(&headers).is_none());
        headers.insert("x-mangodisk-ai-feedback", "v2".parse().unwrap());
        assert!(target(&headers).is_none());
        headers.insert("x-mangodisk-ai-feedback", "v1".parse().unwrap());
        assert_eq!(target(&headers).unwrap().schema_version, 1);
        headers.insert("x-request-id", "untrusted/path".parse().unwrap());
        assert!(target(&headers).is_none());
    }
    #[tokio::test]
    #[ignore = "requires the explicitly configured local website and synthetic build signing key"]
    async fn local_website_feedback_preserves_quota_for_every_module() {
        let cases: Vec<super::super::AiContext> = serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/ai-context-v2.json"
        ))
        .unwrap();
        for context in cases {
            let module = context.subject.module_name();
            let metadata = AiClientMetadata {
                install_id: Uuid::new_v4().to_string(),
                app_version: "1.1.5".into(),
                locale: "zh-CN".into(),
                distribution: "installed".into(),
                os_version: "test".into(),
                timezone: "Asia/Shanghai".into(),
            };
            let before = super::super::official_quota(metadata.clone())
                .await
                .unwrap();
            let (_sender, cancel) = tokio::sync::watch::channel(false);
            let mut answer = String::new();
            let usage = super::super::official_explain(
                super::super::AiConfiguration::initial(),
                super::super::AiRequest {
                    context: Some(context.clone()),
                    language: metadata.locale.clone(),
                },
                metadata.clone(),
                &Uuid::new_v4().to_string(),
                cancel,
                |delta| {
                    if let super::super::AiDelta::Text(text) = delta {
                        answer.push_str(&text);
                    }
                    true
                },
            )
            .await
            .unwrap();
            assert!(!answer.is_empty());
            let reply = usage.feedback.unwrap();
            let charged = super::super::official_quota(metadata.clone())
                .await
                .unwrap();
            assert_eq!(charged.remaining, before.remaining - 1);
            for rating in [
                Some(AiFeedbackRating::Positive),
                Some(AiFeedbackRating::Negative),
                None,
            ] {
                let result = official_feedback(metadata.clone(), reply.request_id.clone(), rating)
                    .await
                    .unwrap();
                assert_eq!(result.rating, rating);
                let after = super::super::official_quota(metadata.clone())
                    .await
                    .unwrap();
                assert_eq!(after.remaining, charged.remaining);
                assert_eq!(after.next_allowed_at, charged.next_allowed_at);
            }
            let mut other = metadata.clone();
            other.install_id = Uuid::new_v4().to_string();
            assert!(matches!(
                official_feedback(
                    other,
                    reply.request_id.clone(),
                    Some(AiFeedbackRating::Positive)
                )
                .await,
                Err(AiError::FeedbackExpired)
            ));
            eprintln!("local_feedback_verified module={module} request_id={} quota_unchanged=true changed_and_retracted=true other_install_rejected=true", reply.request_id);
        }
    }
    #[tokio::test]
    async fn only_complete_official_streams_carry_feedback_targets() {
        use std::io::{Read, Write};
        for (official, complete) in [(false, true), (false, false), (true, true), (true, false)] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = [0; 4096];
                let count = socket.read(&mut request).unwrap();
                assert!(count > 0);
                let mut body = "data: {\"choices\":[{\"delta\":{\"content\":\"Answer\"},\"finish_reason\":\"stop\"}]}\n\n".to_string();
                if complete {
                    body.push_str("data: [DONE]\n\n");
                }
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nX-MangoDisk-AI-Feedback: v1\r\nX-Request-ID: 2c7fa490-6ab3-4c3f-b4b4-d6cb6e3d135f\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            let (_sender, cancel) = tokio::sync::watch::channel(false);
            let result = transport::stream_request(
                transport::client(5)
                    .unwrap()
                    .get(format!("http://{address}")),
                "fixture",
                cancel,
                |_| true,
                official,
            )
            .await;
            server.join().unwrap();
            if complete || !official {
                assert_eq!(result.unwrap().feedback.is_some(), official);
            } else {
                assert!(matches!(result, Err(AiError::IncompleteStream)));
            }
        }
    }
}
