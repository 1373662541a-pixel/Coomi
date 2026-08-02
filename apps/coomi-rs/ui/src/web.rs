use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use axum::Json;
use axum::Router;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::extract::ws::Message;
use axum::extract::ws::WebSocket;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::http::Method;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::delete;
use axum::routing::get;
use axum::routing::post;
use coomi_engine::Agent;
use coomi_engine::AgentEvent;
use coomi_engine::AgentObserver;
use coomi_engine::ApprovalHandler;
use coomi_engine::FileTransferRequest;
use coomi_engine::InputQueue;
use coomi_engine::LoopStatus;
use coomi_engine::PlanStepStatus;
use coomi_engine::Session;
use coomi_engine::SessionStore;
use coomi_engine::ToolCall;
use coomi_engine::ToolRuntime;
use coomi_engine::UserInputRequest;
use coomi_engine::UserInputResponse;
use coomi_security::AccessMode;
use coomi_security::HookRunner;
use coomi_security::SecurityPolicy;
use coomi_services::HttpModelProvider;
use coomi_services::McpRuntime;
use coomi_services::ProviderDocument;
use coomi_services::ProviderRegistry;
use coomi_services::ProviderSettings;
use coomi_services::list_installed_skills;
use coomi_tools::AgentScheduler;
use coomi_tools::CoreTools;
use coomi_tools::ProcessManager;
use futures_util::SinkExt;
use futures_util::StreamExt;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::AbortHandle;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::services::ServeFile;
use uuid::Uuid;

const PROTOCOL_VERSION: u8 = 1;
const BRIDGE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
struct AppState {
    home: PathBuf,
    cwd: PathBuf,
    port: u16,
    permission: Arc<RwLock<PermissionMode>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PermissionMode {
    Ask,
    Auto,
    Full,
}

struct ConnectionContext {
    tx: mpsc::UnboundedSender<Message>,
    permission: Arc<RwLock<PermissionMode>>,
    plan_mode: AtomicBool,
    selected_model: RwLock<Option<String>>,
    approvals: StdMutex<HashMap<String, oneshot::Sender<bool>>>,
    questions: StdMutex<HashMap<String, oneshot::Sender<String>>>,
    file_requests: StdMutex<HashMap<String, oneshot::Sender<Vec<String>>>>,
    input_queue: Arc<InputQueue>,
    active_task: StdMutex<Option<AbortHandle>>,
    running: AtomicBool,
    processes: StdMutex<Option<Arc<ProcessManager>>>,
}

impl ConnectionContext {
    fn new(tx: mpsc::UnboundedSender<Message>, permission: Arc<RwLock<PermissionMode>>) -> Self {
        Self {
            tx,
            permission,
            plan_mode: AtomicBool::new(false),
            selected_model: RwLock::new(None),
            approvals: StdMutex::new(HashMap::new()),
            questions: StdMutex::new(HashMap::new()),
            file_requests: StdMutex::new(HashMap::new()),
            input_queue: Arc::new(InputQueue::default()),
            active_task: StdMutex::new(None),
            running: AtomicBool::new(false),
            processes: StdMutex::new(None),
        }
    }

    fn send_event(&self, payload: Value) {
        self.send_envelope("event", None, payload);
    }

    fn send_ack(&self, id: Option<&str>) {
        self.send_envelope("ack", id, json!({"ok": true}));
    }

    fn send_error(&self, id: Option<&str>, message: impl Into<String>) {
        self.send_envelope(
            "error",
            id,
            json!({"message": message.into(), "code": "bridge_error"}),
        );
    }

