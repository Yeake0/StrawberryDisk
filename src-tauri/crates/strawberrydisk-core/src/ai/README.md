# Optional AI explanations

The default service mode is local. Existing `free` configurations migrate to
`local` when read; the original official service is not registered in Tauri.
The local adapter downloads a pinned, SHA-256-verified llama.cpp CPU runtime
and Qwen3-0.6B Q4 GGUF only after an explicit user action, then reuses the
same Core prompt and context transport over authenticated loopback. The custom
provider mode and its saved credentials remain separate. Model files are not
bundled with the installer and remain under the application's local data path.

Explicit per-item requests explain built-in cleanup rules, privacy data kinds,
startup registrations, system settings, maintenance actions, large files, and
exact-content duplicate copies, and application uninstall candidates.
It does not select items, change risk classifications, execute commands, or
authorize cleanup. Existing native preflight and confirmation remain authoritative.

## Boundaries

- The page maps its result to `AiContext`, an explicit metadata allowlist.
  Never add file contents, authentication secrets, process arguments, or arbitrary scan objects.
  Startup and file locations are explicitly included for software attribution.
  Custom cleanup rules are not yet supported by this adapter.
- Core validates configuration and context, builds prompts, enforces request
  limits, and decodes streaming responses. Model output is untrusted Markdown.
- Core stores the complete provider configuration as plain JSON in
  `data/ai.json` below the adapter-supplied application data directory. A private
  temporary file is atomically persisted over the previous document. The file
  contains the API key and must not be included in logs or feedback attachments.
  Unsupported schemas and invalid documents are rejected without rewriting.
  The unreleased credential-store prototype is not migrated automatically:
  re-enter that preview's settings once; existing OS credentials are untouched.
- Core persists the global feature preference separately in `data/ai-preferences.json`
  (schema 1). A missing file preserves the previous enabled behavior; unreadable,
  invalid, and unsupported documents fail closed without being rewritten.
  Provider configuration deletion never changes this preference. Tauri serializes
  preference writes with request admission and cancels streams and reservations
  when disabling. The frontend loads the preference before mounting,
  hides all explanation UI except the settings toggle, and discards late callbacks.
  Re-enabling preserves provider configuration but never resumes old requests.
- Tauri owns request reservation, cancellation, IPC channels, and diagnostic logs.
- The frontend service owns IPC sessions. The AI store owns transient UI state
  and a bounded, memory-only result cache. Changing configuration clears the cache.
  Shared settings UI is store-independent and retrieves the saved key only while
  editing, masked by default with an explicit visibility toggle. The page shell
  hosts a non-modal panel inside content bounds, above its action footer.
  Clicking a configured item starts its explanation.
  Each module owns one panel and IPC session. Navigation and minimize preserve
  its stream, answer, reasoning and minimized state. Close cancels only that
  module; selecting another item in the same module replaces its previous request.
  The backend permits up to nine independent reservations: eight module streams
  plus a connection test. Cancellation and completion release only their own IDs.
  State is memory-only and does not survive application exit. Configuration changes
  cancel active requests and clear the shared cache, but retain displayed answers;
  only the initiating panel may restart, never hidden panels automatically.
