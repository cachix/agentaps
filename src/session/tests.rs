use super::*;
#[cfg(unix)]
use crate::acp::Event;
#[cfg(unix)]
use std::sync::mpsc;

#[test]
fn old_sessions_seed_prompt_history_from_messages_and_queue() {
    let mut config = agent(ProtocolVersion::V2).snapshot();
    config.prompt_history.clear();
    config.messages.push(ChatEntry {
        role: Role::User,
        key: None,
        text: "sent".into(),
    });
    config.pending_prompts.push("queued".into());
    let restored = SessionController::new(config);
    assert_eq!(restored.config.prompt_history, ["sent", "queued"]);
}

#[test]
fn interrupted_turn_waits_for_recovery_before_queued_prompts() {
    let mut config = agent(ProtocolVersion::V1).snapshot();
    config.was_working = true;
    config.prompt_history = vec!["Earlier".into(), "Active request".into(), "Queued".into()];
    config.pending_prompts.push("Queued".into());
    let mut restored = SessionController::new(config);
    restored.status = Status::Idle;
    restored.session_id = Some("session-1".into());

    assert!(restored.snapshot().was_working);
    assert!(restored.continuation_prompt().contains("Active request"));
    assert!(!restored.continuation_prompt().contains("Queued"));
    assert!(!restored.start_next_queued_prompt());

    restored.config.prompt_history.push("Continuation".into());
    let saved = serde_json::to_vec(&restored.snapshot()).unwrap();
    let reloaded = SessionController::new(serde_json::from_slice(&saved).unwrap());
    assert_eq!(reloaded.messages.len(), restored.messages.len());
    assert!(reloaded.continuation_prompt().contains("Active request"));
    assert!(!reloaded.continuation_prompt().contains("Queued"));
    restored.config.was_working = false;
    assert!(!restored.snapshot().was_working);
}

#[cfg(unix)]
#[test]
fn restored_turn_continues_automatically_before_queued_work() {
    let mut config = agent(ProtocolVersion::V1).snapshot();
    config.was_working = true;
    config.active_prompt = Some("Finish the work".into());
    config.pending_prompts.push("Next task".into());
    let mut restored = SessionController::new(config);
    restored.protocol = Some(ProtocolVersion::V1);
    restored.session_id = Some("session-1".into());
    restored.status = Status::Idle;
    let command = vec![
        "/bin/sh".into(),
        "-c".into(),
        "IFS= read -r request; printf '%s\\n' \"$request\"".into(),
    ];
    let (tx, rx) = mpsc::channel();
    restored.connection = Some(Connection::spawn(1, &command, Path::new("/"), None, tx).unwrap());
    restored.recovery_due = Some(Instant::now() - Duration::from_secs(1));

    assert!(restored.auto_continue_interrupted_turn());
    assert!(restored.active_work);
    assert!(!restored.config.was_working);
    assert_eq!(
        restored.config.active_prompt.as_deref(),
        Some("Finish the work")
    );
    assert_eq!(restored.config.pending_prompts, ["Next task"]);
    assert!(!restored.auto_continue_interrupted_turn());

    let Event::Message { value, .. } = rx.recv_timeout(Duration::from_secs(2)).unwrap() else {
        panic!("Agent did not receive a continuation prompt");
    };
    assert_eq!(value["method"], "session/prompt");
    assert!(
        value["params"]["prompt"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Finish the work")
    );
}

#[test]
fn live_v2_turn_after_restore_does_not_start_a_second_prompt() {
    let mut config = agent(ProtocolVersion::V2).snapshot();
    config.was_working = true;
    config.active_prompt = Some("Finish the work".into());
    let mut restored = SessionController::new(config);
    restored.protocol = Some(ProtocolVersion::V2);
    restored.session_id = Some("session-1".into());
    restored.status = Status::Idle;
    restored.recovery_due = Some(Instant::now() - Duration::from_secs(1));

    SessionController::handle_update(
        &mut restored,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"state_update","state":"running"
        }}}),
    );

    assert!(restored.active_work);
    assert!(!restored.config.was_working);
    assert!(restored.recovery_due.is_none());
    assert!(!restored.auto_continue_interrupted_turn());
}