    fn send_envelope(&self, kind: &str, id: Option<&str>, payload: Value) {
        let mut envelope = json!({
            "v": PROTOCOL_VERSION,
            "type": kind,
            "ts": unix_time(),
            "payload": payload,
        });
        if let Some(id) = id {
            envelope["id"] = Value::String(id.to_owned());
        }
        let _ = self.tx.send(Message::Text(envelope.to_string().into()));
    }
}

pub async fn serve(home: PathBuf, cwd: PathBuf, port: u16, static_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(home.join("config"))?;
    fs::create_dir_all(home.join("sessions"))?;
    anyhow::ensure!(
        static_dir.is_dir(),
        "static directory does not exist: {}",
        static_dir.display()
    );

    let permission = Arc::new(RwLock::new(load_permission_mode(&home)));
    let state = AppState {
        home,
        cwd,
        port,
        permission,
    };
    let index = static_dir.join("index.html");
    let files = ServeDir::new(static_dir).not_found_service(ServeFile::new(index));
    let app = Router::new()
        .route("/api/runtime/health", get(runtime_health))
        .route("/api/runtime/port", get(runtime_port))
        .route("/api/providers", get(list_providers).post(upsert_provider))
        .route("/api/providers/{id}", delete(delete_provider))
        .route("/api/providers/{id}/activate", post(activate_provider))
        .route("/api/providers/{id}/copy", post(copy_provider))
        .route("/api/providers/{id}/reveal", post(reveal_provider_key))
        .route(
            "/api/providers/{id}/discover-models",
            post(discover_provider_models),
        )
        .route("/ws/session/{session_id}", get(websocket_route))
        .fallback_service(files)
        // Local bridge: only allow same-origin browser access (the Android WebView and
        // a browser pointed at 127.0.0.1:{port}). Restricting CORS + WS Origin closes the
        // cross-site attack surface where an arbitrary web page could read provider keys.
        .layer(
            CorsLayer::new()
                .allow_origin(vec![
                    format!("http://127.0.0.1:{port}")
                        .parse::<HeaderValue>()
                        .expect("valid origin"),
                    format!("http://localhost:{port}")
                        .parse::<HeaderValue>()
                        .expect("valid origin"),
                ])
                .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
                .allow_headers([header::CONTENT_TYPE, header::ACCEPT]),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    println!("Coomi Rust bridge {BRIDGE_VERSION} listening on http://127.0.0.1:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn runtime_health(State(state): State<AppState>) -> Json<Value> {
    let document = read_provider_document(&state.home).ok();
    let active = document
        .as_ref()
        .and_then(|doc| doc.providers.get(&doc.active));
    let tools = SecurityPolicy::new(&state.cwd, AccessMode::FullAccess)
        .map(|policy| CoreTools::new(state.cwd.clone(), policy).specs().len())
        .unwrap_or(0);
    Json(json!({
        "status": if active.is_some() { "ok" } else { "setup_required" },
        "version": BRIDGE_VERSION,
        "cwd": state.cwd.display().to_string(),
        "engine": {
            "initialized": active.is_some(),
            "llm": active.map(|provider| provider.model.clone()),
            "tools": tools,
        },
        "runtime": format!("Rust {} ({})", BRIDGE_VERSION, std::env::consts::ARCH),
    }))
}

async fn runtime_port(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"port": state.port}))
}

async fn list_providers(State(state): State<AppState>) -> Json<Value> {
    let document =
        read_provider_document(&state.home).unwrap_or_else(|_| empty_provider_document());
    let providers = document
        .providers
        .iter()
        .map(|(id, provider)| provider_json(id, provider, id == &document.active))
        .collect::<Vec<_>>();
    Json(json!({"providers": providers, "active": document.active}))
}

async fn upsert_provider(
    State(state): State<AppState>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let id = input
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("provider id is required"))?
        .to_owned();
    let path = providers_path(&state.home);
    let mut document =
        read_provider_document(&state.home).unwrap_or_else(|_| empty_provider_document());
    let existing = document.providers.get(&id).cloned();
    let mut settings = existing.clone().unwrap_or_default();

    settings.display = string_field(&input, "name")
        .or_else(|| existing.as_ref().map(|item| item.display.clone()))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| id.clone());
    settings.provider_type = string_field(&input, "type")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| settings.provider_type.clone());
    settings.tool_protocol =
        string_field(&input, "toolProtocol").or_else(|| Some(settings.provider_type.clone()));
    if !matches!(
        settings.provider_type.as_str(),
        "openai_compatible" | "openai_responses" | "anthropic_messages" | "gemini_native"
    ) {
        return Err(ApiError::bad_request(
            "unsupported provider compatibility mode",
        ));
    }
    settings.context_window = match input.get("contextWindow").and_then(Value::as_u64) {
        Some(value @ (128_000 | 256_000 | 512_000)) => Some(value),
        Some(_) => {
            return Err(ApiError::bad_request(
                "context window must be 128000, 256000, or 512000",
            ));
        }
        None => settings.context_window.or(Some(256_000)),
    };
    settings.base_url = string_field(&input, "baseUrl")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_base_url(&id));

    let models = input
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if !models.is_empty() {
        settings
            .extra
            .insert("models".into(), json!(models.clone()));
    }
    settings.model = string_field(&input, "model")
        .filter(|value| !value.is_empty())
        .or_else(|| models.first().cloned())
        .unwrap_or(settings.model);
    settings.fast_model = string_field(&input, "fastModel")
        .filter(|value| !value.is_empty())
        .or_else(|| models.get(1).cloned());
    if let Some(api_key) = string_field(&input, "apiKey").filter(|value| !value.is_empty()) {
        settings.api_key = api_key;
    }
    if let Some(enabled) = input.get("supportsWebSearch").and_then(Value::as_bool) {
        settings.supports_web_search = enabled;
    }
    if settings.model.is_empty() {
        return Err(ApiError::bad_request("at least one model is required"));
    }
    if settings.base_url.is_empty() {
        return Err(ApiError::bad_request("base URL is required"));
    }

    document.providers.insert(id.clone(), settings);
    if document.active.is_empty()
        || input
            .get("activate")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        document.active = id;
    }
    document.save(&path).map_err(ApiError::from)?;
    Ok(Json(json!({"ok": true})))
}