- Each page owns a pure metadata projection. Version 2 of the transient context
  uses a tagged subject with domain-specific facts; older preview contexts are
  rejected. Configuration schema 2 adds the service mode and retains the old
  `freeConsent` field for compatibility. It no longer gates requests. Schema 1
  remains custom; saving migrates atomically.
  Startup explanations include original software names, descriptions, publishers,
  versions, signature status, executable paths and registration paths without
  redaction. Cleanup includes original source paths, source-level block reasons,
  running processes and scan capabilities. Optimization includes selection kind
  and diagnostic codes; `hasRecordedOriginalValue` describes existing saved history,
  not whether future changes can capture an original value. Maintenance includes
  the stable task ID and current status.
  Application uninstall includes product identity, native capability, record state, installation mode,
  catalog availability and an unapplied default/current component scope. Equal component kind/risk/
  selection facts are aggregated; component IDs, associated data paths, fingerprints and executable
  uninstall commands are excluded. Vendor uninstallers own their cleanup scope; retained components
  do not guarantee that a vendor preserves data. Orphaned record removal is not uninstalling software.
  Optional identity metadata is tagged by platform: macOS supplies bundle identifier, product name,
  category and signing metadata; Windows supplies available executable version resources and package
  identity alongside catalog publisher/source facts. Missing fields remain unknown. Certificate subjects
  are not publishers, and reading signing metadata does not validate signatures, safety or notarization.
  `describe_application_identity` resolves an application ID and catalog revision before native reads;
  it accepts no frontend path. These bounded reads run only for an explicit explanation request, never
  during the catalog scan, and do not execute the app or search the web. Identity response schema 1 is
  independent of context schema 2. Read failures retain the existing catalog-based explanation;
  stale selection/catalog callbacks are discarded, as are identity reads pending when the panel is
  closed, AI is disabled or provider configuration changes. Windows version-resource evidence uses
  a separately retained AppX manifest executable; merged registry icon/process hints are never used. Unknown identity fields are omitted from provider text.
  These fields may identify users or installation locations and are
  sent only after an explicit explanation request to the configured provider.
  Startup groups are not silently truncated; malformed or oversized requests are rejected.
  File explanations include only name, full path, scan-reported size and modification time.
  Large-file sizes represent physical storage; duplicate sizes represent logical content length.
  Duplicate explanations include the requested copy, its native protection policy, the group kind,
  and up to 31 other copies, prioritizing protected copies. `omittedCount` explicitly marks
  any unrepresented copies. Proof tokens, scan handles, contents and selection state are excluded.
  Exact equality does not establish that different application paths are interchangeable.
  Names and paths alone do not prove software ownership, malware or uninstall residue.
  Execution arguments, opaque operational IDs, privacy profiles and record details
  remain excluded. Model attribution is inference, not proof of ownership or safety.
  Custom-provider message text omits the IPC schema version and absent descriptive
  strings, diagnostics and modification times. It preserves false/zero values,
  empty source/process inventories, protection policies, paths and null pending
  drafts. The frontend IPC and signed official-service context retain the complete
  versioned schema; compact provider text is not a replacement protocol document.
- Core combines common explanation boundaries with a subject-specific prompt.
  The requested UI language tag explicitly controls the answer language,
  regardless of the language used in item metadata. Rust contains no language
  name mapping or supported-UI-locale list.
  The model must distinguish startup configuration from service runtime, draft
  settings from applied changes, and available maintenance from diagnosed faults.
  No prompt editor or model-driven operation capabilities are exposed.
- Page deactivation or unmounting does not cancel explanations. Native state
  changes dismiss only that module's panel; they do not generate paid requests.

## Editing prompts

Prompt text lives in nine TOML resources in [`prompts/`](prompts/):

- [system.toml](prompts/system.toml): shared language and operation boundaries plus the response format;
- [cleanup.toml](prompts/cleanup.toml), [privacy.toml](prompts/privacy.toml),
  [startup.toml](prompts/startup.toml),
  [system-optimization.toml](prompts/system-optimization.toml) and
  [system-maintenance.toml](prompts/system-maintenance.toml): each tool's general
  guidance and conditional instructions;
- [large-files.toml](prompts/large-files.toml) and
  [duplicate-files.toml](prompts/duplicate-files.toml): file attribution, permanent-deletion
  consequences, exact-equality boundaries, protected targets and partial copy listings.
- [application-uninstall.toml](prompts/application-uninstall.toml): product identification and concrete
  functions, platform-specific evidence limits, proposed scope and native installer boundaries.

Use multiline literal strings (`'''`) to edit Markdown without escaping newlines
or backslashes. TOML comments explain when each field applies; comments are never
sent to the model. Headings inside the text are ordinary Markdown, not selection keys.
Only leading/trailing framing whitespace is trimmed; internal paragraph breaks remain.

Edit the text and rebuild to update instructions. `{{language}}` in `system.shared`
is the only placeholder; it receives the validated UI language tag exactly once.
Adding a UI language needs no Rust prompt mapping. Stable field names are declared
in [prompt_schema.rs](prompt_schema.rs); renaming a field requires updating its
Rust declaration and typed selectors. The build script uses the same schema to reject
missing, unknown or duplicate fields, empty text and invalid placeholders, with
source diagnostics. Do not use unsupported template markers (`{{...}}`) in other fields.