pub(crate) fn agent(protocol: ProtocolVersion) -> SessionController {
    let mut agent = SessionController::new(AgentConfig {
        id: 1,
        command: vec!["fixture".into()],
        archived: false,
        display_name: None,
        title: None,
        session_id: None,
        model: None,
        context: None,
        messages: Vec::new(),
        available_commands: Vec::new(),
        pending_prompts: Vec::new(),
        active_prompt: None,
        prompt_history: Vec::new(),
        was_working: false,
        session_has_activity: false,
        fork_pending: false,
    });
    agent.protocol = Some(protocol);
    agent.session_id = Some("session-1".into());
    agent.status = Status::Working;
    agent.active_work = true;
    agent
}

#[test]
fn agent_session_titles_update_and_survive_restarts() {
    for protocol in [ProtocolVersion::V1, ProtocolVersion::V2] {
        let mut agent = agent(protocol);
        SessionController::handle_update(
            &mut agent,
            &json!({"params":{"sessionId":"session-1","update":{
                "sessionUpdate":"session_info_update","title":"  Fix   login flow  "
            }}}),
        );
        assert_eq!(agent.config.title.as_deref(), Some("Fix login flow"));

        SessionController::handle_update(
            &mut agent,
            &json!({"params":{"sessionId":"other-session","update":{
                "sessionUpdate":"session_info_update","title":"Unrelated work"
            }}}),
        );
        assert_eq!(agent.config.title.as_deref(), Some("Fix login flow"));

        let restored = SessionController::new(agent.snapshot());
        assert_eq!(restored.config.title.as_deref(), Some("Fix login flow"));

        SessionController::handle_update(
            &mut agent,
            &json!({"params":{"sessionId":"session-1","update":{
                "sessionUpdate":"session_info_update","updatedAt":"2026-09-28T00:00:00Z"
            }}}),
        );
        assert_eq!(agent.config.title.as_deref(), Some("Fix login flow"));

        SessionController::handle_update(
            &mut agent,
            &json!({"params":{"sessionId":"session-1","update":{
                "sessionUpdate":"session_info_update","title":null
            }}}),
        );
        assert!(agent.config.title.is_none());
    }
}

#[test]
fn v2_prompt_acknowledgement_is_not_completion() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.awaiting_response = true;
    agent.handle_prompt_response(&json!({"messageId":"user-1"}));
    assert_eq!(agent.status, Status::Working);
    assert!(agent.active_work);
    assert!(agent.awaiting_response);

    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"agent_message","messageId":"agent-1","content":[{"type":"text","text":"Hello"}]
        }}}),
    );
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"agent_message_chunk","messageId":"agent-1","content":{"type":"text","text":" world"}
        }}}),
    );
    assert_eq!(agent.messages.len(), 1);
    assert_eq!(agent.messages[0].text, "Hello world");
    assert!(!agent.awaiting_response);

    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"state_update","state":"idle","stopReason":"end_turn"
        }}}),
    );
    assert_eq!(agent.status, Status::Done);
    assert!(!agent.active_work);
    agent.mark_viewed();
    assert_eq!(agent.status, Status::Idle);
}

#[test]
fn v1_prompt_response_still_completes_turn() {
    let mut agent = agent(ProtocolVersion::V1);
    agent.handle_prompt_response(&json!({"stopReason":"end_turn"}));
    assert_eq!(agent.status, Status::Done);
    assert!(!agent.active_work);
}

