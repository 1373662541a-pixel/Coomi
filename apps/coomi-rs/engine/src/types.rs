use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelCapabilities {
    #[serde(default = "default_context_window")]
    pub context_window: u64,
    #[serde(default = "default_effective_context_window_percent")]
    pub effective_context_window_percent: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact_token_limit: Option<u64>,
    #[serde(default)]
    pub auto_compact_scope: AutoCompactScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comp_hash: Option<String>,
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u64,
    #[serde(default)]
    pub supports_remote_compaction: bool,
    #[serde(default)]
    pub supports_vision: bool,
    #[serde(default)]
    pub supports_native_tools: bool,
    #[serde(default)]
    pub supports_web_search: bool,
    #[serde(default)]
    pub supports_parallel_tool_calls: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            context_window: default_context_window(),
            effective_context_window_percent: default_effective_context_window_percent(),
            auto_compact_token_limit: None,
            auto_compact_scope: AutoCompactScope::Total,
            comp_hash: None,
            max_output_tokens: default_max_output_tokens(),
            supports_remote_compaction: false,
            supports_vision: false,
            supports_native_tools: true,
            supports_web_search: false,
            supports_parallel_tool_calls: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoCompactScope {
    #[default]
    Total,
    BodyAfterPrefix,
}

impl ModelCapabilities {
    pub fn effective_context_window(&self) -> u64 {
        self.context_window
            .saturating_mul(u64::from(self.effective_context_window_percent))
            / 100
    }

    pub fn auto_compact_token_limit(&self) -> u64 {
        let derived = self.context_window.saturating_mul(9) / 10;
        self.auto_compact_token_limit
            .map_or(derived, |limit| limit.min(derived))
            .min(self.effective_context_window())
    }
}

const fn default_context_window() -> u64 {
    256_000
}

const fn default_effective_context_window_percent() -> u8 {
    95
}

const fn default_max_output_tokens() -> u64 {
    8_192
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub compaction_summary: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub internal: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_items: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageContent>,
}

const fn is_false(value: &bool) -> bool {
    !*value
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::plain(Role::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::plain(Role::User, content)
    }

    pub fn assistant(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        let mut tool_calls = tool_calls;
        for call in &mut tool_calls {
            sanitize_json_encoded_data(&mut call.arguments);
        }
        Self {
            role: Role::Assistant,
            content: sanitize_long_encoded_data(&content.into()),
            tool_calls,
            tool_call_id: None,
            compaction_summary: false,
            internal: false,
            provider_items: Vec::new(),
            images: Vec::new(),
        }
    }

    pub fn tool(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: sanitize_long_encoded_data(&content.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id.into()),
            compaction_summary: false,
            internal: false,
            provider_items: Vec::new(),
            images: Vec::new(),
        }
    }

    fn plain(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: sanitize_long_encoded_data(&content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
            compaction_summary: false,
            internal: false,
            provider_items: Vec::new(),
            images: Vec::new(),
        }
    }

    pub fn internal_user(content: impl Into<String>) -> Self {
        let mut message = Self::user(content);
        message.internal = true;
        message
    }

    pub fn summary(content: impl Into<String>) -> Self {
        let mut message = Self::user(content);
        message.compaction_summary = true;
        message
    }

    pub fn provider_item(item: Value) -> Self {
        let mut message = Self::assistant(String::new(), Vec::new());
        message.compaction_summary = true;
        let mut item = item;
        sanitize_json_encoded_data(&mut item);
        message.provider_items.push(item);
        message
    }
}

