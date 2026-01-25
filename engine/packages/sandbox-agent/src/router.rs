use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::sse::Event;
use axum::response::{IntoResponse, Response, Sse};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use futures::{stream, StreamExt};
use reqwest::Client;
use sandbox_agent_error::{AgentError, ErrorType, ProblemDetails, SandboxError};
use sandbox_agent_universal_agent_schema::{
    convert_amp, convert_claude, convert_codex, convert_opencode, AttachmentSource, CrashInfo,
    EventConversion, PermissionRequest, PermissionToolRef, QuestionInfo, QuestionOption,
    QuestionRequest, QuestionToolRef, Started, UniversalEvent, UniversalEventData,
    UniversalMessage, UniversalMessageParsed, UniversalMessagePart,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_stream::wrappers::BroadcastStream;
use tokio::time::sleep;
use utoipa::{OpenApi, ToSchema};

use sandbox_agent_agent_management::agents::{
    AgentError as ManagerError, AgentId, AgentManager, InstallOptions, SpawnOptions, StreamingSpawn,
};
use sandbox_agent_agent_management::credentials::{
    extract_all_credentials, CredentialExtractionOptions, ExtractedCredentials,
};

#[derive(Debug)]
pub struct AppState {
    auth: AuthConfig,
    agent_manager: Arc<AgentManager>,
    session_manager: Arc<SessionManager>,
}

impl AppState {
    pub fn new(auth: AuthConfig, agent_manager: AgentManager) -> Self {
        let agent_manager = Arc::new(agent_manager);
        let session_manager = Arc::new(SessionManager::new(agent_manager.clone()));
        Self {
            auth,
            agent_manager,
            session_manager,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub token: Option<String>,
}

impl AuthConfig {
    pub fn disabled() -> Self {
        Self { token: None }
    }

    pub fn with_token(token: String) -> Self {
        Self { token: Some(token) }
    }
}

pub fn build_router(state: AppState) -> Router {
    let shared = Arc::new(state);

    let mut v1_router = Router::new()
        .route("/agents", get(list_agents))
        .route("/agents/:agent/install", post(install_agent))
        .route("/agents/:agent/modes", get(get_agent_modes))
        .route("/sessions/:session_id", post(create_session))
        .route("/sessions/:session_id/messages", post(post_message))
        .route("/sessions/:session_id/events", get(get_events))
        .route("/sessions/:session_id/events/sse", get(get_events_sse))
        .route(
            "/sessions/:session_id/questions/:question_id/reply",
            post(reply_question),
        )
        .route(
            "/sessions/:session_id/questions/:question_id/reject",
            post(reject_question),
        )
        .route(
            "/sessions/:session_id/permissions/:permission_id/reply",
            post(reply_permission),
        )
        .with_state(shared.clone());

    if shared.auth.token.is_some() {
        v1_router = v1_router.layer(axum::middleware::from_fn_with_state(shared, require_token));
    }

    Router::new().nest("/v1", v1_router)
}

#[derive(OpenApi)]
#[openapi(
    paths(
        install_agent,
        get_agent_modes,
        list_agents,
        create_session,
        post_message,
        get_events,
        get_events_sse,
        reply_question,
        reject_question,
        reply_permission
    ),
    components(
        schemas(
            AgentInstallRequest,
            AgentModeInfo,
            AgentModesResponse,
            AgentInfo,
            AgentListResponse,
            CreateSessionRequest,
            CreateSessionResponse,
            MessageRequest,
            EventsQuery,
            EventsResponse,
            UniversalEvent,
            UniversalEventData,
            UniversalMessage,
            UniversalMessageParsed,
            UniversalMessagePart,
            AttachmentSource,
            Started,
            CrashInfo,
            QuestionRequest,
            QuestionInfo,
            QuestionOption,
            QuestionToolRef,
            PermissionRequest,
            PermissionToolRef,
            QuestionReplyRequest,
            PermissionReplyRequest,
            PermissionReply,
            ProblemDetails,
            ErrorType,
            AgentError
        )
    ),
    tags(
        (name = "agents", description = "Agent management"),
        (name = "sessions", description = "Session management")
    )
)]
pub struct ApiDoc;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Sandbox(#[from] SandboxError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let problem: ProblemDetails = match &self {
            ApiError::Sandbox(err) => err.to_problem_details(),
        };
        let status = StatusCode::from_u16(problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(problem)).into_response()
    }
}

#[derive(Debug)]
struct SessionState {
    session_id: String,
    agent: AgentId,
    agent_mode: String,
    permission_mode: String,
    model: Option<String>,
    variant: Option<String>,
    agent_session_id: Option<String>,
    ended: bool,
    ended_exit_code: Option<i32>,
    ended_message: Option<String>,
    next_event_id: u64,
    events: Vec<UniversalEvent>,
    pending_questions: HashSet<String>,
    pending_permissions: HashSet<String>,
    broadcaster: broadcast::Sender<UniversalEvent>,
    opencode_stream_started: bool,
}

impl SessionState {
    fn new(
        session_id: String,
        agent: AgentId,
        request: &CreateSessionRequest,
    ) -> Result<Self, SandboxError> {
        let (agent_mode, permission_mode) = normalize_modes(
            agent,
            request.agent_mode.as_deref(),
            request.permission_mode.as_deref(),
        )?;
        let (broadcaster, _rx) = broadcast::channel(256);

        Ok(Self {
            session_id,
            agent,
            agent_mode,
            permission_mode,
            model: request.model.clone(),
            variant: request.variant.clone(),
            agent_session_id: None,
            ended: false,
            ended_exit_code: None,
            ended_message: None,
            next_event_id: 0,
            events: Vec::new(),
            pending_questions: HashSet::new(),
            pending_permissions: HashSet::new(),
            broadcaster,
            opencode_stream_started: false,
        })
    }

    fn record_conversion(&mut self, conversion: EventConversion) -> UniversalEvent {
        let agent_session_id = conversion
            .agent_session_id
            .clone()
            .or_else(|| self.agent_session_id.clone());
        if self.agent_session_id.is_none() {
            self.agent_session_id = conversion.agent_session_id.clone();
        }
        self.record_event(conversion.data, agent_session_id)
    }