#[test]
fn resetting_context_reuses_agent_settings_without_reusing_session_state() {
    let mut previous = agent(ProtocolVersion::V2);
    previous.config.display_name = Some("Example agent".into());
    previous.config.title = Some("Earlier task".into());
    previous.config.session_id = Some("session-1".into());
    previous.config.pending_prompts.push("Queued work".into());
    previous.model = Some("Example model".into());
    previous.context = Some((42, 100));
    previous.log(Role::User, "Earlier request");
    previous.upsert_message(
        Role::Agent,
        "reply-1",
        Some(&json!([{"type":"text","text":"Earlier reply"}])),
        false,
    );

    let mut history = previous.messages.clone();
    history.push(ChatEntry {
        role: Role::ContextReset,
        key: None,
        text: "Context reset".into(),
    });
    let config = previous.reset_config(2, history);
    assert_eq!(config.id, 2);
    assert_eq!(config.command, previous.config.command);
    assert_eq!(config.display_name, previous.config.display_name);
    assert!(config.title.is_none());
    assert_eq!(config.session_id, None);
    assert_eq!(config.model, None);
    assert_eq!(config.context, None);
    assert!(config.pending_prompts.is_empty());
    assert!(!config.session_has_activity);

    let saved = serde_json::to_vec(&config).unwrap();
    let mut reset = SessionController::new(serde_json::from_slice(&saved).unwrap());
    assert_eq!(reset.status, Status::Connecting);
    assert_eq!(reset.session_id, None);
    assert_eq!(reset.messages.len(), 3);
    assert_eq!(reset.messages[2].role, Role::ContextReset);
    assert!(reset.messages.iter().all(|entry| entry.key.is_none()));
    reset.upsert_message(
        Role::Agent,
        "reply-1",
        Some(&json!([{"type":"text","text":"New reply"}])),
        false,
    );
    assert_eq!(reset.messages.len(), 4);
    assert_eq!(reset.messages[1].text, "Earlier reply");
    assert_eq!(reset.messages[3].text, "New reply");
    assert_eq!(previous.messages.len(), 2);
}

#[test]
fn fork_copies_history_through_selected_response_without_source_session_state() {
    let mut source = agent(ProtocolVersion::V2);
    source.config.title = Some("Original task".into());
    source.config.pending_prompts.push("Queued work".into());
    source.log(Role::User, "First question");
    source.upsert_message(
        Role::Agent,
        "first-reply",
        Some(&json!([{"type":"text","text":"First answer"}])),
        false,
    );
    source.log(Role::User, "Later question");
    source.log(Role::Agent, "Later answer");

    let config = source.fork_config(2, 1).unwrap();
    assert_eq!(config.id, 2);
    assert_eq!(config.session_id, None);
    assert!(config.title.is_none());
    assert_eq!(config.messages.len(), 2);
    assert_eq!(config.messages[1].text, "First answer");
    assert!(config.messages.iter().all(|entry| entry.key.is_none()));
    assert_eq!(config.prompt_history, ["First question"]);
    assert!(config.pending_prompts.is_empty());
    assert!(config.fork_pending);
    assert!(source.fork_config(3, 0).is_none());
    assert_eq!(source.messages.len(), 4);

    let restored: AgentConfig =
        serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
    assert!(restored.fork_pending);
}

#[test]
#[cfg(unix)]
fn fork_supplies_only_active_conversation_to_first_prompt() {
    let mut source = agent(ProtocolVersion::V1);
    source.log(Role::User, "Old question");
    source.log(Role::ContextReset, "Context reset");
    source.log(Role::User, "Current question");
    source.log(Role::Tool, "Reading file");
    source.log(Role::Agent, "Current answer");
    let config = source.fork_config(2, 4).unwrap();
    let mut fork = SessionController::new(config);
    fork.protocol = Some(ProtocolVersion::V1);
    fork.session_id = Some("fork-session".into());
    let command = vec![
        "/bin/sh".into(),
        "-c".into(),
        "while IFS= read -r line; do printf '%s\\n' \"$line\"; done".into(),
    ];
    let (events_tx, events_rx) = mpsc::channel();
    fork.connection =
        Some(Connection::spawn(2, &command, Path::new("/"), None, events_tx).unwrap());

    fork.start_prompt("New direction".into()).unwrap();
    let Event::Message { value, .. } = events_rx.recv_timeout(Duration::from_secs(2)).unwrap()
    else {
        panic!("agent disconnected before receiving fork prompt");
    };
    let sent = value["params"]["prompt"][0]["text"].as_str().unwrap();
    assert!(sent.contains("Current question"));
    assert!(sent.contains("Current answer"));
    assert!(sent.contains("New direction"));
    assert!(!sent.contains("Old question"));
    assert!(!sent.contains("Reading file"));
    assert!(!fork.config.fork_pending);

    fork.handle_prompt_response(&json!({"stopReason":"end_turn"}));
    fork.start_prompt("Follow-up".into()).unwrap();
    let Event::Message { value, .. } = events_rx.recv_timeout(Duration::from_secs(2)).unwrap()
    else {
        panic!("agent disconnected before receiving follow-up");
    };
    assert_eq!(value["params"]["prompt"][0]["text"], "Follow-up");
}