Sources are embedded with `include_str!` and parsed once per process. Runtime does
not read files or split Markdown headings. [prompt.rs](prompt.rs) selects applicable
fields using typed domain facts. Keep these conditions intact when editing text;
unrelated instructions must not be sent. Local and custom providers use the same
assembled prompt. File count does not determine token usage: only selected text
enters the request, not TOML syntax, field names or maintenance comments.

## Provider contract

New installations default to local AI. The user explicitly downloads the model
and runtime before the first explanation. Inference uses an authenticated
loopback endpoint on this computer. Custom providers retain their own saved
credentials and receive metadata only when selected. Changing modes preserves
the custom draft. The former official service remains in Core for compatibility
tests but is not registered as a Tauri command and receives no desktop requests.

Configure a base URL ending at the API prefix (commonly `/v1`), a model, and a key.
Requests use `POST /chat/completions` with `stream: true`. Both HTTP and HTTPS
are accepted; HTTP does not encrypt the API key or request. Local loopback services
may omit a key. Redirects and automatic retries are disabled. The editor saves
the visible custom configuration as a whole; editing the endpoint preserves the
entered key and headers. Saving local mode retains the last persisted custom
configuration and ignores unfinished custom drafts.

All AI requests use the shared
`StrawberryDisk/<version> (<OS> <OS version>; <architecture>)` user agent. Custom services
can override it with a `User-Agent` header without affecting other requests.
For the exact `https://opencode.ai/zen/go/v1` endpoint, custom requests include
`x-opencode-session` with a fresh opaque UUID. Each
connection test or explanation is currently a single-request conversation, so
the session value is stable for that request and differs between requests. Other
custom endpoints do not receive this provider-specific header automatically.
The custom-service editor also accepts up to 8 additional request headers. They
are stored with the API key in the secret-bearing configuration, excluded from
public settings and logs, and sent only to the configured custom endpoint.
Header names must be unique and valid HTTP tokens; values must be printable ASCII.
Transport-owned fields (`Host`, `Content-Type`, `Content-Length`, `Transfer-Encoding`, `Connection`,
`Upgrade`, `Proxy-Authorization`, and `X-Request-ID`) cannot be overridden.
Other custom headers take precedence over built-in headers, including Bearer
authorization and the OpenCode Go defaults. The `{{uuid}}` placeholder expands
to a fresh random UUID for each request, and `{{timestamp}}` expands to the
current Unix time in seconds. Each value is sampled once per request and reused
across all headers, including repeated placeholders in a single value. Use
`{{uuid}}` for providers requiring a dynamic affinity header. The editor displays header values directly and submits
all custom fields together. Omitted key or header fields retain their stored
values. Each header value is limited to 1,024 bytes after variable expansion;
the configuration document allows 32 KiB to accommodate JSON escaping for all
eight headers. Header and temperature guidance appears in hover tooltips.

Custom-provider streams contain `choices[0].delta.content` and a `stop` finish reason;
`[DONE]` is optional when the final event is complete. The official service still
requires `[DONE]` before accepting an answer or feedback target. Truncated,
oversized, empty, and unsuccessful responses are errors, not cacheable answers.
The optional disabled-reasoning mode sends provider-specific
extensions; use the default mode if a provider rejects them. Cancellation aborts
the local request but cannot guarantee that a provider stops billing immediately.
New configurations default to provider-managed reasoning for compatibility.
Existing saved reasoning preferences are preserved; disabling reasoning is opt-in.
Advanced custom-provider settings accept optional temperature (0–2) and positive
integer `max_tokens`. Blank values omit these fields, leaving the provider's
defaults unchanged, including connection tests. `max_completion_tokens` is not
sent. Schema 1/2 files without these additive fields retain provider defaults;
switching to the official service preserves but does not send custom overrides.
Length-limited output is a distinct error, never silently accepted or retried.
Wire traffic is bounded at 4 MiB to accommodate repeated per-token gateway
metadata. Visible text remains bounded at 32 KiB and individual SSE records at
64 KiB. Readable reasoning is independently bounded at 256 KiB.
The total request timeout is 180 seconds (connection timeout: 10 seconds)
to accommodate provider-default reasoning. These defensive byte limits and the timeout protect the client
from malformed or unbounded streams; they are not generation token budgets.