    fn record_event(
        &mut self,
        data: UniversalEventData,
        agent_session_id: Option<String>,
    ) -> UniversalEvent {
        self.next_event_id += 1;
        let data = self.normalize_event_data(data);
        let event = UniversalEvent {
            id: self.next_event_id,
            timestamp: now_rfc3339(),
            session_id: self.session_id.clone(),
            agent: self.agent.as_str().to_string(),
            agent_session_id: agent_session_id.clone(),
            data,
        };
        self.update_pending(&event);
        self.events.push(event.clone());
        let _ = self.broadcaster.send(event.clone());
        if self.agent_session_id.is_none() {
            self.agent_session_id = agent_session_id;
        }
        event
    }

    fn normalize_event_data(&self, mut data: UniversalEventData) -> UniversalEventData {
        match &mut data {
            UniversalEventData::QuestionAsked { question_asked } => {
                question_asked.session_id = self.session_id.clone();
            }
            UniversalEventData::PermissionAsked { permission_asked } => {
                permission_asked.session_id = self.session_id.clone();
            }
            _ => {}
        }
        data
    }

    fn update_pending(&mut self, event: &UniversalEvent) {
        match &event.data {
            UniversalEventData::QuestionAsked { question_asked } => {
                self.pending_questions.insert(question_asked.id.clone());
            }
            UniversalEventData::PermissionAsked { permission_asked } => {
                self.pending_permissions
                    .insert(permission_asked.id.clone());
            }
            _ => {}
        }
    }

    fn take_question(&mut self, question_id: &str) -> bool {
        self.pending_questions.remove(question_id)
    }

    fn take_permission(&mut self, permission_id: &str) -> bool {
        self.pending_permissions.remove(permission_id)
    }

    fn mark_ended(&mut self, exit_code: Option<i32>, message: String) {
        self.ended = true;
        self.ended_exit_code = exit_code;
        self.ended_message = Some(message);
    }

    fn ended_error(&self) -> Option<SandboxError> {
        if !self.ended {
            return None;
        }
        Some(SandboxError::AgentProcessExited {
            agent: self.agent.as_str().to_string(),
            exit_code: self.ended_exit_code,
            stderr: self.ended_message.clone(),
        })
    }
}

#[derive(Debug)]
struct SessionManager {
    agent_manager: Arc<AgentManager>,
    sessions: Mutex<HashMap<String, SessionState>>,
    opencode_server: Mutex<Option<OpencodeServer>>,
    http_client: Client,
}

#[derive(Debug)]
struct OpencodeServer {
    base_url: String,
    #[allow(dead_code)]
    child: Option<std::process::Child>,
}

struct SessionSubscription {
    initial_events: Vec<UniversalEvent>,
    receiver: broadcast::Receiver<UniversalEvent>,
}

impl SessionManager {
    fn new(agent_manager: Arc<AgentManager>) -> Self {
        Self {
            agent_manager,
            sessions: Mutex::new(HashMap::new()),
            opencode_server: Mutex::new(None),
            http_client: Client::new(),
        }
    }

    async fn create_session(
        self: &Arc<Self>,
        session_id: String,
        request: CreateSessionRequest,
    ) -> Result<CreateSessionResponse, SandboxError> {
        let agent_id = parse_agent_id(&request.agent)?;
        {
            let sessions = self.sessions.lock().await;
            if sessions.contains_key(&session_id) {
                return Err(SandboxError::SessionAlreadyExists { session_id });
            }
        }

        let manager = self.agent_manager.clone();
        let agent_version = request.agent_version.clone();
        let agent_name = request.agent.clone();
        let install_result = tokio::task::spawn_blocking(move || {
            manager.install(
                agent_id,
                InstallOptions {
                    reinstall: false,
                    version: agent_version,
                },
            )
        })
        .await
        .map_err(|err| SandboxError::InstallFailed {
            agent: agent_name,
            stderr: Some(err.to_string()),
        })?;
        install_result.map_err(|err| map_install_error(agent_id, err))?;

        let mut session = SessionState::new(session_id.clone(), agent_id, &request)?;
        if agent_id == AgentId::Opencode {
            let opencode_session_id = self.create_opencode_session().await?;
            session.agent_session_id = Some(opencode_session_id);
        }

        let started = Started {
            message: Some("session.created".to_string()),
            details: None,
        };
        session.record_event(
            UniversalEventData::Started { started },
            session.agent_session_id.clone(),
        );

        let agent_session_id = session.agent_session_id.clone();
        let mut sessions = self.sessions.lock().await;
        sessions.insert(session_id.clone(), session);
        drop(sessions);

        if agent_id == AgentId::Opencode {
            self.ensure_opencode_stream(session_id).await?;
        }

        Ok(CreateSessionResponse {
            healthy: true,
            error: None,
            agent_session_id,
        })
    }

    async fn agent_modes(&self, agent: AgentId) -> Result<Vec<AgentModeInfo>, SandboxError> {
        if agent != AgentId::Opencode {
            return Ok(agent_modes_for(agent));
        }

        match self.fetch_opencode_modes().await {
            Ok(mut modes) => {
                ensure_custom_mode(&mut modes);
                if modes.is_empty() {
                    Ok(agent_modes_for(agent))
                } else {
                    Ok(modes)
                }
            }
            Err(_) => Ok(agent_modes_for(agent)),
        }
    }