async fn delete_provider(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let path = providers_path(&state.home);
    let mut document = read_provider_document(&state.home).map_err(ApiError::from)?;
    if !document.providers.contains_key(&id) {
        return Err(ApiError::not_found("provider not found"));
    }
    if document.providers.len() == 1 {
        return Err(ApiError::bad_request(
            "at least one provider must remain configured",
        ));
    }
    document.providers.remove(&id);
    if document.active == id {
        document.active = document
            .providers
            .keys()
            .next()
            .cloned()
            .unwrap_or_default();
    }
    document.save(&path).map_err(ApiError::from)?;
    Ok(Json(json!({"ok": true})))
}

async fn activate_provider(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let path = providers_path(&state.home);
    let mut document = read_provider_document(&state.home).map_err(ApiError::from)?;
    if !document.providers.contains_key(&id) {
        return Err(ApiError::not_found("provider not found"));
    }
    document.active = id;
    document.save(&path).map_err(ApiError::from)?;
    Ok(Json(json!({"ok": true})))
}

async fn copy_provider(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let path = providers_path(&state.home);
    let mut document = read_provider_document(&state.home).map_err(ApiError::from)?;
    let source = document
        .providers
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("provider not found"))?;
    let base = format!("{id}-copy");
    let mut copied_id = base.clone();
    let mut suffix = 2usize;
    while document.providers.contains_key(&copied_id) {
        copied_id = format!("{base}-{suffix}");
        suffix += 1;
    }
    document.providers.insert(copied_id.clone(), source);
    document.save(&path).map_err(ApiError::from)?;
    Ok(Json(json!({"ok": true, "id": copied_id})))
}

async fn reveal_provider_key(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let document = read_provider_document(&state.home).map_err(ApiError::from)?;
    let provider = document
        .providers
        .get(&id)
        .ok_or_else(|| ApiError::not_found("provider not found"))?;
    Ok(Json(json!({"apiKey": provider.api_key})))
}

async fn discover_provider_models(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let path = providers_path(&state.home);
    let mut document = read_provider_document(&state.home).map_err(ApiError::from)?;
    let provider = document
        .providers
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("provider not found"))?;
    let models = fetch_provider_models(&provider).await?;
    if models.is_empty() {
        return Err(ApiError::bad_request(
            "provider returned no available models",
        ));
    }
    if let Some(settings) = document.providers.get_mut(&id) {
        settings
            .extra
            .insert("models".into(), json!(models.clone()));
    }
    document.save(&path).map_err(ApiError::from)?;
    Ok(Json(json!({"models": models})))
}

async fn fetch_provider_models(provider: &ProviderSettings) -> Result<Vec<String>, ApiError> {
    let base = provider.base_url.trim_end_matches('/');
    if base.is_empty() {
        return Err(ApiError::bad_request("base URL is required"));
    }
    let endpoint = format!("{base}/models");
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|error| ApiError::bad_gateway(format!("HTTP client setup failed: {error}")))?;
    let mut request = client
        .get(&endpoint)
        .header("Accept", "application/json")
        .header("User-Agent", "Coomi-Android/2.0");
    if provider.provider_type.contains("gemini") {
        request = request.query(&[("key", provider.api_key.as_str())]);
    } else if provider.provider_type.contains("anthropic") {
        request = request
            .header("x-api-key", &provider.api_key)
            .header("anthropic-version", "2023-06-01");
    } else if !provider.api_key.is_empty() {
        request = request.bearer_auth(&provider.api_key);
    }
    let response = request.send().await.map_err(|error| {
        ApiError::bad_gateway(format!("model discovery request failed: {error}"))
    })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::bad_gateway(format!("failed to read model discovery response: {error}"))
    })?;
    if !status.is_success() {
        return Err(ApiError::bad_gateway(format!(
            "model discovery returned HTTP {status}: {}",
            preview(&body)
        )));
    }
    let value: Value = serde_json::from_str(&body)
        .map_err(|error| ApiError::bad_gateway(format!("invalid model response: {error}")))?;
    let entries = value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError::bad_gateway("model response has no data/models array"))?;
    let mut models = entries
        .iter()
        .filter_map(|entry| {
            entry
                .get("id")
                .or_else(|| entry.get("name"))
                .and_then(Value::as_str)
        })
        .map(|model| model.strip_prefix("models/").unwrap_or(model).to_owned())
        .filter(|model| !model.is_empty())
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    Ok(models)
}

async fn websocket_route(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Reject cross-origin WebSocket upgrades (e.g. from arbitrary web pages). Requests
    // without an Origin header (curl, CLI tools) are allowed — there is no browser
    // CSRF context for them.
    let allowed_origins = [
        format!("http://127.0.0.1:{}", state.port),
        format!("http://localhost:{}", state.port),
    ];
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        if !allowed_origins.iter().any(|allowed| allowed == origin) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    ws.on_upgrade(move |socket| websocket_session(socket, state, session_id))
}

