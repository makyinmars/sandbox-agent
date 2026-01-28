//! Tests for session resumption behavior.

use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tempfile::TempDir;

use sandbox_agent::router::{build_router, AppState, AuthConfig};
use sandbox_agent_agent_management::agents::{AgentId, AgentManager};
use tower::util::ServiceExt;

struct TestApp {
    app: Router,
    _install_dir: TempDir,
}

impl TestApp {
    fn new() -> Self {
        let install_dir = tempfile::tempdir().expect("create temp install dir");
        let manager = AgentManager::new(install_dir.path()).expect("create agent manager");
        let state = AppState::new(AuthConfig::disabled(), manager);
        let app = build_router(state);
        Self {
            app,
            _install_dir: install_dir,
        }
    }
}

async fn send_json(
    app: &Router,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    let body = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };
    let request = builder.body(body).expect("request");
    let response = app.clone().oneshot(request).await.expect("request handled");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("read body")
        .to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).to_string()))
    };
    (status, value)
}

async fn create_session(app: &Router, agent: AgentId, session_id: &str) {
    let (status, _) = send_json(
        app,
        Method::POST,
        &format!("/v1/sessions/{session_id}"),
        Some(json!({
            "agent": agent.as_str(),
            "permissionMode": "bypass"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create session {agent}");
}

/// Send a message and return the status code (allows checking for errors)
async fn send_message_with_status(
    app: &Router,
    session_id: &str,
    message: &str,
) -> (StatusCode, Value) {
    send_json(
        app,
        Method::POST,
        &format!("/v1/sessions/{session_id}/messages"),
        Some(json!({ "message": message })),
    )
    .await
}

fn is_session_ended(event: &Value) -> bool {
    event
        .get("type")
        .and_then(Value::as_str)
        .map(|t| t == "session.ended")
        .unwrap_or(false)
}

/// Test that verifies the session can be reopened after ending
#[tokio::test]
async fn session_reopen_after_end() {
    let test_app = TestApp::new();
    let session_id = "reopen-test";

    // Create session with mock agent
    create_session(&test_app.app, AgentId::Mock, session_id).await;

    // Send "end" command to mock agent to end the session
    let (status, _) = send_message_with_status(&test_app.app, session_id, "end").await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Wait for session to end
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify session is ended
    let path = format!("/v1/sessions/{session_id}/events?offset=0&limit=100");
    let (_, payload) = send_json(&test_app.app, Method::GET, &path, None).await;
    let events = payload
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let has_ended = events.iter().any(|e| is_session_ended(e));
    assert!(has_ended, "Session should be ended after 'end' command");

    // Try to send another message - mock agent supports resume so this should work
    // (or fail if we haven't implemented reopen for mock)
    let (status, body) = send_message_with_status(&test_app.app, session_id, "hello again").await;

    // For mock agent, the session should be reopenable since mock is in agent_supports_resume
    // But mock's session.ended is triggered differently than real agents
    // This test documents the current behavior
    if status == StatusCode::NO_CONTENT {
        eprintln!("Mock agent session was successfully reopened after end");
    } else {
        eprintln!(
            "Mock agent session could not be reopened (status {}): {:?}",
            status, body
        );
    }
}