#[test]
fn model_and_context_follow_acp_session_updates() {
    let mut agent = agent(ProtocolVersion::V1);
    let options = json!([
        {"id":"model","category":"model","currentValue":"default",
            "options":[{"value":"default","name":"Default","description":"Opus (1M context)"}]},
        {"configId":"thought_level","category":"thought_level","type":"select",
            "currentValue":"high","options":[{"value":"high","name":"High"}]}
    ]);
    assert_eq!(model_option(&options).unwrap().label(), "Opus (1M context)");
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"config_option_update","configOptions":options
        }}}),
    );
    assert_eq!(agent.model.as_deref(), Some("Opus (1M context)"));
    assert_eq!(agent.effort_option.as_ref().unwrap().label(), "High");
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"usage_update","used":42_000,"size":200_000
        }}}),
    );
    assert_eq!(agent.context, Some((42_000, 200_000)));
}

#[test]
fn model_options_support_grouped_choices_and_acp_selection() {
    let options = json!([
        {"configId":"thought_level","category":"thought_level","type":"select",
            "currentValue":"low","options":[{"value":"low","name":"Low"}]},
        {"configId":"provider_model","category":"model","type":"select",
            "currentValue":"model-b","options":[
                {"groupId":"fast","name":"Fast","options":[{"value":"model-a","name":"Model A"}]},
                {"groupId":"strong","name":"Strong","options":[{"value":"model-b","name":"Model B"}]}
            ]}
    ]);
    let option = model_option(&options).unwrap();
    assert_eq!(option.id, "provider_model");
    assert_eq!(option.label(), "Model B");
    assert_eq!(option.choices.len(), 2);
    assert_eq!(
        set_config_option_request(7, "session-1", &option, "model-a"),
        json!({"jsonrpc":"2.0","id":7,"method":"session/set_config_option","params":{
            "sessionId":"session-1","configId":"provider_model","type":"id","value":"model-a"
        }})
    );
    let effort = effort_option(&options).unwrap();
    assert_eq!(effort.id, "thought_level");
    assert_eq!(effort.label(), "Low");
    assert_eq!(
        set_config_option_request(8, "session-1", &effort, "low")["params"]["configId"],
        "thought_level"
    );
}

#[test]
fn mode_follows_agent_config_option_and_mode_updates() {
    let mut agent = agent(ProtocolVersion::V2);
    let options = json!([
        {"id":"mode","name":"Mode","category":"mode","type":"select","currentValue":"default",
            "options":[{"value":"default","name":"Manual"},{"value":"acceptEdits","name":"Accept edits"},
                {"value":"auto","name":"Auto"}]}
    ]);
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"config_option_update","configOptions":options
        }}}),
    );
    let mode = agent.mode_option.as_ref().unwrap();
    assert_eq!(mode.label(), "Manual");
    assert_eq!(mode.choices.len(), 3);
    assert_eq!(
        mode_request(7, "session-1", mode, false, "auto"),
        json!({"jsonrpc":"2.0","id":7,"method":"session/set_config_option","params":{
            "sessionId":"session-1","configId":"mode","type":"id","value":"auto"
        }})
    );
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"current_mode_update","currentModeId":"acceptEdits"
        }}}),
    );
    assert_eq!(agent.mode_option.as_ref().unwrap().label(), "Accept edits");
}