/// Replace very long inline encodings before they enter model context or persistence.
/// Structured `ImageContent` is intentionally excluded and remains available to vision calls.
pub fn sanitize_long_encoded_data(input: &str) -> String {
    const MIN_ENCODED_CHARS: usize = 4_096;
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len().min(64 * 1024));
    let mut cursor = 0;
    let mut index = 0;
    while index < bytes.len() {
        if !is_base64_body_byte(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_base64_body_byte(bytes[index]) {
            index += 1;
        }
        let mut padding = 0;
        while index < bytes.len() && bytes[index] == b'=' && padding < 2 {
            index += 1;
            padding += 1;
        }
        let encoded = &input[start..index];
        if encoded.len() < MIN_ENCODED_CHARS {
            continue;
        }
        let is_hex = encoded.len() % 2 == 0 && encoded.bytes().all(|byte| byte.is_ascii_hexdigit());
        let is_base64 =
            encoded.len() % 4 == 0 && encoded.bytes().filter(|byte| *byte == b'=').count() <= 2;
        if !is_hex && !is_base64 {
            continue;
        }
        output.push_str(&input[cursor..start]);
        let kind = if is_hex { "hex" } else { "base64" };
        output.push_str(&format!(
            "[encoded_data omitted type={kind} chars={} md5={:x}]",
            encoded.len(),
            md5::compute(encoded.as_bytes())
        ));
        cursor = index;
    }
    if cursor == 0 {
        return input.to_owned();
    }
    output.push_str(&input[cursor..]);
    output
}

fn is_base64_body_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/')
}