Streaming IPC separates `text` and `reasoning` deltas. Readable strings from
`delta.reasoning_content`, or the `delta.reasoning` alias, appear in a muted,
height-bounded, selectable section above the answer. Structured or encrypted
reasoning details are not rendered. Inline `<think>...</think>` content is
discarded from the answer, including tags split across stream chunks; it is
not promoted into the reasoning section. The section collapses when the answer
starts unless the user has taken control by expanding, scrolling, or selecting
text. Copy copies only the answer.
Reasoning stays in the bounded, memory-only result cache with its answer; it is
never logged, persisted, or sent back to the provider. Reasoning without a final
answer is still an empty-response error and is not cached as success.

Answers reuse the release-note Markdown renderer with compact foreground
typography and inert links. Every streaming update is sanitized through an
explicit formatting allowlist: no scripts, images, embedded documents, styles,
or event handlers. If sanitization is unsupported, Vue renders plain text and
logs a typed compatibility diagnostic. Reasoning stays plain text. Prompts ask
for one short identification sentence and three labeled bullets: purpose/source, operation impact,
and conditional advice, all in the requested language. Each bullet should use one or two short
sentences. The format is selected before scope and current-state facts so restrictions and
unapplied changes remain authoritative. No fixed word or character budget is imposed.
Each section adds useful information: identify the item, explain its role and supported source,
describe the operation's consequences, then give a choice with a reason. Translate protocol fields
into ordinary language rather than exposing flags or rule IDs. Cleanup advice weighs known space
benefit against reuse cost; file size never establishes that data or software is unused.
A provider can still violate instructions; inspect actual answers, not just successful streams.
Application uninstall explanations introduce the product and describe hypothetical app/data loss.
They omit default/current selection and pending operation status; applicable prerequisites stay in
the impact bullet. Unavailable capabilities and orphaned records still lead with their limitation.
Unresolved product names must not become speculative descriptions of their function.
Explain unfamiliar feature names with everyday words and concrete uses before describing a setting.
Distinguish changing a feature's tips or access method from disabling the feature itself. Recommended
settings without a pending change describe hypothetical effects, without selection or scan bookkeeping;
active settings and real pending changes retain their state-specific explanations.
Language, style, operational boundaries and domain facts are separate prompt sections.
Unexpected code blocks and tables remain locally scrollable. Copy
preserves the answer's Markdown source; mouse selection copies visible text.
Sanitizer regression tests use jsdom because DOMPurify does not support happy-dom
as a security test environment. Production rendering still uses the native WebView.

Logs contain module tags, context schema versions, operation IDs, durations, HTTP status, typed failure reasons, and
token counts, text/reasoning byte counts, stream completion and request policy.
Request policy logs include the validated output language for both provider modes.
Stream summaries also cover cancellation and report the actual terminal error;
normal cancellation is an informational event, not a warning. HTTP error bodies
are read only for diagnostics, bounded at 8 KiB and two seconds. Known provider
codes and parameter names are mapped to internal enums, including SSE errors;
unknown values and free-form messages are discarded. Diagnostic read failures
preserve the original HTTP error. Configuration IO failures record the precise
stage, IO error kind and native error code, without raw error messages or paths.
They must not include keys, endpoints, prompts,
response bodies, or private scan data.

## Validation

Language tags are protocol inputs, not the desktop UI locale list. Both request
paths and the website accept a maximum of 32 ASCII bytes: a 2–8 letter primary
subtag followed by optional 1–8 alphanumeric subtags separated by hyphens. This
bounded syntax check is not a full BCP 47 registry lookup. Preserve the original
tag when signing; new UI languages must not require a Rust or server allowlist
release. Shared signature vectors include a language absent from the UI catalog.
Official error mapping lives in `official_protocol`, without dependencies on
request construction or network IO. The frontend collects typed client metadata;
only the update adapter projects it into HTTP headers.