#[test]
fn plan_toggle_follows_collaboration_mode_option() {
    let mut agent = agent(ProtocolVersion::V2);
    let options = json!([
        {"id":"mode","category":"mode","type":"select","currentValue":"agent",
            "options":[{"value":"read-only","name":"Read-only"},{"value":"agent","name":"Auto review"}]},
        {"id":"collaboration_mode","name":"Collaboration mode","category":"collaboration_mode",
            "type":"select","currentValue":"default",
            "options":[{"value":"default","name":"Default"},{"value":"plan","name":"Plan"}]}
    ]);
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"config_option_update","configOptions":options
        }}}),
    );
    assert_eq!(agent.mode_option.as_ref().unwrap().label(), "Auto review");
    let plan = agent.plan_toggle().unwrap();
    assert!(!plan.active);
    assert_eq!(plan.next, "plan");
    agent.collaboration_option.as_mut().unwrap().current = "plan".into();
    let plan = agent.plan_toggle().unwrap();
    assert!(plan.active);
    assert_eq!(plan.next, "default");
    // Agents without a plan choice get no toggle.
    agent.collaboration_option = collaboration_option(&json!([
        {"id":"collaboration_mode","category":"collaboration_mode","currentValue":"pair",
            "options":[{"value":"pair","name":"Pair"}]}
    ]));
    assert!(agent.plan_toggle().is_none());
}

#[test]
fn session_modes_are_used_when_agent_has_no_mode_config_option() {
    let mut agent = agent(ProtocolVersion::V1);
    update_session_modes(
        &mut agent,
        &json!({"currentModeId":"default","availableModes":[
            {"id":"default","name":"Default"},{"id":"yolo","name":"YOLO"}
        ]}),
    );
    assert!(agent.legacy_modes);
    let mode = agent.mode_option.as_ref().unwrap();
    assert_eq!(mode.label(), "Default");
    assert_eq!(
        mode_request(3, "session-1", mode, true, "yolo"),
        json!({"jsonrpc":"2.0","id":3,"method":"session/set_mode","params":{
            "sessionId":"session-1","modeId":"yolo"
        }})
    );
    // Config options without a mode keep the session's mode list.
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"config_option_update","configOptions":[]
        }}}),
    );
    assert_eq!(agent.mode_option.as_ref().unwrap().label(), "Default");
}

#[test]
fn reasoning_effort_is_detected_in_model_config_options() {
    let options = json!([
        {"configId":"temperature","category":"model_config","name":"Temperature",
            "type":"select","currentValue":"balanced","options":[{"value":"balanced","name":"Balanced"}]},
        {"configId":"reasoning_effort","category":"model_config","name":"Reasoning effort",
            "type":"select","currentValue":"high","options":[{"value":"low","name":"Low"},
                {"value":"high","name":"High"}]}
    ]);
    let effort = effort_option(&options).unwrap();
    assert_eq!(effort.id, "reasoning_effort");
    assert_eq!(effort.label(), "High");
}

#[test]
fn session_history_survives_config_round_trip() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.model = Some("Example Model".into());
    agent.context = Some((3_000, 64_000));
    agent.log(Role::User, "Please check the build");
    agent.log(Role::Tool, "cargo test · completed");
    agent.config.pending_prompts.push("Follow up next".into());
    let saved = agent.snapshot();
    let encoded = serde_json::to_vec(&saved).unwrap();
    let restored = SessionController::new(serde_json::from_slice(&encoded).unwrap());
    assert_eq!(restored.config.session_id.as_deref(), Some("session-1"));
    assert_eq!(restored.model.as_deref(), Some("Example Model"));
    assert_eq!(restored.context, Some((3_000, 64_000)));
    assert_eq!(restored.messages[0].text, "Please check the build");
    assert_eq!(restored.messages[1].text, "cargo test · completed");
    assert!(restored.messages[2].text.contains("active"));
    assert_eq!(restored.config.pending_prompts, vec!["Follow up next"]);
}