async fn websocket_session(socket: WebSocket, state: AppState, session_id: String) {
    let (mut sink, mut source) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let context = Arc::new(ConnectionContext::new(tx, Arc::clone(&state.permission)));
    let writer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = source.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(envelope) = serde_json::from_str::<Value>(&text) else {
            context.send_error(None, "invalid JSON command");
            continue;
        };
        let id = envelope.get("id").and_then(Value::as_str);
        let payload = envelope.get("payload").cloned().unwrap_or(Value::Null);
        handle_command(&state, &session_id, Arc::clone(&context), id, payload).await;
    }

    if let Some(handle) = context
        .active_task
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    {
        handle.abort();
    }
    // Symmetric with `cancel`: kill any shell subprocesses the disconnected turn started.
    let processes = {
        let mut guard = context
            .processes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.take()
    };
    if let Some(processes) = processes {
        processes.terminate_all().await;
    }
    writer.abort();
}

async fn handle_command(
    state: &AppState,
    session_id: &str,
    context: Arc<ConnectionContext>,
    envelope_id: Option<&str>,
    payload: Value,
) {
    let command = payload
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match command {
        "send_message" => {
            let prompt = payload
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if prompt.is_empty() {
                context.send_error(envelope_id, "message text is required");
                return;
            }
            if context.running.swap(true, Ordering::SeqCst) {
                context.send_error(envelope_id, "a turn is already running");
                return;
            }
            context.send_ack(envelope_id);
            let turn_state = state.clone();
            let turn_session_id = session_id.to_owned();
            let turn_prompt = if context.plan_mode.load(Ordering::Relaxed) {
                format!(
                    "Work in planning mode. Inspect the project and return an actionable plan before making changes.\n\n{prompt}"
                )
            } else {
                prompt.to_owned()
            };
            let turn_context = Arc::clone(&context);
            let cleanup_context = Arc::clone(&context);
            let task = tokio::spawn(async move {
                if let Err(error) = run_turn(
                    &turn_state,
                    &turn_session_id,
                    &turn_prompt,
                    Arc::clone(&turn_context),
                )
                .await
                {
                    turn_context.send_event(json!({
                        "event_type": "agent_error",
                        "message": format!("{error:#}"),
                        "is_fatal": false,
                    }));
                }
                turn_context.send_event(json!({"event_type": "turn_end"}));
                cleanup_context.running.store(false, Ordering::SeqCst);
                cleanup_context
                    .active_task
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take();
            });
            *context
                .active_task
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(task.abort_handle());
        }
        "cancel" => {
            // Abort the agent task first (synchronous), then kill any shell subprocesses
            // started by tools; killing first would let the still-running agent spawn new
            // processes that escape this cleanup round.
            if let Some(handle) = context
                .active_task
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
            {
                handle.abort();
            }
            context.running.store(false, Ordering::SeqCst);
            let processes = {
                let mut guard = context
                    .processes
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.take()
            };
            if let Some(processes) = processes {
                processes.terminate_all().await;
            }
            context.send_ack(envelope_id);
            context.send_event(json!({"event_type": "agent_cancelled"}));
            context.send_event(json!({"event_type": "turn_end"}));
        }
        "jump_in" => {
            if let Some(text) = payload
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
            {
                context.input_queue.push(text.to_owned());
            }
            context.send_ack(envelope_id);
        }
        "approve_tool" => {
            let call_id = payload
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let allow = matches!(
                payload.get("decision").and_then(Value::as_str),
                Some("allow" | "always")
            );
            if let Some(sender) = context
                .approvals
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(call_id)
            {
                let _ = sender.send(allow);
            }
            context.send_ack(envelope_id);
        }
        "answer_question" => {
            let call_id = payload
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let answer = payload
                .get("answer")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if let Some(sender) = context
                .questions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(call_id)
            {
                let _ = sender.send(answer);
            }
            context.send_ack(envelope_id);
        }
        "file_transfer_result" => {
            let request_id = payload
                .get("request_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let paths = payload
                .get("paths")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if let Some(sender) = context
                .file_requests
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(request_id)
            {
                let _ = sender.send(paths);
            }
            context.send_ack(envelope_id);
        }
        "set_permission_mode" => {
            let mode = match payload.get("mode").and_then(Value::as_str) {
                Some("auto") => PermissionMode::Auto,
                Some("full") => PermissionMode::Full,
                _ => PermissionMode::Ask,
            };
            *context.permission.write().await = mode;
            if let Err(error) = save_permission_mode(&state.home, mode) {
                context.send_error(
                    envelope_id,
                    format!("failed to save permission mode: {error}"),
                );
                return;
            }
            context.send_ack(envelope_id);
        }
        "enter_plan_mode" => {
            context.plan_mode.store(true, Ordering::Relaxed);
            context.send_ack(envelope_id);
        }
        "exit_plan_mode" => {
            context.plan_mode.store(false, Ordering::Relaxed);
            context.send_ack(envelope_id);
        }
        "select_model" => {
            let provider = payload
                .get("provider_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let model = payload
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if provider.is_empty() || model.is_empty() {
                context.send_error(envelope_id, "provider_id and model are required");
            } else {
                let path = providers_path(&state.home);
                match read_provider_document(&state.home) {
                    Ok(mut document) if document.providers.contains_key(provider) => {
                        if let Some(settings) = document.providers.get_mut(provider) {
                            settings.model = model.to_owned();
                            let models = provider_models(settings);
                            if !models.iter().any(|item| item == model) {
                                let mut expanded = models;
                                expanded.push(model.to_owned());
                                settings.extra.insert("models".into(), json!(expanded));
                            }
                        }
                        document.active = provider.to_owned();
                        if let Err(error) = document.save(&path) {
                            context.send_error(
                                envelope_id,
                                format!("failed to persist model: {error}"),
                            );
                            return;
                        }
                    }
                    Ok(_) => {
                        context.send_error(envelope_id, "provider not found");
                        return;
                    }
                    Err(error) => {
                        context
                            .send_error(envelope_id, format!("failed to load providers: {error}"));
                        return;
                    }
                }
                *context.selected_model.write().await = Some(format!("{provider}:{model}"));
                context.send_ack(envelope_id);
            }
        }
        _ => context.send_error(envelope_id, format!("unsupported command: {command}")),
    }
}

async fn run_turn(
    state: &AppState,
    session_id: &str,
    prompt: &str,
    context: Arc<ConnectionContext>,
) -> Result<()> {
    let registry = ProviderRegistry::load(&providers_path(&state.home))
        .context("configure a provider before starting a chat")?;
    let selected = context.selected_model.read().await.clone();
    let store = SessionStore::new(&state.home);
    let requested_id = Uuid::parse_str(session_id).context("invalid session id")?;
    let existing = store.load(requested_id).ok();
    let selector = selected.as_deref().or_else(|| {
        existing.as_ref().and_then(|session| {
            (!session.provider_id.is_empty()).then_some(session.provider_id.as_str())
        })
    });
    let provider_config = registry.resolve(selector)?;
    let mut session = load_or_create_web_session(
        &store,
        requested_id,
        &provider_config.id,
        &provider_config.model,
        &state.cwd,
    );

    // Use the session's own working directory so history and context always belong
    // to the same project; fall back to the engine cwd only when the session's
    // directory no longer exists (e.g. the project folder was moved).
    let session_cwd = session.cwd.clone();
    let cwd = if session_cwd.is_dir() {
        session_cwd
    } else {
        state.cwd.clone()
    };

    let permission = *context.permission.read().await;
    let policy_mode = match permission {
        PermissionMode::Ask => AccessMode::WorkspaceWrite,
        PermissionMode::Auto | PermissionMode::Full => AccessMode::FullAccess,
    };
    let policy = SecurityPolicy::new(&cwd, policy_mode)?;
    let instructions = coomi_engine::discover_project_instructions(&cwd)?;
    let prompt_context = system_prompt(&state.home, &cwd, policy_mode, &instructions);
    let scheduler = AgentScheduler::new(
        cwd.clone(),
        state.home.clone(),
        provider_config.clone(),
        policy_mode,
        prompt_context.clone(),
    )
    .without_persistent_memory();
    let tools = CoreTools::new(cwd.clone(), policy)
        .with_skills_directory(state.home.join("skills"))
        .with_config_home(state.home.clone())
        .with_session_state(session.plan.clone(), session.loop_state.clone())
        .with_mcp_runtime(Arc::new(McpRuntime::load(&state.home).await))
        .with_hooks(Arc::new(HookRunner::load(&state.home)?))
        .with_agent_scheduler(scheduler, session.messages.clone());
    // Expose the turn's process manager so `cancel` can kill any shell started by tools.
    *context
        .processes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(tools.process_manager());
    let provider = HttpModelProvider::new(provider_config)?;
    let approval = BrowserApproval {
        context: Arc::clone(&context),
    };
    let observer = BrowserObserver::new(
        Arc::clone(&context),
        session.usage.input_tokens,
        session.usage.output_tokens,
    );
    let agent = Agent::new(prompt_context)
        .with_max_tool_rounds(96)
        .with_input_queue(Arc::clone(&context.input_queue));
    agent
        .run_turn(
            &mut session,
            prompt.to_owned(),
            &provider,
            &tools,
            &approval,
            &observer,
        )
        .await?;
    store.save(&session)?;

    while session
        .loop_state
        .as_ref()
        .is_some_and(|loop_state| loop_state.status == LoopStatus::Active)
    {
        agent
            .continue_loop(&mut session, &provider, &tools, &approval, &observer)
            .await?;
        store.save(&session)?;
    }
    Ok(())
}

fn load_or_create_web_session(
    store: &SessionStore,
    session_id: Uuid,
    provider_id: &str,
    model: &str,
    cwd: &Path,
) -> Session {
    let mut session = store.load(session_id).unwrap_or_else(|_| {
        let mut session = Session::new(provider_id, model, cwd.to_path_buf());
        session.id = session_id;
        session
    });
    // Keep the session's original working directory: a session must only ever see
    // its own project context (history + cwd), never inherit the current engine cwd.
    // Only brand-new sessions adopt the current cwd; empty cwd only happens for
    // sessions saved by older versions.
    if session.cwd.as_os_str().is_empty() {
        session.cwd = cwd.to_path_buf();
    }
    session.switch_model(provider_id, model);
    session
}

struct BrowserObserver {
    context: Arc<ConnectionContext>,
    started: StdMutex<HashMap<String, Instant>>,
    usage: StdMutex<BrowserUsageState>,
}

#[derive(Clone, Copy, Default)]
struct BrowserUsageState {
    input_tokens: u64,
    output_tokens: u64,
    context_used_tokens: u64,
    context_window_tokens: u64,
}

impl BrowserObserver {
    fn new(context: Arc<ConnectionContext>, input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            context,
            started: StdMutex::new(HashMap::new()),
            usage: StdMutex::new(BrowserUsageState {
                input_tokens,
                output_tokens,
                ..BrowserUsageState::default()
            }),
        }
    }

    fn send_usage(&self) {
        let state = *self
            .usage
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.context.send_event(browser_usage_event(state));
    }
}

fn browser_usage_event(state: BrowserUsageState) -> Value {
    let total_tokens = state.input_tokens.saturating_add(state.output_tokens);
    let context_ratio = if state.context_window_tokens == 0 {
        0.0
    } else {
        (state.context_used_tokens as f64 / state.context_window_tokens as f64).min(1.0)
    };
    json!({
        "event_type": "usage_update",
        "usage": {
            "input_tokens": state.input_tokens,
            "output_tokens": state.output_tokens,
            "total_tokens": total_tokens,
            "context_used_tokens": state.context_used_tokens,
            "context_window_tokens": state.context_window_tokens,
            "context_ratio": context_ratio,
        },
    })
}

impl AgentObserver for BrowserObserver {
    fn on_event(&self, event: &AgentEvent) {
        match event {
            AgentEvent::Text(content) | AgentEvent::TextDelta(content) => {
                self.context
                    .send_event(json!({"event_type": "text_chunk", "content": content}));
            }
            AgentEvent::ReasoningDelta(content) => {
                self.context
                    .send_event(json!({"event_type": "reasoning_chunk", "content": content}));
            }
            AgentEvent::ToolStarted(call) => {
                self.started
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(call.id.clone(), Instant::now());
                self.context.send_event(json!({
                    "event_type": "tool_start",
                    "call_id": call.id,
                    "tool_name": call.name,
                    "arguments": call.arguments,
                }));
                self.context.send_event(json!({
                    "event_type": "tool_running",
                    "call_id": call.id,
                    "tool_name": call.name,
                }));
            }
            AgentEvent::ToolFinished { call, result } => {
                let elapsed = self
                    .started
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&call.id)
                    .map(|started| started.elapsed().as_secs_f64())
                    .unwrap_or_default();
                self.context.send_event(json!({
                    "event_type": "tool_done",
                    "call_id": call.id,
                    "tool_name": call.name,
                    "elapsed": elapsed,
                    "result_preview": preview(&result.output),
                    "is_error": !result.success,
                }));
            }
            AgentEvent::TurnCompleted(usage) => {
                if let Ok(mut state) = self.usage.lock() {
                    state.input_tokens = usage.input_tokens;
                    state.output_tokens = usage.output_tokens;
                }
                self.send_usage();
            }
            AgentEvent::CompactionCompleted {
                before_tokens,
                after_tokens,
                ..
            } => {
                self.context.send_event(json!({
                    "event_type": "compression",
                    "before": before_tokens,
                    "after": after_tokens,
                }));
            }
            AgentEvent::PlanUpdated(plan) => {
                if let Some((index, step)) = plan
                    .steps
                    .iter()
                    .enumerate()
                    .find(|(_, step)| step.status == PlanStepStatus::InProgress)
                {
                    self.context.send_event(json!({
                        "event_type": "loop_step_start",
                        "step_index": index + 1,
                        "step_description": step.step,
                        "total_steps": plan.steps.len(),
                    }));
                }
            }
            AgentEvent::LoopUpdated(loop_state) => {
                self.context.send_event(json!({
                    "event_type": "loop_progress",
                    "current_step": loop_state.turns_completed,
                    "total_steps": loop_state.turns_completed + u64::from(loop_state.status == LoopStatus::Active),
                    "status": format!("{:?}", loop_state.status).to_ascii_lowercase(),
                }));
            }
            AgentEvent::ContextUpdated(status) => {
                if let Ok(mut state) = self.usage.lock() {
                    state.context_used_tokens = status.used_tokens;
                    state.context_window_tokens = status.context_window;
                }
                self.send_usage();
            }
            AgentEvent::ModelStarted { .. }
            | AgentEvent::CompactionStarted { .. }
            | AgentEvent::QueuedInputAccepted(_) => {}
        }
    }
}