`tests/fixtures/ai-context-v2.json` contains eight synthetic module contexts shared
by frontend projection tests and Rust deserialization, prompt, and transport tests.
It is test-only contract evidence, not persisted settings or a production request
source. Keep the shared fixture so field/schema drift fails on both sides.

Run repository checks and Core tests on macOS and Windows. The ignored
`actual_provider_stream` test is an opt-in live request configured through
`ZENAI_AI_GATEWAY_API_KEY`, `STRAWBERRYDISK_AI_TEST_ENDPOINT`, and
`STRAWBERRYDISK_AI_TEST_MODEL`; it can incur provider charges. Select a synthetic
module fixture with `STRAWBERRYDISK_AI_TEST_MODULE` (defaults to `cleanup`). Check the test source
for current environment variable names before running it.
Set `STRAWBERRYDISK_AI_TEST_LANGUAGE` to review another output language (defaults to
`zh-CN`). This opt-in test prints its synthetic-fixture answer for manual review;
a successful stream alone does not prove that the requested language was used.

Three ignored tests support reproducible multi-module evaluation:
- `capture_ai_evaluation_catalogs` reads native catalogs into an existing absolute
  `STRAWBERRYDISK_AI_EVAL_DIRECTORY`, using isolated application state. It never cleans
  files or changes startup/system settings.
- `evaluate_ai_corpus` reads an explicit JSON array of `{id, context, language?}` from
  `STRAWBERRYDISK_AI_EVAL_INPUT` and writes answer/usage/timing records to a new
  `STRAWBERRYDISK_AI_EVAL_OUTPUT` file. It uses the production transport and default
  configuration, with two requests at a time and no automatic retries. Optional per-case
  language defaults to `zh-CN`; set `STRAWBERRYDISK_AI_EVAL_CONCURRENCY=1` for serial
  comparisons on rate-limited providers. Results include actual input/output token
  usage and the full answer for quality review. The provider
  variables above are required unless `STRAWBERRYDISK_AI_EVAL_CONFIGURATION` selects an existing
  custom-provider configuration JSON. That mode preserves the saved model, reasoning, custom
  headers and generation overrides without copying credentials into artifacts or command arguments.
  Production AI diagnostics are emitted without configuration, content, answers or reasoning.
  Validate costs and review every answer manually;
  HTTP success is not evidence of factual accuracy.
- `export_ai_evaluation_messages` reads the same corpus and writes only production
  system/user messages to a new `STRAWBERRYDISK_AI_EVAL_OUTPUT` file, without credentials
  or network IO. Use absolute input/output paths. Export before and after changes
  to compare request sizes and replay identical cases with unchanged model settings.
  Measure tokens through provider usage, not character counts; compare output
  quality and current-state safety separately. Prompt compression should merge
  repeated wording while preserving state/scope rules that encode measured regressions.
Keep catalogs, corpora and raw answers outside version-controlled files and feedback logs:
they can contain private paths and installation details. Output files are never
overwritten, so completed evaluation evidence survives a later failure.

Configuration-file tests use isolated temporary directories and synthetic keys.
The frontend tests cover automatic generation, minimized streaming, cancellation,
rapid selection changes, retries, and cache reuse.

## Legacy free-service reply feedback

The following contract is retained for the unregistered legacy Core adapter.
The desktop no longer submits official-service ratings or quota requests.

Completed official replies may include an optional `feedback` target (schema 1)
in the usage result. The desktop only accepts the `x-mangodisk-ai-feedback: v1`
capability with a valid server `X-Request-ID`; older servers and custom providers
never expose rating controls. Feedback targets and confirmed ratings share the
memory-only answer cache. Configuration changes discard their attribution.

`ai_set_feedback` sends a separately signed `PUT` to
`/api/v1/ai/explanations/{serverRequestId}/feedback` with
`{"rating":"positive"}`, `{"rating":"negative"}`, or `{"rating":null}`
to retract. Each operation uses a fresh request ID and nonce and a 15-second
transport timeout. It sends no prompt, answer, paths, or provider credentials.
Failed submissions retain the confirmed rating and can be retried. Expired
or inaccessible replies disable further rating until another reply is generated.
Feedback never changes AI usage, cooldowns, scan results, or cleanup selection.