#[test]
fn queued_prompt_stays_saved_when_agent_cannot_send_it() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.config.pending_prompts.push("Next request".into());
    assert!(!agent.start_next_queued_prompt());
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"state_update","state":"idle","stopReason":"end_turn"
        }}}),
    );
    assert!(agent.start_next_queued_prompt());
    assert_eq!(agent.status, Status::Error);
    assert_eq!(agent.config.pending_prompts, vec!["Next request"]);
}

#[cfg(unix)]
#[test]
fn claude_refresh_lock_retries_prompt_once_without_duplicating_user_message() {
    let mut agent = agent(ProtocolVersion::V1);
    let command = vec![
        "/bin/sh".into(),
        "-c".into(),
        "while IFS= read -r line; do printf '%s\\n' \"$line\"; done".into(),
    ];
    let (events_tx, events_rx) = mpsc::channel();
    agent.connection =
        Some(Connection::spawn(1, &command, Path::new("/"), None, events_tx).unwrap());
    agent.start_prompt("Please help".into()).unwrap();
    let Event::Message { value: first, .. } =
        events_rx.recv_timeout(Duration::from_secs(2)).unwrap()
    else {
        panic!("agent did not receive first prompt");
    };
    let lock_error = "Failed to refresh OAuth token: another Claude Code process is refreshing it or exited mid-refresh";
    assert!(agent.defer_oauth_retry(3, lock_error));
    assert!(!agent.retry_oauth_request());
    agent.oauth_retry.as_mut().unwrap().due = Some(Instant::now() - Duration::from_secs(1));
    assert!(agent.retry_oauth_request());
    let Event::Message { value: second, .. } =
        events_rx.recv_timeout(Duration::from_secs(2)).unwrap()
    else {
        panic!("agent did not receive retried prompt");
    };
    assert_eq!(second["params"], first["params"]);
    assert_ne!(second["id"], first["id"]);
    assert_eq!(agent.config.active_prompt.as_deref(), Some("Please help"));
    assert_eq!(
        agent
            .messages
            .iter()
            .filter(|entry| entry.role == Role::User)
            .count(),
        1
    );
    assert!(!agent.defer_oauth_retry(4, lock_error));
}

#[test]
fn claude_refresh_lock_does_not_retry_after_agent_output() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.remember_auth_request(json!({"jsonrpc":"2.0","id":3,"method":"session/prompt"}));
    agent.awaiting_response = false;
    assert!(!agent.defer_oauth_retry(
        3,
        "Failed to refresh OAuth token: another Claude Code process is refreshing it or exited mid-refresh"
    ));
    assert!(agent.oauth_retry.as_ref().unwrap().due.is_none());
}

#[test]
fn claude_refresh_lock_does_not_replay_accepted_v2_message() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.awaiting_response = true;
    agent.remember_auth_request(json!({"jsonrpc":"2.0","id":3,"method":"session/prompt"}));
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"user_message","messageId":"user-1","content":[{"type":"text","text":"Please help"}]
        }}}),
    );
    assert!(!agent.defer_oauth_retry(
        3,
        "Failed to refresh OAuth token: another Claude Code process is refreshing it or exited mid-refresh"
    ));
}

#[cfg(unix)]
#[test]
fn claude_refresh_lock_retries_session_setup() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.status = Status::Connecting;
    let command = vec![
        "/bin/sh".into(),
        "-c".into(),
        "while IFS= read -r line; do printf '%s\\n' \"$line\"; done".into(),
    ];
    let (events_tx, events_rx) = mpsc::channel();
    agent.connection =
        Some(Connection::spawn(1, &command, Path::new("/"), None, events_tx).unwrap());
    SessionController::start_new_session(&mut agent, Path::new("/"));
    let Event::Message { value: first, .. } =
        events_rx.recv_timeout(Duration::from_secs(2)).unwrap()
    else {
        panic!("agent did not receive session setup");
    };
    assert!(agent.defer_oauth_retry(
        2,
        "Failed to refresh OAuth token: another Claude Code process is refreshing it or exited mid-refresh"
    ));
    agent.oauth_retry.as_mut().unwrap().due = Some(Instant::now() - Duration::from_secs(1));
    assert!(agent.retry_oauth_request());
    let Event::Message { value: second, .. } =
        events_rx.recv_timeout(Duration::from_secs(2)).unwrap()
    else {
        panic!("agent did not receive retried session setup");
    };
    assert_eq!(second, first);
    assert_eq!(agent.status, Status::Connecting);
}

