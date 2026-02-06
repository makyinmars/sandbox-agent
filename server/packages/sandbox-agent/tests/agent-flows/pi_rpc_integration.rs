// Pi RPC integration tests (gated via SANDBOX_TEST_PI + PATH).
include!("../common/http.rs");

<<<<<<< Updated upstream
=======
struct EnvVarGuard {
    key: String,
    previous: Option<String>,
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(&self.key, value),
            None => std::env::remove_var(&self.key),
        }
    }
}

fn set_env_var(key: &str, value: &str) -> EnvVarGuard {
    let previous = std::env::var(key).ok();
    std::env::set_var(key, value);
    EnvVarGuard {
        key: key.to_string(),
        previous,
    }
}

>>>>>>> Stashed changes
fn pi_test_config() -> Option<TestAgentConfig> {
    let configs = match test_agents_from_env() {
        Ok(configs) => configs,
        Err(err) => {
            eprintln!("Skipping Pi RPC integration test: {err}");
            return None;
        }
    };
    configs
        .into_iter()
        .find(|config| config.agent == AgentId::Pi)
}
<<<<<<< Updated upstream
=======

async fn create_pi_session_checked(app: &Router, session_id: &str) -> Value {
    let (status, payload) = send_json(
        app,
        Method::POST,
        &format!("/v1/sessions/{session_id}"),
        Some(json!({
            "agent": "pi",
            "permissionMode": test_permission_mode(AgentId::Pi),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create pi session {session_id}");
    payload
}

async fn poll_events_until_assistant_count(
    app: &Router,
    session_id: &str,
    expected_assistant_messages: usize,
    timeout: Duration,
) -> Vec<Value> {
    let start = Instant::now();
    let mut offset = 0u64;
    let mut events = Vec::new();

    while start.elapsed() < timeout {
        let path = format!("/v1/sessions/{session_id}/events?offset={offset}&limit=200");
        let (status, payload) = send_json(app, Method::GET, &path, None).await;
        assert_eq!(status, StatusCode::OK, "poll events");
        let new_events = payload
            .get("events")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        if !new_events.is_empty() {
            if let Some(last) = new_events
                .last()
                .and_then(|event| event.get("sequence"))
                .and_then(Value::as_u64)
            {
                offset = last;
            }
            events.extend(new_events);
        }

        if events.iter().any(is_unparsed_event) {
            break;
        }

        let assistant_count = events
            .iter()
            .filter(|event| is_assistant_message(event))
            .count();
        if assistant_count >= expected_assistant_messages {
            break;
        }

        if events.iter().any(is_error_event) {
            break;
        }

        tokio::time::sleep(Duration::from_millis(800)).await;
    }

    events
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_rpc_session_and_stream() {
    let Some(config) = pi_test_config() else {
        return;
    };
>>>>>>> Stashed changes

async fn create_pi_session_with_native(app: &Router, session_id: &str) -> String {
    let (status, payload) = send_json(
        app,
        Method::POST,
        &format!("/v1/sessions/{session_id}"),
        Some(json!({
            "agent": "pi",
            "permissionMode": test_permission_mode(AgentId::Pi),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create pi session");
    let native_session_id = payload
        .get("native_session_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    assert!(
        !native_session_id.is_empty(),
        "expected native_session_id for pi session"
    );
    native_session_id
}

fn assert_strictly_increasing_sequences(events: &[Value], label: &str) {
    let mut last_sequence = 0u64;
    for event in events {
        let sequence = event
            .get("sequence")
            .and_then(Value::as_u64)
            .expect("missing sequence");
        assert!(
            sequence > last_sequence,
            "{label}: sequence did not increase (prev {last_sequence}, next {sequence})"
        );
        last_sequence = sequence;
    }
}

<<<<<<< Updated upstream
fn assert_all_events_for_session(events: &[Value], session_id: &str) {
    for event in events {
        let event_session_id = event
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert_eq!(
            event_session_id, session_id,
            "cross-session event detected in {session_id}: {event}"
        );
    }
}

fn assert_item_started_ids_unique(events: &[Value], label: &str) {
    let mut ids = std::collections::HashSet::new();
    for event in events {
        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if event_type != "item.started" {
            continue;
        }
        let Some(item_id) = event
            .get("data")
            .and_then(|data| data.get("item"))
            .and_then(|item| item.get("item_id"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        assert!(
            ids.insert(item_id.to_string()),
            "{label}: duplicate item.started id {item_id}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_rpc_session_and_stream() {
=======
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_rpc_multi_session_create_per_session_mode() {
    let _mode_guard = set_env_var("SANDBOX_AGENT_PI_FORCE_RUNTIME_MODE", "per-session");
>>>>>>> Stashed changes
    let Some(config) = pi_test_config() else {
        return;
    };

    let app = TestApp::new();
    let _guard = apply_credentials(&config.credentials);
    install_agent(&app.app, config.agent).await;

<<<<<<< Updated upstream
    let session_id = "pi-rpc-session";
    let _native_session_id = create_pi_session_with_native(&app.app, session_id).await;

    let events = read_turn_stream_events(&app.app, session_id, Duration::from_secs(120)).await;
    assert!(!events.is_empty(), "no events from pi stream");
    assert!(
        !events.iter().any(is_unparsed_event),
        "agent.unparsed event encountered"
    );
    assert!(
        should_stop(&events),
        "turn stream did not reach a terminal event"
    );
    assert_strictly_increasing_sequences(&events, "pi_rpc_session_and_stream");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_parallel_sessions_turns() {
=======
    let first = create_pi_session_checked(&app.app, "pi-multi-a").await;
    let second = create_pi_session_checked(&app.app, "pi-multi-b").await;

    let first_native = first
        .get("native_session_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let second_native = second
        .get("native_session_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(!first_native.is_empty(), "first native session id missing");
    assert!(
        !second_native.is_empty(),
        "second native session id missing"
    );
    assert_ne!(
        first_native, second_native,
        "per-session mode should allocate independent native session ids"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_rpc_per_session_queue_and_termination_isolation() {
    let _mode_guard = set_env_var("SANDBOX_AGENT_PI_FORCE_RUNTIME_MODE", "per-session");
>>>>>>> Stashed changes
    let Some(config) = pi_test_config() else {
        return;
    };

    let app = TestApp::new();
    let _guard = apply_credentials(&config.credentials);
    install_agent(&app.app, config.agent).await;

<<<<<<< Updated upstream
    let session_a = "pi-parallel-a";
    let session_b = "pi-parallel-b";
    create_pi_session_with_native(&app.app, session_a).await;
    create_pi_session_with_native(&app.app, session_b).await;

    let app_a = app.app.clone();
    let app_b = app.app.clone();
    let send_a = send_message(&app_a, session_a);
    let send_b = send_message(&app_b, session_b);
    tokio::join!(send_a, send_b);

    let app_a = app.app.clone();
    let app_b = app.app.clone();
    let poll_a = poll_events_until(&app_a, session_a, Duration::from_secs(120));
    let poll_b = poll_events_until(&app_b, session_b, Duration::from_secs(120));
    let (events_a, events_b) = tokio::join!(poll_a, poll_b);

    assert!(!events_a.is_empty(), "no events for session A");
    assert!(!events_b.is_empty(), "no events for session B");
    assert!(
        should_stop(&events_a),
        "session A did not reach a terminal event"
    );
    assert!(
        should_stop(&events_b),
        "session B did not reach a terminal event"
    );
    assert!(
        !events_a.iter().any(is_unparsed_event),
        "session A encountered agent.unparsed"
    );
    assert!(
        !events_b.iter().any(is_unparsed_event),
        "session B encountered agent.unparsed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_event_isolation() {
    let Some(config) = pi_test_config() else {
        return;
    };

    let app = TestApp::new();
    let _guard = apply_credentials(&config.credentials);
    install_agent(&app.app, config.agent).await;

    let session_a = "pi-isolation-a";
    let session_b = "pi-isolation-b";
    create_pi_session_with_native(&app.app, session_a).await;
    create_pi_session_with_native(&app.app, session_b).await;

    let app_a = app.app.clone();
    let app_b = app.app.clone();
    let send_a = send_message(&app_a, session_a);
    let send_b = send_message(&app_b, session_b);
    tokio::join!(send_a, send_b);

    let app_a = app.app.clone();
    let app_b = app.app.clone();
    let poll_a = poll_events_until(&app_a, session_a, Duration::from_secs(120));
    let poll_b = poll_events_until(&app_b, session_b, Duration::from_secs(120));
    let (events_a, events_b) = tokio::join!(poll_a, poll_b);

    assert!(should_stop(&events_a), "session A did not complete");
    assert!(should_stop(&events_b), "session B did not complete");
    assert_all_events_for_session(&events_a, session_a);
    assert_all_events_for_session(&events_b, session_b);
    assert_strictly_increasing_sequences(&events_a, "session A");
    assert_strictly_increasing_sequences(&events_b, "session B");
    assert_item_started_ids_unique(&events_a, "session A");
    assert_item_started_ids_unique(&events_b, "session B");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_terminate_one_session_does_not_affect_other() {
    let Some(config) = pi_test_config() else {
        return;
    };

    let app = TestApp::new();
    let _guard = apply_credentials(&config.credentials);
    install_agent(&app.app, config.agent).await;

    let session_a = "pi-terminate-a";
    let session_b = "pi-terminate-b";
    create_pi_session_with_native(&app.app, session_a).await;
    create_pi_session_with_native(&app.app, session_b).await;

    let terminate_status = send_status(
        &app.app,
        Method::POST,
        &format!("/v1/sessions/{session_a}/terminate"),
        None,
    )
    .await;
    assert_eq!(
        terminate_status,
        StatusCode::NO_CONTENT,
        "terminate session A"
    );

    send_message(&app.app, session_b).await;
    let events_b = poll_events_until(&app.app, session_b, Duration::from_secs(120)).await;
    assert!(!events_b.is_empty(), "no events for session B");
    assert!(
        should_stop(&events_b),
        "session B did not complete after A terminated"
    );

    let events_a = poll_events_until(&app.app, session_a, Duration::from_secs(10)).await;
    assert!(
        events_a.iter().any(|event| {
            event
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|ty| ty == "session.ended")
        }),
        "session A missing session.ended after terminate"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_runtime_restart_scope() {
    let Some(config) = pi_test_config() else {
        return;
    };

    let app = TestApp::new();
    let _guard = apply_credentials(&config.credentials);
    install_agent(&app.app, config.agent).await;

    let session_a = "pi-restart-scope-a";
    let session_b = "pi-restart-scope-b";
    create_pi_session_with_native(&app.app, session_a).await;
    create_pi_session_with_native(&app.app, session_b).await;

    let terminate_status = send_status(
        &app.app,
        Method::POST,
        &format!("/v1/sessions/{session_a}/terminate"),
        None,
    )
    .await;
    assert_eq!(
        terminate_status,
        StatusCode::NO_CONTENT,
        "terminate session A to stop only its runtime"
    );

    send_message(&app.app, session_b).await;
    let events_b = poll_events_until(&app.app, session_b, Duration::from_secs(120)).await;
    assert!(
        should_stop(&events_b),
        "session B did not continue after A stopped"
    );
    assert_all_events_for_session(&events_b, session_b);
}
=======
    create_pi_session_checked(&app.app, "pi-queue-a").await;
    create_pi_session_checked(&app.app, "pi-queue-b").await;

    let status = send_status(
        &app.app,
        Method::POST,
        "/v1/sessions/pi-queue-a/messages",
        Some(json!({ "message": "Reply with exactly FIRST." })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "send first prompt");

    let status = send_status(
        &app.app,
        Method::POST,
        "/v1/sessions/pi-queue-a/messages",
        Some(json!({ "message": "Reply with exactly SECOND." })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "enqueue second prompt");

    let status = send_status(
        &app.app,
        Method::POST,
        "/v1/sessions/pi-queue-b/messages",
        Some(json!({ "message": PROMPT })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "send prompt to sibling session"
    );

    let events_a =
        poll_events_until_assistant_count(&app.app, "pi-queue-a", 2, Duration::from_secs(240))
            .await;
    let events_b =
        poll_events_until_assistant_count(&app.app, "pi-queue-b", 1, Duration::from_secs(180))
            .await;

    assert!(
        !events_a.iter().any(is_unparsed_event),
        "session a emitted agent.unparsed"
    );
    assert!(
        !events_b.iter().any(is_unparsed_event),
        "session b emitted agent.unparsed"
    );
    let assistant_count_a = events_a
        .iter()
        .filter(|event| is_assistant_message(event))
        .count();
    let assistant_count_b = events_b
        .iter()
        .filter(|event| is_assistant_message(event))
        .count();
    assert!(
        assistant_count_a >= 2,
        "expected at least two assistant completions for queued session, got {assistant_count_a}"
    );
    assert!(
        assistant_count_b >= 1,
        "expected assistant completion for sibling session, got {assistant_count_b}"
    );

    let status = send_status(
        &app.app,
        Method::POST,
        "/v1/sessions/pi-queue-a/terminate",
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "terminate first session");

    let status = send_status(
        &app.app,
        Method::POST,
        "/v1/sessions/pi-queue-b/messages",
        Some(json!({ "message": PROMPT })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "sibling session should continue after termination"
    );

    let events_b_after =
        poll_events_until_assistant_count(&app.app, "pi-queue-b", 2, Duration::from_secs(180))
            .await;
    let assistant_count_b_after = events_b_after
        .iter()
        .filter(|event| is_assistant_message(event))
        .count();
    assert!(
        assistant_count_b_after >= 2,
        "expected additional assistant completion for sibling session after termination"
    );
}
>>>>>>> Stashed changes