pub fn sanitize_json_encoded_data(value: &mut Value) {
    match value {
        Value::String(text) => *text = sanitize_long_encoded_data(text),
        Value::Array(values) => values.iter_mut().for_each(sanitize_json_encoded_data),
        Value::Object(values) => values.values_mut().for_each(sanitize_json_encoded_data),
        _ => {}
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

impl ToolCall {
    pub fn resource_key(&self) -> Option<String> {
        [
            "path",
            "file",
            "directory",
            "cwd",
            "session_id",
            "id",
            "name",
        ]
        .iter()
        .find_map(|key| self.arguments.get(*key).and_then(Value::as_str))
        .map(|value| value.replace('\\', "/").to_ascii_lowercase())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InvalidToolCall {
    pub id: String,
    pub name: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolConcurrency {
    ReadOnly,
    Mutating,
    Destructive,
    Interactive,
}

impl ToolSpec {
    /// Conservative scheduling metadata for built-ins. Unknown/MCP tools stay serial
    /// until they explicitly gain a trusted classification.
    pub fn concurrency(&self) -> ToolConcurrency {
        match self.name.as_str() {
            "read_file" | "list_dir" | "search" | "grep_files" | "web_search" | "fetch"
            | "view_image" | "show_image" | "list_skills" | "read_skill" | "memory_list"
            | "memory_read" | "memory_search" | "list_mcp" | "get_loop" | "wait_agent" => {
                ToolConcurrency::ReadOnly
            }
            "uninstall_mcp" | "uninstall_skill" | "memory_delete" | "close_agent" => {
                ToolConcurrency::Destructive
            }
            "request_user_input" | "request_file_import" | "request_file_export" => {
                ToolConcurrency::Interactive
            }
            _ => ToolConcurrency::Mutating,
        }
    }

    pub fn background_capable(&self) -> bool {
        matches!(self.name.as_str(), "shell" | "local_shell" | "wait_agent")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImageContent {
    pub media_type: String,
    pub data: String,
}

impl ImageContent {
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.media_type, self.data)
    }
}

#[derive(Clone, Debug)]
pub struct ModelRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CompactionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub system_prompt: String,
    pub tools: Vec<ToolSpec>,
}

#[derive(Clone, Debug)]
pub struct CompactionResponse {
    pub messages: Vec<ChatMessage>,
    pub usage: TokenUsage,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub cache_observed_input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_data_available: bool,
}

impl TokenUsage {
    pub fn add(&mut self, other: &Self) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.cached_input_tokens = self
            .cached_input_tokens
            .saturating_add(other.cached_input_tokens);
        self.cache_observed_input_tokens = self
            .cache_observed_input_tokens
            .saturating_add(other.cache_observed_input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_data_available |= other.cache_data_available;
    }

    pub fn total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }

    pub fn saturating_sub(&self, previous: &Self) -> Self {
        Self {
            input_tokens: self.input_tokens.saturating_sub(previous.input_tokens),
            cached_input_tokens: self
                .cached_input_tokens
                .saturating_sub(previous.cached_input_tokens),
            cache_observed_input_tokens: self
                .cache_observed_input_tokens
                .saturating_sub(previous.cache_observed_input_tokens),
            output_tokens: self.output_tokens.saturating_sub(previous.output_tokens),
            cache_data_available: self.cache_data_available,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ModelResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    /// Tool calls that were rejected before execution because their arguments
    /// could not be normalized to a JSON object.
    pub invalid_tool_calls: Vec<InvalidToolCall>,
    pub usage: TokenUsage,
    pub streamed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderErrorKind {
    Http,
    Timeout,
    Connect,
    Dns,
    Tls,
    Proxy,
    Redirect,
    RequestBuild,
    RequestBody,
    LocalIo,
    Request,
    Stream,
    Decode,
}

#[derive(Debug)]
pub struct ProviderRequestError {
    pub phase: &'static str,
    pub kind: ProviderErrorKind,
    pub status: Option<u16>,
    pub retry_after_ms: Option<u64>,
    pub request_id: Option<String>,
    pub retryable: bool,
    pub detail: String,
}

impl std::fmt::Display for ProviderRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "provider error [phase={} kind={:?} retryable={}",
            self.phase, self.kind, self.retryable
        )?;
        if let Some(status) = self.status {
            write!(formatter, " status={status}")?;
        }
        if let Some(retry_after_ms) = self.retry_after_ms {
            write!(formatter, " retry_after_ms={retry_after_ms}")?;
        }
        if let Some(request_id) = &self.request_id {
            write!(formatter, " request_id={request_id}")?;
        }
        write!(formatter, "]: {}", self.detail)
    }
}

impl std::error::Error for ProviderRequestError {}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub plan: Option<PlanState>,
    pub loop_state: Option<LoopState>,
    pub additional_context: Option<String>,
    pub images: Vec<ImageContent>,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            plan: None,
            loop_state: None,
            additional_context: None,
            images: Vec::new(),
        }
    }

    pub fn error(output: impl Into<String>) -> Self {
        Self {
            success: false,
            output: output.into(),
            plan: None,
            loop_state: None,
            additional_context: None,
            images: Vec::new(),
        }
    }

    pub fn with_plan(mut self, plan: PlanState) -> Self {
        self.plan = Some(plan);
        self
    }

    pub fn with_loop(mut self, loop_state: LoopState) -> Self {
        self.loop_state = Some(loop_state);
        self
    }

    pub fn with_additional_context(mut self, context: impl Into<String>) -> Self {
        self.additional_context = Some(context.into());
        self
    }

    pub fn with_image(mut self, media_type: impl Into<String>, data: impl Into<String>) -> Self {
        self.images.push(ImageContent {
            media_type: media_type.into(),
            data: data.into(),
        });
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanStep {
    pub step: String,
    pub status: PlanStepStatus,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanState {
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub steps: Vec<PlanStep>,
}

impl PlanState {
    pub fn validate(&self) -> Result<(), String> {
        if self.steps.is_empty() {
            return Err("plan must contain at least one step".into());
        }
        if self
            .steps
            .iter()
            .filter(|step| step.status == PlanStepStatus::InProgress)
            .count()
            > 1
        {
            return Err("at most one plan step may be in progress".into());
        }
        if self.steps.iter().any(|step| step.step.trim().is_empty()) {
            return Err("plan steps must not be empty".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopStatus {
    Active,
    Paused,
    Blocked,
    UsageLimited,
    BudgetLimited,
    Complete,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LoopState {
    pub objective: String,
    pub status: LoopStatus,
    #[serde(default)]
    pub token_budget: Option<u64>,
    #[serde(default)]
    pub tokens_used: u64,
    #[serde(default)]
    pub time_used_seconds: u64,
    #[serde(default)]
    pub blocked_streak: u8,
    #[serde(default)]
    pub turns_completed: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UserInputOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<UserInputOption>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UserInputRequest {
    pub questions: Vec<UserInputQuestion>,
    pub auto_resolution_ms: Option<u64>,
}

pub type UserInputResponse = BTreeMap<String, String>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileTransferRequest {
    pub request_id: String,
    pub operation: String,
    pub path: Option<String>,
    pub suggested_name: Option<String>,
    pub multiple: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AgentEvent {
    ModelStarted {
        provider: String,
        model: String,
        round: usize,
    },
    ConnectionRetry {
        attempt: u8,
        max_attempts: u8,
        delay_ms: u64,
        message: String,
    },
    /// Discard partial stream output before retrying the same model request.
    StreamReset,
    Text(String),
    TextDelta(String),
    ReasoningDelta(String),
    ContextUpdated(ContextStatus),
    /// Usage reported by one completed model request. This is emitted after
    /// every tool-loop model call so observers can refresh live statistics.
    ModelUsage {
        total: TokenUsage,
        request: TokenUsage,
    },
    CompactionStarted {
        automatic: bool,
    },
    CompactionCompleted {
        automatic: bool,
        before_tokens: u64,
        after_tokens: u64,
    },
    PlanUpdated(PlanState),
    LoopUpdated(LoopState),
    QueuedInputAccepted(Vec<String>),
    ToolStarted(ToolCall),
    ToolFinished {
        call: ToolCall,
        result: ToolResult,
    },
    TurnCompleted {
        total: TokenUsage,
        turn: TokenUsage,
    },
}

pub trait AgentObserver: Send + Sync {
    fn on_event(&self, event: &AgentEvent);
}

#[async_trait]
pub trait TurnControl: Send + Sync {
    /// Wait at a resumable boundary. Implementations must not report a running
    /// model request or tool subprocess as paused before this method is reached.
    async fn safe_point(&self) -> Result<()>;
}

pub struct NoopObserver;

impl AgentObserver for NoopObserver {
    fn on_event(&self, _event: &AgentEvent) {}
}

#[async_trait]
pub trait ApprovalHandler: Send + Sync {
    async fn approve(&self, call: &ToolCall, reason: &str) -> bool;

    async fn request_user_input(&self, _request: &UserInputRequest) -> Option<UserInputResponse> {
        None
    }

    async fn request_file_transfer(&self, _request: &FileTransferRequest) -> Option<Vec<String>> {
        None
    }
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn model(&self) -> &str;
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse>;

    async fn complete_stream(
        &self,
        request: ModelRequest,
        _observer: &dyn ModelStreamObserver,
    ) -> Result<ModelResponse> {
        self.complete(request).await
    }

    async fn compact(&self, _request: CompactionRequest) -> Result<Option<CompactionResponse>> {
        Ok(None)
    }
}

pub trait ModelStreamObserver: Send + Sync {
    fn on_text_delta(&self, delta: &str);
    fn on_reasoning_delta(&self, delta: &str);
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextStatus {
    pub used_tokens: u64,
    pub context_window: u64,
    pub effective_context_window: u64,
    pub auto_compact_token_limit: u64,
    pub remaining_tokens: u64,
    pub used_percent: u8,
    pub remaining_percent: u8,
    pub auto_compact_scope_tokens: u64,
    pub compaction_count: u64,
}

#[async_trait]
pub trait ToolRuntime: Send + Sync {
    fn specs(&self) -> Vec<ToolSpec>;
    async fn call(&self, call: &ToolCall, approval: &dyn ApprovalHandler) -> ToolResult;

    async fn lifecycle(&self, _event: &str, _payload: Value) -> Result<Option<String>, String> {
        Ok(None)
    }
}

#[cfg(test)]
mod encoding_tests {
    use super::sanitize_long_encoded_data;

    #[test]
    fn removes_one_megabyte_base64_without_retaining_payload() {
        let payload = "QUJD".repeat(256 * 1024);
        let sanitized = sanitize_long_encoded_data(&format!("data:image/png;base64,{payload}"));
        assert!(sanitized.contains("encoded_data omitted type=base64"));
        assert!(sanitized.contains("chars=1048576"));
        assert!(!sanitized.contains(&payload[..4096]));
        assert!(sanitized.len() < 256);
    }

    #[test]
    fn keeps_hashes_and_regular_jwts() {
        let hash = "0123456789abcdef".repeat(4);
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.signature";
        let input = format!("hash={hash} jwt={jwt}");
        assert_eq!(sanitize_long_encoded_data(&input), input);
    }
}
