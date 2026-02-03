// Session update endpoint coverage uses the mock agent to avoid external dependencies.
include!("../common/http.rs");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_session_applies_model_variant() {
    let app = TestApp::new();
    let session_id = "update-mock";

    let status = send_status(
        &app.app,
        Method::POST,
        &format!("/v1/sessions/{session_id}"),
        Some(json!({
            "agent": "mock",
            "permissionMode": "default"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create session");

    let (status, response) = send_json(
        &app.app,
        Method::PATCH,
        &format!("/v1/sessions/{session_id}"),
        Some(json!({
            "model": "test-model",
            "variant": "test-variant"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "update session");
    assert_eq!(
        response.get("sessionId").and_then(Value::as_str),
        Some(session_id)
    );
    assert_eq!(
        response.get("model").and_then(Value::as_str),
        Some("test-model")
    );
    assert_eq!(
        response.get("variant").and_then(Value::as_str),
        Some("test-variant")
    );
    let expected_native = format!("mock-{session_id}");
    assert_eq!(
        response.get("nativeSessionId").and_then(Value::as_str),
        Some(expected_native.as_str())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_session_requires_fields() {
    let app = TestApp::new();
    let session_id = "update-empty";

    let status = send_status(
        &app.app,
        Method::POST,
        &format!("/v1/sessions/{session_id}"),
        Some(json!({
            "agent": "mock",
            "permissionMode": "default"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create session");

    let (status, _) = send_json(
        &app.app,
        Method::PATCH,
        &format!("/v1/sessions/{session_id}"),
        Some(json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "empty update rejected");
}