struct BrowserApproval {
    context: Arc<ConnectionContext>,
}

#[async_trait]
impl ApprovalHandler for BrowserApproval {
    async fn approve(&self, call: &ToolCall, reason: &str) -> bool {
        let mode = *self.context.permission.read().await;
        if mode == PermissionMode::Full
            || (mode == PermissionMode::Auto && !reason.to_ascii_lowercase().contains("delete"))
        {
            return true;
        }
        let (sender, receiver) = oneshot::channel();
        self.context
            .approvals
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(call.id.clone(), sender);
        self.context.send_event(json!({
            "event_type": "tool_approval_request",
            "call_id": call.id,
            "tool_name": call.name,
            "arguments": call.arguments,
            "access": approval_access(reason),
            "risk_summary": reason,
        }));
        tokio::time::timeout(std::time::Duration::from_secs(300), receiver)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(false)
    }

    async fn request_user_input(&self, request: &UserInputRequest) -> Option<UserInputResponse> {
        let question = request.questions.first()?;
        let call_id = format!("question-{}", Uuid::new_v4());
        let (sender, receiver) = oneshot::channel();
        self.context
            .questions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(call_id.clone(), sender);
        self.context.send_event(json!({
            "event_type": "user_question_request",
            "call_id": call_id,
            "question": question.question,
            "options": question.options.iter().map(|option| option.label.clone()).collect::<Vec<_>>(),
            "allow_free_text": true,
        }));
        let timeout_ms = request
            .auto_resolution_ms
            .unwrap_or(300_000)
            .clamp(1_000, 300_000);
        let answer = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), receiver)
            .await
            .ok()
            .and_then(Result::ok)?;
        Some(BTreeMap::from([(question.id.clone(), answer)]))
    }

    async fn request_file_transfer(&self, request: &FileTransferRequest) -> Option<Vec<String>> {
        let (sender, receiver) = oneshot::channel();
        self.context
            .file_requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(request.request_id.clone(), sender);
        self.context.send_event(json!({
            "event_type": "file_transfer_request",
            "request_id": request.request_id,
            "operation": request.operation,
            "path": request.path,
            "suggested_name": request.suggested_name,
            "multiple": request.multiple,
        }));
        tokio::time::timeout(std::time::Duration::from_secs(600), receiver)
            .await
            .ok()
            .and_then(Result::ok)
    }
}