    async fn send_message(
        self: &Arc<Self>,
        session_id: String,
        message: String,
    ) -> Result<(), SandboxError> {
        let session_snapshot = self.session_snapshot(&session_id, false).await?;
        if session_snapshot.agent == AgentId::Opencode {
            self.ensure_opencode_stream(session_id.clone()).await?;
            self.send_opencode_prompt(&session_snapshot, &message).await?;
            return Ok(());
        }

        let manager = self.agent_manager.clone();
        let prompt = message;
        let credentials = tokio::task::spawn_blocking(move || {
            let options = CredentialExtractionOptions::new();
            extract_all_credentials(&options)
        })
        .await
        .map_err(|err| SandboxError::StreamError {
            message: err.to_string(),
        })?;

        let spawn_options = build_spawn_options(&session_snapshot, prompt, credentials);
        let agent_id = session_snapshot.agent;
        let spawn_result = tokio::task::spawn_blocking(move || manager.spawn_streaming(agent_id, spawn_options))
            .await
            .map_err(|err| SandboxError::StreamError {
                message: err.to_string(),
            })?;

        let spawn_result = spawn_result.map_err(|err| map_spawn_error(agent_id, err))?;
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            manager
                .consume_spawn(session_id, agent_id, spawn_result)
                .await;
        });

        Ok(())
    }

    async fn events(
        &self,
        session_id: &str,
        offset: u64,
        limit: Option<u64>,
    ) -> Result<EventsResponse, SandboxError> {
        let sessions = self.sessions.lock().await;
        let session = sessions.get(session_id).ok_or_else(|| SandboxError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;

        let mut events: Vec<UniversalEvent> = session
            .events
            .iter()
            .filter(|event| event.id > offset)
            .cloned()
            .collect();

        let has_more = if let Some(limit) = limit {
            let limit = limit as usize;
            if events.len() > limit {
                events.truncate(limit);
                true
            } else {
                false
            }
        } else {
            false
        };

        Ok(EventsResponse { events, has_more })
    }

    async fn subscribe(
        &self,
        session_id: &str,
        offset: u64,
    ) -> Result<SessionSubscription, SandboxError> {
        let sessions = self.sessions.lock().await;
        let session = sessions.get(session_id).ok_or_else(|| SandboxError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        let initial_events = session
            .events
            .iter()
            .filter(|event| event.id > offset)
            .cloned()
            .collect::<Vec<_>>();
        let receiver = session.broadcaster.subscribe();
        Ok(SessionSubscription {
            initial_events,
            receiver,
        })
    }

    async fn reply_question(
        &self,
        session_id: &str,
        question_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<(), SandboxError> {
        let (agent, agent_session_id) = {
            let mut sessions = self.sessions.lock().await;
            let session = sessions.get_mut(session_id).ok_or_else(|| SandboxError::SessionNotFound {
                session_id: session_id.to_string(),
            })?;
            if let Some(err) = session.ended_error() {
                return Err(err);
            }
            if !session.take_question(question_id) {
                return Err(SandboxError::InvalidRequest {
                    message: format!("unknown question id: {question_id}"),
                });
            }
            (session.agent, session.agent_session_id.clone())
        };

        if agent == AgentId::Opencode {
            let agent_session_id = agent_session_id.ok_or_else(|| SandboxError::InvalidRequest {
                message: "missing OpenCode session id".to_string(),
            })?;
            self.opencode_question_reply(&agent_session_id, question_id, answers)
                .await?;
        } else {
            // TODO: Forward question replies to subprocess agents.
        }

        Ok(())
    }

    async fn reject_question(
        &self,
        session_id: &str,
        question_id: &str,
    ) -> Result<(), SandboxError> {
        let (agent, agent_session_id) = {
            let mut sessions = self.sessions.lock().await;
            let session = sessions.get_mut(session_id).ok_or_else(|| SandboxError::SessionNotFound {
                session_id: session_id.to_string(),
            })?;
            if let Some(err) = session.ended_error() {
                return Err(err);
            }
            if !session.take_question(question_id) {
                return Err(SandboxError::InvalidRequest {
                    message: format!("unknown question id: {question_id}"),
                });
            }
            (session.agent, session.agent_session_id.clone())
        };

        if agent == AgentId::Opencode {
            let agent_session_id = agent_session_id.ok_or_else(|| SandboxError::InvalidRequest {
                message: "missing OpenCode session id".to_string(),
            })?;
            self.opencode_question_reject(&agent_session_id, question_id)
                .await?;
        } else {
            // TODO: Forward question rejections to subprocess agents.
        }

        Ok(())
    }

    async fn reply_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        reply: PermissionReply,
    ) -> Result<(), SandboxError> {
        let (agent, agent_session_id) = {
            let mut sessions = self.sessions.lock().await;
            let session = sessions.get_mut(session_id).ok_or_else(|| SandboxError::SessionNotFound {
                session_id: session_id.to_string(),
            })?;
            if let Some(err) = session.ended_error() {
                return Err(err);
            }
            if !session.take_permission(permission_id) {
                return Err(SandboxError::InvalidRequest {
                    message: format!("unknown permission id: {permission_id}"),
                });
            }
            (session.agent, session.agent_session_id.clone())
        };

        if agent == AgentId::Opencode {
            let agent_session_id = agent_session_id.ok_or_else(|| SandboxError::InvalidRequest {
                message: "missing OpenCode session id".to_string(),
            })?;
            self.opencode_permission_reply(&agent_session_id, permission_id, reply)
                .await?;
        } else {
            // TODO: Forward permission replies to subprocess agents.
        }

        Ok(())
    }

    async fn session_snapshot(
        &self,
        session_id: &str,
        allow_ended: bool,
    ) -> Result<SessionSnapshot, SandboxError> {
        let sessions = self.sessions.lock().await;
        let session = sessions.get(session_id).ok_or_else(|| SandboxError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        if !allow_ended {
            if let Some(err) = session.ended_error() {
                return Err(err);
            }
        }
        Ok(SessionSnapshot::from(session))
    }

    async fn consume_spawn(
        self: Arc<Self>,
        session_id: String,
        agent: AgentId,
        spawn: StreamingSpawn,
    ) {
        let StreamingSpawn {
            mut child,
            stdout,
            stderr,
        } = spawn;
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        if let Some(stdout) = stdout {
            let tx_stdout = tx.clone();
            tokio::task::spawn_blocking(move || {
                read_lines(stdout, tx_stdout);
            });
        }
        if let Some(stderr) = stderr {
            let tx_stderr = tx.clone();
            tokio::task::spawn_blocking(move || {
                read_lines(stderr, tx_stderr);
            });
        }
        drop(tx);

        while let Some(line) = rx.recv().await {
            if let Some(conversion) = parse_agent_line(agent, &line, &session_id) {
                let _ = self.record_conversion(&session_id, conversion).await;
            }
        }

        let status = tokio::task::spawn_blocking(move || child.wait()).await;
        match status {
            Ok(Ok(status)) if status.success() => {}
            Ok(Ok(status)) => {
                let message = format!("agent exited with status {:?}", status);
                self.record_error(
                    &session_id,
                    message.clone(),
                    Some("process_exit".to_string()),
                    None,
                )
                    .await;
                self.mark_session_ended(&session_id, status.code(), &message)
                    .await;
            }
            Ok(Err(err)) => {
                let message = format!("failed to wait for agent: {err}");
                self.record_error(
                    &session_id,
                    message.clone(),
                    Some("process_wait_failed".to_string()),
                    None,
                )
                .await;
                self.mark_session_ended(
                    &session_id,
                    None,
                    &message,
                )
                .await;
            }
            Err(err) => {
                let message = format!("failed to join agent task: {err}");
                self.record_error(
                    &session_id,
                    message.clone(),
                    Some("process_wait_failed".to_string()),
                    None,
                )
                .await;
                self.mark_session_ended(
                    &session_id,
                    None,
                    &message,
                )
                .await;
            }
        }
    }

    async fn record_conversion(
        &self,
        session_id: &str,
        conversion: EventConversion,
    ) -> Result<UniversalEvent, SandboxError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(session_id).ok_or_else(|| SandboxError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        Ok(session.record_conversion(conversion))
    }

    async fn record_event(
        &self,
        session_id: &str,
        data: UniversalEventData,
        agent_session_id: Option<String>,
    ) -> Result<UniversalEvent, SandboxError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(session_id).ok_or_else(|| SandboxError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        Ok(session.record_event(data, agent_session_id))
    }

    async fn record_error(
        &self,
        session_id: &str,
        message: String,
        kind: Option<String>,
        details: Option<Value>,
    ) {
        let error = CrashInfo { message, kind, details };
        let _ = self
            .record_event(
                session_id,
                UniversalEventData::Error { error },
                None,
            )
            .await;
    }

    async fn mark_session_ended(&self, session_id: &str, exit_code: Option<i32>, message: &str) {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(session_id) {
            if session.ended {
                return;
            }
            session.mark_ended(exit_code, message.to_string());
        }
    }

    async fn ensure_opencode_stream(self: &Arc<Self>, session_id: String) -> Result<(), SandboxError> {
        let agent_session_id = {
            let mut sessions = self.sessions.lock().await;
            let session = sessions.get_mut(&session_id).ok_or_else(|| SandboxError::SessionNotFound {
                session_id: session_id.clone(),
            })?;
            if session.opencode_stream_started {
                return Ok(());
            }
            let agent_session_id = session.agent_session_id.clone().ok_or_else(|| SandboxError::InvalidRequest {
                message: "missing OpenCode session id".to_string(),
            })?;
            session.opencode_stream_started = true;
            agent_session_id
        };

        let manager = Arc::clone(self);
        tokio::spawn(async move {
            manager
                .stream_opencode_events(session_id, agent_session_id)
                .await;
        });

        Ok(())
    }

    async fn stream_opencode_events(self: Arc<Self>, session_id: String, agent_session_id: String) {
        let base_url = match self.ensure_opencode_server().await {
            Ok(base_url) => base_url,
            Err(err) => {
                self.record_error(
                    &session_id,
                    format!("failed to start OpenCode server: {err}"),
                    Some("opencode_server".to_string()),
                    None,
                )
                .await;
                self.mark_session_ended(
                    &session_id,
                    None,
                    "opencode server unavailable",
                )
                .await;
                return;
            }
        };

        let url = format!("{base_url}/event/subscribe");
        let response = match self.http_client.get(url).send().await {
            Ok(response) => response,
            Err(err) => {
                self.record_error(
                    &session_id,
                    format!("OpenCode SSE connection failed: {err}"),
                    Some("opencode_stream".to_string()),
                    None,
                )
                .await;
                self.mark_session_ended(
                    &session_id,
                    None,
                    "opencode sse connection failed",
                )
                .await;
                return;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            self.record_error(
                &session_id,
                format!("OpenCode SSE error {status}: {body}"),
                Some("opencode_stream".to_string()),
                None,
            )
            .await;
            self.mark_session_ended(
                &session_id,
                None,
                "opencode sse error",
            )
            .await;
            return;
        }

        let mut accumulator = SseAccumulator::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(err) => {
                    self.record_error(
                        &session_id,
                        format!("OpenCode SSE stream error: {err}"),
                        Some("opencode_stream".to_string()),
                        None,
                    )
                    .await;
                    self.mark_session_ended(
                        &session_id,
                        None,
                        "opencode sse stream error",
                    )
                    .await;
                    return;
                }
            };
            let text = String::from_utf8_lossy(&chunk);
            for event_payload in accumulator.push(&text) {
                let value: Value = match serde_json::from_str(&event_payload) {
                    Ok(value) => value,
                    Err(err) => {
                        let conversion = EventConversion::new(unparsed_message(
                            &event_payload,
                            &err.to_string(),
                        ));
                        let _ = self.record_conversion(&session_id, conversion).await;
                        continue;
                    }
                };
                if !opencode_event_matches_session(&value, &agent_session_id) {
                    continue;
                }
                let conversion = match serde_json::from_value(value.clone()) {
                    Ok(event) => convert_opencode::event_to_universal(&event),
                    Err(err) => EventConversion::new(unparsed_message(
                        &value.to_string(),
                        &err.to_string(),
                    )),
                };
                let _ = self.record_conversion(&session_id, conversion).await;
            }
        }
    }

    async fn ensure_opencode_server(&self) -> Result<String, SandboxError> {
        {
            let guard = self.opencode_server.lock().await;
            if let Some(server) = guard.as_ref() {
                return Ok(server.base_url.clone());
            }
        }

        let manager = self.agent_manager.clone();
        let server = tokio::task::spawn_blocking(move || -> Result<OpencodeServer, SandboxError> {
            let path = manager
                .resolve_binary(AgentId::Opencode)
                .map_err(|err| map_spawn_error(AgentId::Opencode, err))?;
            let port = find_available_port()?;
            let mut command = std::process::Command::new(path);
            command
                .arg("serve")
                .arg("--port")
                .arg(port.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let child = command.spawn().map_err(|err| SandboxError::StreamError {
                message: err.to_string(),
            })?;
            Ok(OpencodeServer {
                base_url: format!("http://127.0.0.1:{port}"),
                child: Some(child),
            })
        })
        .await
        .map_err(|err| SandboxError::StreamError {
            message: err.to_string(),
        })??;

        {
            let mut guard = self.opencode_server.lock().await;
            if let Some(existing) = guard.as_ref() {
                return Ok(existing.base_url.clone());
            }
            *guard = Some(server);
        }
        let guard = self.opencode_server.lock().await;
        guard
            .as_ref()
            .map(|server| server.base_url.clone())
            .ok_or_else(|| SandboxError::StreamError {
                message: "OpenCode server missing".to_string(),
            })
    }

    async fn fetch_opencode_modes(&self) -> Result<Vec<AgentModeInfo>, SandboxError> {
        let base_url = self.ensure_opencode_server().await?;
        let endpoints = [format!("{base_url}/app/agents"), format!("{base_url}/agents")];
        for url in endpoints {
            let response = self.http_client.get(&url).send().await;
            let response = match response {
                Ok(response) => response,
                Err(_) => continue,
            };
            if !response.status().is_success() {
                continue;
            }
            let value: Value = response.json().await.map_err(|err| SandboxError::StreamError {
                message: err.to_string(),
            })?;
            let modes = parse_opencode_modes(&value);
            if !modes.is_empty() {
                return Ok(modes);
            }
        }
        Err(SandboxError::StreamError {
            message: "OpenCode agent modes unavailable".to_string(),
        })
    }

    async fn create_opencode_session(&self) -> Result<String, SandboxError> {
        let base_url = self.ensure_opencode_server().await?;
        let url = format!("{base_url}/session");
        for _ in 0..10 {
            let response = self
                .http_client
                .post(&url)
                .json(&json!({}))
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(_) => {
                    sleep(Duration::from_millis(200)).await;
                    continue;
                }
            };
            if !response.status().is_success() {
                sleep(Duration::from_millis(200)).await;
                continue;
            }
            let value: Value = response.json().await.map_err(|err| SandboxError::StreamError {
                message: err.to_string(),
            })?;
            if let Some(id) = value.get("id").and_then(Value::as_str) {
                return Ok(id.to_string());
            }
            if let Some(id) = value.get("sessionId").and_then(Value::as_str) {
                return Ok(id.to_string());
            }
            if let Some(id) = value.get("session_id").and_then(Value::as_str) {
                return Ok(id.to_string());
            }
            return Err(SandboxError::StreamError {
                message: format!("OpenCode session response missing id: {value}"),
            });
        }
        Err(SandboxError::StreamError {
            message: "OpenCode session create failed after retries".to_string(),
        })
    }

    async fn send_opencode_prompt(
        &self,
        session: &SessionSnapshot,
        prompt: &str,
    ) -> Result<(), SandboxError> {
        let base_url = self.ensure_opencode_server().await?;
        let session_id = session.agent_session_id.as_ref().ok_or_else(|| SandboxError::InvalidRequest {
            message: "missing OpenCode session id".to_string(),
        })?;
        let url = format!("{base_url}/session/{session_id}/prompt");
        let mut body = json!({
            "agent": session.agent_mode.clone(),
            "parts": [{ "type": "text", "text": prompt }]
        });
        if let Some(model) = session.model.as_deref() {
            if let Some((provider, model_id)) = model.split_once('/') {
                body["model"] = json!({
                    "providerID": provider,
                    "modelID": model_id
                });
            } else {
                body["model"] = json!({ "modelID": model });
            }
        }
        if let Some(variant) = session.variant.as_deref() {
            body["variant"] = json!(variant);
        }

        let response = self
            .http_client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|err| SandboxError::StreamError {
                message: err.to_string(),
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SandboxError::StreamError {
                message: format!("OpenCode prompt failed {status}: {body}"),
            });
        }

        Ok(())
    }

    async fn opencode_question_reply(
        &self,
        _session_id: &str,
        request_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<(), SandboxError> {
        let base_url = self.ensure_opencode_server().await?;
        let url = format!("{base_url}/question/reply");
        let response = self
            .http_client
            .post(url)
            .json(&json!({
                "requestID": request_id,
                "answers": answers
            }))
            .send()
            .await
            .map_err(|err| SandboxError::StreamError {
                message: err.to_string(),
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SandboxError::StreamError {
                message: format!("OpenCode question reply failed {status}: {body}"),
            });
        }
        Ok(())
    }

    async fn opencode_question_reject(
        &self,
        _session_id: &str,
        request_id: &str,
    ) -> Result<(), SandboxError> {
        let base_url = self.ensure_opencode_server().await?;
        let url = format!("{base_url}/question/reject");
        let response = self
            .http_client
            .post(url)
            .json(&json!({ "requestID": request_id }))
            .send()
            .await
            .map_err(|err| SandboxError::StreamError {
                message: err.to_string(),
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SandboxError::StreamError {
                message: format!("OpenCode question reject failed {status}: {body}"),
            });
        }
        Ok(())
    }

    async fn opencode_permission_reply(
        &self,
        _session_id: &str,
        request_id: &str,
        reply: PermissionReply,
    ) -> Result<(), SandboxError> {
        let base_url = self.ensure_opencode_server().await?;
        let url = format!("{base_url}/permission/reply");
        let response = self
            .http_client
            .post(url)
            .json(&json!({
                "requestID": request_id,
                "reply": reply
            }))
            .send()
            .await
            .map_err(|err| SandboxError::StreamError {
                message: err.to_string(),
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SandboxError::StreamError {
                message: format!("OpenCode permission reply failed {status}: {body}"),
            });
        }
        Ok(())
    }
}

async fn require_token(
    State(state): State<Arc<AppState>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let expected = match &state.auth.token {
        Some(token) => token.as_str(),
        None => return Ok(next.run(req).await),
    };

    let provided = extract_token(req.headers());
    if provided.as_deref() == Some(expected) {
        Ok(next.run(req).await)
    } else {
        Err(SandboxError::TokenInvalid {
            message: Some("missing or invalid token".to_string()),
        }
        .into())
    }
}

fn extract_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(value) = value.to_str() {
            let value = value.trim();
            if let Some(stripped) = value.strip_prefix("Bearer ") {
                return Some(stripped.to_string());
            }
            if let Some(stripped) = value.strip_prefix("Token ") {
                return Some(stripped.to_string());
            }
        }
    }

    if let Some(value) = headers.get("x-sandbox-token") {
        if let Ok(value) = value.to_str() {
            return Some(value.to_string());
        }
    }

    None
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentInstallRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reinstall: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentModeInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentModesResponse {
    pub modes: Vec<AgentModeInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub id: String,
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentListResponse {
    pub agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionResponse {
    pub healthy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventsQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventsResponse {
    pub events: Vec<UniversalEvent>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestionReplyRequest {
    pub answers: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermissionReplyRequest {
    pub reply: PermissionReply,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PermissionReply {
    Once,
    Always,
    Reject,
}

impl std::str::FromStr for PermissionReply {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "once" => Ok(Self::Once),
            "always" => Ok(Self::Always),
            "reject" => Ok(Self::Reject),
            _ => Err(format!("invalid permission reply: {value}")),
        }
    }
}

#[utoipa::path(
    post,
    path = "/v1/agents/{agent}/install",
    request_body = AgentInstallRequest,
    responses(
        (status = 204, description = "Agent installed"),
        (status = 400, body = ProblemDetails),
        (status = 404, body = ProblemDetails),
        (status = 500, body = ProblemDetails)
    ),
    params(("agent" = String, Path, description = "Agent id")),
    tag = "agents"
)]
async fn install_agent(
    State(state): State<Arc<AppState>>,
    Path(agent): Path<String>,
    Json(request): Json<AgentInstallRequest>,
) -> Result<StatusCode, ApiError> {
    let agent_id = parse_agent_id(&agent)?;
    let reinstall = request.reinstall.unwrap_or(false);
    let manager = state.agent_manager.clone();

    let result = tokio::task::spawn_blocking(move || {
        manager.install(
            agent_id,
            InstallOptions {
                reinstall,
                version: None,
            },
        )
    })
    .await
    .map_err(|err| SandboxError::InstallFailed {
        agent: agent.clone(),
        stderr: Some(err.to_string()),
    })?;

    result.map_err(|err| map_install_error(agent_id, err))?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/v1/agents/{agent}/modes",
    responses(
        (status = 200, body = AgentModesResponse),
        (status = 400, body = ProblemDetails)
    ),
    params(("agent" = String, Path, description = "Agent id")),
    tag = "agents"
)]
async fn get_agent_modes(
    State(state): State<Arc<AppState>>,
    Path(agent): Path<String>,
) -> Result<Json<AgentModesResponse>, ApiError> {
    let agent_id = parse_agent_id(&agent)?;
    let modes = state.session_manager.agent_modes(agent_id).await?;
    Ok(Json(AgentModesResponse { modes }))
}

#[utoipa::path(
    get,
    path = "/v1/agents",
    responses((status = 200, body = AgentListResponse)),
    tag = "agents"
)]
async fn list_agents(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AgentListResponse>, ApiError> {
    let manager = state.agent_manager.clone();
    let agents = tokio::task::spawn_blocking(move || {
        all_agents()
            .into_iter()
            .map(|agent_id| {
                let installed = manager.is_installed(agent_id);
                let version = manager.version(agent_id).ok().flatten();
                let path = manager.resolve_binary(agent_id).ok();
                AgentInfo {
                    id: agent_id.as_str().to_string(),
                    installed,
                    version,
                    path: path.map(|path| path.to_string_lossy().to_string()),
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|err| SandboxError::StreamError {
        message: err.to_string(),
    })?;

    Ok(Json(AgentListResponse { agents }))
}

#[utoipa::path(
    post,
    path = "/v1/sessions/{session_id}",
    request_body = CreateSessionRequest,
    responses(
        (status = 200, body = CreateSessionResponse),
        (status = 400, body = ProblemDetails),
        (status = 409, body = ProblemDetails)
    ),
    params(("session_id" = String, Path, description = "Client session id")),
    tag = "sessions"
)]
async fn create_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, ApiError> {
    let response = state
        .session_manager
        .create_session(session_id, request)
        .await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/v1/sessions/{session_id}/messages",
    request_body = MessageRequest,
    responses(
        (status = 204, description = "Message accepted"),
        (status = 404, body = ProblemDetails)
    ),
    params(("session_id" = String, Path, description = "Session id")),
    tag = "sessions"
)]
async fn post_message(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<MessageRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .session_manager
        .send_message(session_id, request.message)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/v1/sessions/{session_id}/events",
    params(
        ("session_id" = String, Path, description = "Session id"),
        ("offset" = Option<u64>, Query, description = "Last seen event id (exclusive)"),
        ("limit" = Option<u64>, Query, description = "Max events to return")
    ),
    responses(
        (status = 200, body = EventsResponse),
        (status = 404, body = ProblemDetails)
    ),
    tag = "sessions"
)]
async fn get_events(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<EventsResponse>, ApiError> {
    let offset = query.offset.unwrap_or(0);
    let response = state
        .session_manager
        .events(&session_id, offset, query.limit)
        .await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/v1/sessions/{session_id}/events/sse",
    params(
        ("session_id" = String, Path, description = "Session id"),
        ("offset" = Option<u64>, Query, description = "Last seen event id (exclusive)")
    ),
    responses((status = 200, description = "SSE event stream")),
    tag = "sessions"
)]
async fn get_events_sse(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<EventsQuery>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let offset = query.offset.unwrap_or(0);
    let subscription = state
        .session_manager
        .subscribe(&session_id, offset)
        .await?;
    let initial_events = subscription.initial_events;
    let receiver = subscription.receiver;

    let initial_stream = stream::iter(initial_events.into_iter().map(|event| {
        Ok::<Event, Infallible>(to_sse_event(event))
    }));

    let live_stream = BroadcastStream::new(receiver).filter_map(|result| async move {
        match result {
            Ok(event) => Some(Ok::<Event, Infallible>(to_sse_event(event))),
            Err(_) => None,
        }
    });

    let stream = initial_stream.chain(live_stream);
    Ok(Sse::new(stream))
}

#[utoipa::path(
    post,
    path = "/v1/sessions/{session_id}/questions/{question_id}/reply",
    request_body = QuestionReplyRequest,
    responses(
        (status = 204, description = "Question answered"),
        (status = 404, body = ProblemDetails)
    ),
    params(
        ("session_id" = String, Path, description = "Session id"),
        ("question_id" = String, Path, description = "Question id")
    ),
    tag = "sessions"
)]
async fn reply_question(
    State(state): State<Arc<AppState>>,
    Path((session_id, question_id)): Path<(String, String)>,
    Json(request): Json<QuestionReplyRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .session_manager
        .reply_question(&session_id, &question_id, request.answers)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/v1/sessions/{session_id}/questions/{question_id}/reject",
    responses(
        (status = 204, description = "Question rejected"),
        (status = 404, body = ProblemDetails)
    ),
    params(
        ("session_id" = String, Path, description = "Session id"),
        ("question_id" = String, Path, description = "Question id")
    ),
    tag = "sessions"
)]
async fn reject_question(
    State(state): State<Arc<AppState>>,
    Path((session_id, question_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .session_manager
        .reject_question(&session_id, &question_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/v1/sessions/{session_id}/permissions/{permission_id}/reply",
    request_body = PermissionReplyRequest,
    responses(
        (status = 204, description = "Permission reply accepted"),
        (status = 404, body = ProblemDetails)
    ),
    params(
        ("session_id" = String, Path, description = "Session id"),
        ("permission_id" = String, Path, description = "Permission id")
    ),
    tag = "sessions"
)]
async fn reply_permission(
    State(state): State<Arc<AppState>>,
    Path((session_id, permission_id)): Path<(String, String)>,
    Json(request): Json<PermissionReplyRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .session_manager
        .reply_permission(&session_id, &permission_id, request.reply)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn all_agents() -> [AgentId; 4] {
    [
        AgentId::Claude,
        AgentId::Codex,
        AgentId::Opencode,
        AgentId::Amp,
    ]
}

fn parse_agent_id(agent: &str) -> Result<AgentId, SandboxError> {
    AgentId::parse(agent).ok_or_else(|| SandboxError::UnsupportedAgent {
        agent: agent.to_string(),
    })
}

fn agent_modes_for(agent: AgentId) -> Vec<AgentModeInfo> {
    match agent {
        AgentId::Opencode => vec![
            AgentModeInfo {
                id: "build".to_string(),
                name: "Build".to_string(),
                description: "Default build mode".to_string(),
            },
            AgentModeInfo {
                id: "plan".to_string(),
                name: "Plan".to_string(),
                description: "Planning mode".to_string(),
            },
            AgentModeInfo {
                id: "custom".to_string(),
                name: "Custom".to_string(),
                description: "Any user-defined OpenCode agent name".to_string(),
            },
        ],
        AgentId::Codex => vec![
            AgentModeInfo {
                id: "build".to_string(),
                name: "Build".to_string(),
                description: "Default build mode".to_string(),
            },
            AgentModeInfo {
                id: "plan".to_string(),
                name: "Plan".to_string(),
                description: "Planning mode via prompt prefix".to_string(),
            },
        ],
        AgentId::Claude => vec![
            AgentModeInfo {
                id: "build".to_string(),
                name: "Build".to_string(),
                description: "Default build mode".to_string(),
            },
            AgentModeInfo {
                id: "plan".to_string(),
                name: "Plan".to_string(),
                description: "Plan mode (requires permissionMode=plan)".to_string(),
            },
        ],
        AgentId::Amp => vec![AgentModeInfo {
            id: "build".to_string(),
            name: "Build".to_string(),
            description: "Default build mode".to_string(),
        }],
    }
}

fn normalize_agent_mode(agent: AgentId, agent_mode: Option<&str>) -> Result<String, SandboxError> {
    let mode = agent_mode.unwrap_or("build");
    match agent {
        AgentId::Opencode => Ok(mode.to_string()),
        AgentId::Codex => match mode {
            "build" | "plan" => Ok(mode.to_string()),
            value => Err(SandboxError::ModeNotSupported {
                agent: agent.as_str().to_string(),
                mode: value.to_string(),
            }
            .into()),
        },
        AgentId::Claude => match mode {
            "build" | "plan" => Ok(mode.to_string()),
            value => Err(SandboxError::ModeNotSupported {
                agent: agent.as_str().to_string(),
                mode: value.to_string(),
            }
            .into()),
        },
        AgentId::Amp => match mode {
            "build" => Ok("build".to_string()),
            value => Err(SandboxError::ModeNotSupported {
                agent: agent.as_str().to_string(),
                mode: value.to_string(),
            }
            .into()),
        },
    }
}

fn normalize_permission_mode(
    agent: AgentId,
    permission_mode: Option<&str>,
) -> Result<String, SandboxError> {
    let mode = match permission_mode.unwrap_or("default") {
        "default" | "plan" | "bypass" => permission_mode.unwrap_or("default"),
        value => {
            return Err(SandboxError::InvalidRequest {
                message: format!("invalid permission mode: {value}"),
            }
            .into())
        }
    };
    let supported = match agent {
        AgentId::Claude | AgentId::Codex => matches!(mode, "default" | "plan" | "bypass"),
        AgentId::Amp => matches!(mode, "default" | "bypass"),
        AgentId::Opencode => matches!(mode, "default"),
    };
    if !supported {
        return Err(SandboxError::ModeNotSupported {
            agent: agent.as_str().to_string(),
            mode: mode.to_string(),
        }
        .into());
    }
    Ok(mode.to_string())
}

fn normalize_modes(
    agent: AgentId,
    agent_mode: Option<&str>,
    permission_mode: Option<&str>,
) -> Result<(String, String), SandboxError> {
    let agent_mode = normalize_agent_mode(agent, agent_mode)?;
    if agent == AgentId::Claude && agent_mode == "plan" {
        if let Some(permission_mode) = permission_mode {
            if permission_mode != "plan" {
                return Err(SandboxError::InvalidRequest {
                    message: "Claude agentMode=plan requires permissionMode=plan".to_string(),
                }
                .into());
            }
        }
        let permission_mode = normalize_permission_mode(agent, Some("plan"))?;
        return Ok((agent_mode, permission_mode));
    }
    let permission_mode = normalize_permission_mode(agent, permission_mode)?;
    Ok((agent_mode, permission_mode))
}

fn map_install_error(agent: AgentId, err: ManagerError) -> SandboxError {
    match err {
        ManagerError::UnsupportedAgent { agent } => SandboxError::UnsupportedAgent { agent },
        ManagerError::BinaryNotFound { .. } => SandboxError::AgentNotInstalled {
            agent: agent.as_str().to_string(),
        },
        ManagerError::ResumeUnsupported { agent } => SandboxError::InvalidRequest {
            message: format!("resume unsupported for {agent}"),
        },
        ManagerError::UnsupportedPlatform { .. }
        | ManagerError::DownloadFailed { .. }
        | ManagerError::Http(_)
        | ManagerError::UrlParse(_)
        | ManagerError::Io(_)
        | ManagerError::ExtractFailed(_) => SandboxError::InstallFailed {
            agent: agent.as_str().to_string(),
            stderr: Some(err.to_string()),
        },
    }
}

fn map_spawn_error(agent: AgentId, err: ManagerError) -> SandboxError {
    match err {
        ManagerError::BinaryNotFound { .. } => SandboxError::AgentNotInstalled {
            agent: agent.as_str().to_string(),
        },
        ManagerError::ResumeUnsupported { agent } => SandboxError::InvalidRequest {
            message: format!("resume unsupported for {agent}"),
        },
        _ => SandboxError::AgentProcessExited {
            agent: agent.as_str().to_string(),
            exit_code: None,
            stderr: Some(err.to_string()),
        },
    }
}

fn build_spawn_options(
    session: &SessionSnapshot,
    prompt: String,
    credentials: ExtractedCredentials,
) -> SpawnOptions {
    let mut options = SpawnOptions::new(prompt);
    options.model = session.model.clone();
    options.variant = session.variant.clone();
    options.agent_mode = Some(session.agent_mode.clone());
    options.permission_mode = Some(session.permission_mode.clone());
    options.session_id = session.agent_session_id.clone().or_else(|| {
        if session.agent == AgentId::Opencode {
            Some(session.session_id.clone())
        } else {
            None
        }
    });
    if let Some(anthropic) = credentials.anthropic {
        options
            .env
            .entry("ANTHROPIC_API_KEY".to_string())
            .or_insert(anthropic.api_key.clone());
        options
            .env
            .entry("CLAUDE_API_KEY".to_string())
            .or_insert(anthropic.api_key);
    }
    if let Some(openai) = credentials.openai {
        options
            .env
            .entry("OPENAI_API_KEY".to_string())
            .or_insert(openai.api_key.clone());
        options
            .env
            .entry("CODEX_API_KEY".to_string())
            .or_insert(openai.api_key);
    }
    options
}

fn read_lines<R: std::io::Read>(reader: R, sender: mpsc::UnboundedSender<String>) {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim_end_matches(&['\r', '\n'][..]).to_string();
                if sender.send(trimmed).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

fn parse_agent_line(agent: AgentId, line: &str, session_id: &str) -> Option<EventConversion> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: Value = match serde_json::from_str(trimmed) {
        Ok(value) => value,
        Err(err) => {
            return Some(EventConversion::new(unparsed_message(
                trimmed,
                &err.to_string(),
            )));
        }
    };
    let conversion = match agent {
        AgentId::Claude => {
            convert_claude::event_to_universal_with_session(&value, session_id.to_string())
        }
        AgentId::Codex => match serde_json::from_value(value.clone()) {
            Ok(event) => convert_codex::event_to_universal(&event),
            Err(err) => EventConversion::new(unparsed_message(
                &value.to_string(),
                &err.to_string(),
            )),
        },
        AgentId::Opencode => match serde_json::from_value(value.clone()) {
            Ok(event) => convert_opencode::event_to_universal(&event),
            Err(err) => EventConversion::new(unparsed_message(
                &value.to_string(),
                &err.to_string(),
            )),
        },
        AgentId::Amp => match serde_json::from_value(value.clone()) {
            Ok(event) => convert_amp::event_to_universal(&event),
            Err(err) => EventConversion::new(unparsed_message(
                &value.to_string(),
                &err.to_string(),
            )),
        },
    };
    Some(conversion)
}

fn opencode_event_matches_session(value: &Value, session_id: &str) -> bool {
    match extract_opencode_session_id(value) {
        Some(id) => id == session_id,
        None => false,
    }
}

fn extract_opencode_session_id(value: &Value) -> Option<String> {
    if let Some(id) = value.get("session_id").and_then(Value::as_str) {
        return Some(id.to_string());
    }
    if let Some(id) = value.get("sessionID").and_then(Value::as_str) {
        return Some(id.to_string());
    }
    if let Some(id) = value.get("sessionId").and_then(Value::as_str) {
        return Some(id.to_string());
    }
    if let Some(id) = extract_nested_string(value, &["properties", "sessionID"]) {
        return Some(id);
    }
    if let Some(id) = extract_nested_string(value, &["properties", "part", "sessionID"]) {
        return Some(id);
    }
    if let Some(id) = extract_nested_string(value, &["session", "id"]) {
        return Some(id);
    }
    if let Some(id) = extract_nested_string(value, &["properties", "session", "id"]) {
        return Some(id);
    }
    None
}

fn extract_nested_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        if let Ok(index) = key.parse::<usize>() {
            current = current.get(index)?;
        } else {
            current = current.get(*key)?;
        }
    }
    current.as_str().map(|s| s.to_string())
}

fn find_available_port() -> Result<u16, SandboxError> {
    for port in 4200..=4300 {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err(SandboxError::StreamError {
        message: "no available OpenCode port".to_string(),
    })
}

struct SseAccumulator {
    buffer: String,
    data_lines: Vec<String>,
}

impl SseAccumulator {
    fn new() -> Self {
        Self {
            buffer: String::new(),
            data_lines: Vec::new(),
        }
    }

    fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();
        while let Some(pos) = self.buffer.find('\n') {
            let mut line = self.buffer[..pos].to_string();
            self.buffer.drain(..=pos);
            if line.ends_with('\r') {
                line.pop();
            }
            if line.is_empty() {
                if !self.data_lines.is_empty() {
                    events.push(self.data_lines.join("\n"));
                    self.data_lines.clear();
                }
                continue;
            }
            if let Some(data) = line.strip_prefix("data:") {
                self.data_lines.push(data.trim_start().to_string());
            }
        }
        events
    }
}

fn parse_opencode_modes(value: &Value) -> Vec<AgentModeInfo> {
    let mut modes = Vec::new();
    let mut seen = HashSet::new();

    let items = value
        .as_array()
        .or_else(|| value.get("agents").and_then(Value::as_array))
        .or_else(|| value.get("data").and_then(Value::as_array));

    let Some(items) = items else { return modes };

    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| item.get("slug").and_then(Value::as_str))
            .or_else(|| item.get("name").and_then(Value::as_str));
        let Some(id) = id else { continue };
        if !seen.insert(id.to_string()) {
            continue;
        }
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string();
        let description = item
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        modes.push(AgentModeInfo {
            id: id.to_string(),
            name,
            description,
        });
    }

    modes
}