#[test]
fn claude_refresh_lock_wait_ignores_failed_turn_idle_update() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.awaiting_response = true;
    agent.remember_auth_request(json!({"jsonrpc":"2.0","id":3,"method":"session/prompt"}));
    assert!(agent.defer_oauth_retry(
        3,
        "Failed to refresh OAuth token: another Claude Code process is refreshing it or exited mid-refresh"
    ));
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"state_update","state":"idle","stopReason":"end_turn"
        }}}),
    );
    assert!(agent.active_work);
    assert!(agent.awaiting_response);
}

#[test]
#[cfg(unix)]
fn queued_prompts_reach_the_agent_in_order_after_each_turn() {
    let mut agent = agent(ProtocolVersion::V1);
    let command = vec![
        "/bin/sh".into(),
        "-c".into(),
        "while IFS= read -r line; do printf '%s\\n' \"$line\"; done".into(),
    ];
    let (events_tx, events_rx) = mpsc::channel();
    agent.connection =
        Some(Connection::spawn(1, &command, Path::new("/"), None, events_tx).unwrap());
    agent.config.pending_prompts = vec!["First".into(), "!printf '%s' hi".into()];

    assert!(!agent.start_next_queued_prompt());
    agent.handle_prompt_response(&json!({"stopReason":"end_turn"}));
    for (original, expected) in [
        ("First", "First"),
        (
            "!printf '%s' hi",
            "Run this shell command exactly as written, then report its output:\n\nprintf '%s' hi",
        ),
    ] {
        assert!(agent.start_next_queued_prompt());
        let Event::Message { value, .. } = events_rx.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("agent disconnected before receiving queued prompt");
        };
        assert_eq!(value["params"]["prompt"][0]["text"], expected);
        assert_eq!(agent.messages.last().unwrap().text, original);
        agent.handle_prompt_response(&json!({"stopReason":"end_turn"}));
    }
    assert!(agent.config.pending_prompts.is_empty());
}

#[test]
fn restoration_respects_v1_capabilities() {
    assert_eq!(
        restore_mode(ProtocolVersion::V2, &json!({})),
        Some(RestoreMode::Resume)
    );
    assert_eq!(
        restore_mode(
            ProtocolVersion::V1,
            &json!({"agentCapabilities":{"sessionCapabilities":{"resume":{}},"loadSession":true}})
        ),
        Some(RestoreMode::Resume)
    );
    assert_eq!(
        restore_mode(
            ProtocolVersion::V1,
            &json!({"agentCapabilities":{"loadSession":true}})
        ),
        Some(RestoreMode::Load)
    );
    assert_eq!(restore_mode(ProtocolVersion::V1, &json!({})), None);
}

#[test]
fn v1_load_replay_does_not_duplicate_saved_history() {
    let mut agent = agent(ProtocolVersion::V1);
    agent.restoring = Some(RestoreMode::Load);
    agent.log(Role::Agent, "Saved reply");
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Saved reply"}
        }}}),
    );
    assert_eq!(agent.messages.len(), 1);
    assert_eq!(agent.messages[0].text, "Saved reply");
}

#[test]
fn empty_sessions_are_not_restored() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.active_work = false;
    agent.log(Role::System, "A prior restore attempt failed");
    assert!(!agent.has_restorable_activity());
    agent.log(Role::User, "Continue this conversation");
    assert!(agent.has_restorable_activity());
}