fn system_prompt(home: &Path, cwd: &Path, policy: AccessMode, instructions: &str) -> String {
    let skills = list_installed_skills(home)
        .unwrap_or_default()
        .into_iter()
        .filter(|skill| skill.enabled)
        .map(|skill| skill.name)
        .collect::<Vec<_>>();
    let mut prompt = format!(
        "You are Coomi, a pragmatic coding agent running locally on Android. Inspect evidence before editing, keep changes scoped, preserve unrelated work, and verify results. Use request_file_import when the user needs to choose phone files and request_file_export to return local artifacts such as APKs. For web access, use web_search to find pages and the built-in fetch tool to read their content; if web_search reports unavailable, explain the cause once and never replace it with shell, curl, wget, or repeated command-line searches.\n\nWorking directory: {}\nAccess policy: {}",
        cwd.display(),
        policy.label(),
    );
    if !skills.is_empty() {
        prompt.push_str(&format!("\nInstalled skills: {}", skills.join(", ")));
    }
    if !instructions.trim().is_empty() {
        prompt.push_str("\n\nProject instructions:\n");
        prompt.push_str(instructions);
    }
    prompt
}

fn providers_path(home: &Path) -> PathBuf {
    home.join("config").join("providers.json")
}

fn read_provider_document(home: &Path) -> Result<ProviderDocument> {
    ProviderDocument::load(&providers_path(home))
}