fn ensure_custom_mode(modes: &mut Vec<AgentModeInfo>) {
    if modes.iter().any(|mode| mode.id == "custom") {
        return;
    }
    modes.push(AgentModeInfo {
        id: "custom".to_string(),
        name: "Custom".to_string(),
        description: "Any user-defined OpenCode agent name".to_string(),
    });
}

fn unparsed_message(raw: &str, error: &str) -> UniversalEventData {
    UniversalEventData::Message {
        message: UniversalMessage::Unparsed {
            raw: Value::String(raw.to_string()),
            error: Some(error.to_string()),
        },
    }
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn to_sse_event(event: UniversalEvent) -> Event {
    Event::default()
        .json_data(&event)
        .unwrap_or_else(|_| Event::default().data("{}"))
}

#[derive(Clone, Debug)]
struct SessionSnapshot {
    session_id: String,
    agent: AgentId,
    agent_mode: String,
    permission_mode: String,
    model: Option<String>,
    variant: Option<String>,
    agent_session_id: Option<String>,
}

impl From<&SessionState> for SessionSnapshot {
    fn from(session: &SessionState) -> Self {
        Self {
            session_id: session.session_id.clone(),
            agent: session.agent,
            agent_mode: session.agent_mode.clone(),
            permission_mode: session.permission_mode.clone(),
            model: session.model.clone(),
            variant: session.variant.clone(),
            agent_session_id: session.agent_session_id.clone(),
        }
    }
}

pub fn add_token_header(headers: &mut HeaderMap, token: &str) {
    let value = format!("Bearer {token}");
    if let Ok(header) = HeaderValue::from_str(&value) {
        headers.insert(axum::http::header::AUTHORIZATION, header);
    }
}