fn empty_provider_document() -> ProviderDocument {
    ProviderDocument {
        active: String::new(),
        providers: BTreeMap::new(),
        extra: BTreeMap::new(),
    }
}

fn provider_json(id: &str, provider: &ProviderSettings, active: bool) -> Value {
    let models = provider_models(provider);
    json!({
        "id": id,
        "name": if provider.display.is_empty() { id } else { &provider.display },
        "apiKeyMasked": mask_key(&provider.api_key),
        "hasKey": !provider.api_key.is_empty(),
        "models": models,
        "baseUrl": provider.base_url,
        "type": provider.provider_type,
        "model": provider.model,
        "fastModel": provider.fast_model,
        "toolProtocol": provider.tool_protocol,
        "contextWindow": provider.context_window.unwrap_or(256_000),
        "supportsWebSearch": provider.supports_web_search,
        "active": active,
    })
}

fn provider_models(provider: &ProviderSettings) -> Vec<String> {
    let mut models = provider
        .extra
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    for model in std::iter::once(Some(provider.model.clone()))
        .chain(std::iter::once(provider.fast_model.clone()))
        .flatten()
    {
        if !model.is_empty() && !models.contains(&model) {
            models.push(model);
        }
    }
    models
}

fn permission_settings_path(home: &Path) -> PathBuf {
    home.join("config").join("web-settings.json")
}

fn load_permission_mode(home: &Path) -> PermissionMode {
    let value = fs::read_to_string(permission_settings_path(home))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    match value
        .as_ref()
        .and_then(|value| value.get("permissionMode"))
        .and_then(Value::as_str)
    {
        Some("auto") => PermissionMode::Auto,
        Some("full") => PermissionMode::Full,
        _ => PermissionMode::Ask,
    }
}

fn save_permission_mode(home: &Path, mode: PermissionMode) -> Result<()> {
    let path = permission_settings_path(home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mode = match mode {
        PermissionMode::Ask => "ask",
        PermissionMode::Auto => "auto",
        PermissionMode::Full => "full",
    };
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({"permissionMode": mode}))?,
    )?;
    Ok(())
}

fn mask_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let tail = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("****{tail}")
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.trim().to_owned())
}

fn default_base_url(id: &str) -> String {
    match id.to_ascii_lowercase().as_str() {
        "openai" => "https://api.openai.com/v1",
        "anthropic" => "https://api.anthropic.com/v1",
        "google" | "gemini" => "https://generativelanguage.googleapis.com/v1beta",
        "deepseek" => "https://api.deepseek.com/v1",
        "zhipu" => "https://open.bigmodel.cn/api/paas/v4",
        "minimax" => "https://api.minimaxi.com/v1",
        _ => "",
    }
    .to_owned()
}

fn approval_access(reason: &str) -> &'static str {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("delete") || lower.contains("overwrite") || lower.contains("destructive") {
        "destructive"
    } else if lower.contains("write") || lower.contains("change") || lower.contains("process") {
        "write"
    } else {
        "read_only"
    }
}

fn preview(value: &str) -> String {
    let mut output = value.chars().take(1_000).collect::<String>();
    if value.chars().count() > 1_000 {
        output.push_str("...");
    }
    output
}

fn unix_time() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default()
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::bad_request(format!("{error:#}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(json!({"error": self.message}))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coomi_engine::ChatMessage;
    use coomi_services::MemoryManager;
    use coomi_services::MemoryScope;
    use coomi_services::MemoryType;

    #[test]
    fn provider_json_never_exposes_secret() {
        let provider = ProviderSettings {
            display: "Primary".into(),
            api_key: "secret-123456".into(),
            base_url: "https://example.test/v1".into(),
            model: "main".into(),
            fast_model: Some("fast".into()),
            ..ProviderSettings::default()
        };
        let value = provider_json("primary", &provider, true);
        assert_eq!(value["apiKeyMasked"], "****3456");
        assert_eq!(value["models"], json!(["main", "fast"]));
        assert_eq!(value["contextWindow"], 256_000);
        assert!(!value.to_string().contains("secret-123456"));
    }

    #[test]
    fn approval_risk_maps_to_frontend_access_values() {
        assert_eq!(approval_access("command may delete data"), "destructive");
        assert_eq!(approval_access("shell can change files"), "write");
        assert_eq!(approval_access("read metadata"), "read_only");
    }

    #[test]
    fn browser_usage_includes_session_and_context_totals() {
        let value = browser_usage_event(BrowserUsageState {
            input_tokens: 12_000,
            output_tokens: 800,
            context_used_tokens: 32_000,
            context_window_tokens: 128_000,
        });
        assert_eq!(value["usage"]["total_tokens"], 12_800);
        assert_eq!(value["usage"]["context_used_tokens"], 32_000);
        assert_eq!(value["usage"]["context_window_tokens"], 128_000);
        assert_eq!(value["usage"]["context_ratio"], 0.25);
    }

    #[test]
    fn web_prompt_does_not_include_shared_persistent_memory() {
        let home = tempfile::tempdir().expect("temporary home");
        let project = tempfile::tempdir().expect("temporary project");
        MemoryManager::new(home.path(), project.path())
            .save(
                MemoryScope::Global,
                "other-session",
                "must stay outside web sessions",
                MemoryType::User,
                "CROSS_SESSION_SENTINEL",
            )
            .expect("save shared memory");

        let prompt = system_prompt(home.path(), project.path(), AccessMode::FullAccess, "");
        assert!(!prompt.contains("CROSS_SESSION_SENTINEL"));
        assert!(!prompt.contains("Persistent memory:"));
    }

    #[test]
    fn web_session_loads_only_the_requested_history() {
        let home = tempfile::tempdir().expect("temporary home");
        let project = tempfile::tempdir().expect("temporary project");
        let store = SessionStore::new(home.path());
        let mut first = Session::new("provider", "model", project.path().to_path_buf());
        first.messages.push(ChatMessage::user("FIRST_SESSION_ONLY"));
        let mut second = Session::new("provider", "model", project.path().to_path_buf());
        second
            .messages
            .push(ChatMessage::user("SECOND_SESSION_ONLY"));
        store.save(&first).expect("save first session");
        store.save(&second).expect("save second session");

        let loaded =
            load_or_create_web_session(&store, second.id, "provider", "model", project.path());
        let serialized = serde_json::to_string(&loaded.messages).expect("serialize messages");
        assert!(serialized.contains("SECOND_SESSION_ONLY"));
        assert!(!serialized.contains("FIRST_SESSION_ONLY"));
        assert_eq!(loaded.id, second.id);
    }
}
