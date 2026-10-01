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
        images: Vec::new(),
    });
    config.pending_prompts.push("queued".into());
    let restored = SessionController::new(config, ImageStore::for_tests());
    assert_eq!(restored.config.prompt_history, ["sent", "queued"]);
}

#[test]
fn interrupted_turn_waits_for_recovery_before_queued_prompts() {
    let mut config = agent(ProtocolVersion::V1).snapshot();
    config.was_working = true;
    config.prompt_history = vec!["Earlier".into(), "Active request".into(), "Queued".into()];
    config.pending_prompts.push("Queued".into());
    let mut restored = SessionController::new(config, ImageStore::for_tests());
    restored.status = Status::Idle;
    restored.session_id = Some("session-1".into());

    assert!(restored.snapshot().was_working);
    assert!(restored.continuation_prompt().contains("Active request"));
    assert!(!restored.continuation_prompt().contains("Queued"));
    assert!(!restored.start_next_queued_prompt());

    restored.config.prompt_history.push("Continuation".into());
    let saved = serde_json::to_vec(&restored.snapshot()).unwrap();
    let reloaded = SessionController::new(
        serde_json::from_slice(&saved).unwrap(),
        ImageStore::for_tests(),
    );
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
    let mut restored = SessionController::new(config, ImageStore::for_tests());
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
    assert_eq!(restored.config.pending_prompts, [Prompt::from("Next task")]);
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
    let mut restored = SessionController::new(config, ImageStore::for_tests());
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
    let mut agent = SessionController::new(
        AgentConfig {
            id: 1,
            command: vec!["fixture".into()],
            archived: false,
            display_name: None,
            title: None,
            custom_title: None,
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
            fork_source: None,
        },
        ImageStore::for_tests(),
    );
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

        let restored = SessionController::new(agent.snapshot(), ImageStore::for_tests());
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
fn custom_session_names_override_agent_updates_and_survive_saved_config() {
    for protocol in [ProtocolVersion::V1, ProtocolVersion::V2] {
        let mut agent = agent(protocol);
        agent.config.rename_session("  My   task\nname  ");
        assert_eq!(agent.config.session_title(), Some("My task name"));
        SessionController::handle_update(
            &mut agent,
            &json!({"params":{"sessionId":"session-1","update":{
                "sessionUpdate":"session_info_update","title":"Agent's latest title"
            }}}),
        );
        assert_eq!(agent.config.session_title(), Some("My task name"));

        let saved = serde_json::to_vec(&agent.snapshot()).unwrap();
        let config: AgentConfig = serde_json::from_slice(&saved).unwrap();
        let mut restored = SessionController::new(config, ImageStore::for_tests());
        assert_eq!(restored.config.session_title(), Some("My task name"));
        assert_eq!(
            restored.reset_config(2, Vec::new()).session_title(),
            Some("My task name")
        );
        restored.config.rename_session(" \n\t ");
        assert_eq!(
            restored.config.session_title(),
            Some("Agent's latest title")
        );
        let saved = serde_json::to_value(restored.snapshot()).unwrap();
        assert!(saved.get("custom_title").is_none());
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
        images: Vec::new(),
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
    let restored: AgentConfig = serde_json::from_slice(&saved).unwrap();
    assert!(restored.messages.is_empty());
    let mut reset = SessionController::new(config, ImageStore::for_tests());
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
    let mut fork = SessionController::new(config, ImageStore::for_tests());
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
fn session_references_and_pending_work_survive_without_local_history() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.model = Some("Example Model".into());
    agent.context = Some((3_000, 64_000));
    agent.log(Role::User, "Please check the build");
    agent.log(Role::Tool, "cargo test · completed");
    agent.config.pending_prompts.push("Follow up next".into());
    let saved = agent.snapshot();
    let encoded = serde_json::to_vec(&saved).unwrap();
    let restored = SessionController::new(
        serde_json::from_slice(&encoded).unwrap(),
        ImageStore::for_tests(),
    );
    assert_eq!(restored.config.session_id.as_deref(), Some("session-1"));
    assert_eq!(restored.model.as_deref(), Some("Example Model"));
    assert_eq!(restored.context, Some((3_000, 64_000)));
    let value: Value = serde_json::from_slice(&encoded).unwrap();
    assert!(value.get("messages").is_none());
    assert!(value.get("prompt_history").is_none());
    assert!(
        restored
            .messages
            .iter()
            .all(|entry| entry.role == Role::System)
    );
    assert_eq!(restored.config.prompt_history, ["Follow up next"]);
    assert_eq!(
        restored.config.pending_prompts,
        vec![Prompt::from("Follow up next")]
    );
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
    assert_eq!(
        agent.config.pending_prompts,
        vec![Prompt::from("Next request")]
    );
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
        Some(RestoreMode::Load)
    );
    assert_eq!(
        restore_mode(
            ProtocolVersion::V1,
            &json!({"agentCapabilities":{"loadSession":true}})
        ),
        Some(RestoreMode::Load)
    );
    assert_eq!(
        restore_mode(
            ProtocolVersion::V1,
            &json!({"agentCapabilities":{"sessionCapabilities":{"resume":{}}}})
        ),
        Some(RestoreMode::Resume)
    );
    assert_eq!(restore_mode(ProtocolVersion::V1, &json!({})), None);
}

#[test]
fn v1_load_replays_user_agent_and_tool_history() {
    let mut agent = agent(ProtocolVersion::V1);
    agent.restoring = Some(RestoreMode::Load);
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"user_message_chunk","content":{"type":"text","text":"Saved request"}
        }}}),
    );
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Saved reply"}
        }}}),
    );
    SessionController::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"tool_call","toolCallId":"tool-1","title":"Read file","status":"completed"
        }}}),
    );
    agent.handle_message(Path::new("/"), &json!({"id":2,"result":{}}));
    assert_eq!(agent.messages.len(), 3);
    assert_eq!(agent.messages[0].text, "Saved request");
    assert_eq!(agent.messages[1].text, "Saved reply");
    assert_eq!(agent.messages[2].role, Role::Tool);
    assert_eq!(agent.config.prompt_history, ["Saved request"]);
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

#[cfg(unix)]
fn echo_requests(agent: &mut SessionController) -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    agent.connection = Some(
        Connection::spawn(
            agent.config.id,
            &[
                "/bin/sh".into(),
                "-c".into(),
                "while IFS= read -r request; do printf '%s\\n' \"$request\"; done".into(),
            ],
            Path::new("/"),
            None,
            tx,
        )
        .unwrap(),
    );
    rx
}

#[cfg(unix)]
fn next_request(rx: &mpsc::Receiver<Event>) -> Value {
    match rx.recv_timeout(Duration::from_secs(2)).unwrap() {
        Event::Message { value, .. } => value,
        other => panic!("Unexpected agent event: {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn v2_resume_requests_history_and_rebuilds_the_view_from_harness_replay() {
    let mut session = agent(ProtocolVersion::V2);
    session.active_work = false;
    session.status = Status::Connecting;
    session.log(Role::Agent, "Stale view");
    session.config.prompt_history.push("Stale request".into());
    let rx = echo_requests(&mut session);
    SessionController::resume_session(
        &mut session,
        Path::new("/"),
        RestoreMode::Resume,
        "session-1".into(),
    );
    let request = next_request(&rx);
    let typed: v2::ResumeSessionRequest =
        serde_json::from_value(request["params"].clone()).unwrap();
    assert!(matches!(typed.replay_from, Some(v2::ReplayFrom::Start(_))));
    assert_eq!(request["method"], "session/resume");
    assert!(session.messages.is_empty());
    for update in [
        json!({"sessionUpdate":"user_message","messageId":"user-1","content":[{"type":"text","text":"Earlier request"}]}),
        json!({"sessionUpdate":"agent_message","messageId":"agent-1","content":[{"type":"text","text":"Earlier reply"}]}),
        json!({"sessionUpdate":"agent_message_chunk","messageId":"agent-1","content":{"type":"text","text":" continued"}}),
        json!({"sessionUpdate":"tool_call_update","toolCallId":"tool-1","title":"Read file","status":"completed"}),
    ] {
        session.handle_update(&json!({"params":{"sessionId":"session-1","update":update}}));
    }
    session.handle_message(Path::new("/"), &json!({"id":2,"result":{}}));
    assert_eq!(session.messages.len(), 3);
    assert_eq!(session.messages[1].text, "Earlier reply continued");
    assert_eq!(session.messages[2].role, Role::Tool);
    assert_eq!(session.config.prompt_history, ["Earlier request"]);
    assert!(session.snapshot().messages.is_empty());
    assert!(session.snapshot().prompt_history.is_empty());
}

#[cfg(unix)]
#[test]
fn unsubmitted_fork_reloads_its_source_after_restart_without_saving_a_transcript() {
    let mut source = agent(ProtocolVersion::V2);
    source.active_work = false;
    source.log(Role::User, "First question");
    source.upsert_message_text(
        Role::Agent,
        "agent-1",
        "First reply".into(),
        Vec::new(),
        false,
    );
    source.log(Role::User, "Later question");
    source.log(Role::Agent, "Later reply");
    let fork = source.fork_config(2, 1).unwrap();
    let bytes = serde_json::to_vec(&fork).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("First reply"));
    let mut fork = SessionController::new(
        serde_json::from_slice(&bytes).unwrap(),
        ImageStore::for_tests(),
    );
    let rx = echo_requests(&mut fork);
    fork.handle_message(
        Path::new("/"),
        &json!({"id":1,"result":{"protocolVersion":2,"info":{"name":"fixture","version":"1"},"capabilities":{"session":{}}}}),
    );
    let request = next_request(&rx);
    assert_eq!(request["method"], "session/resume");
    assert_eq!(request["params"]["sessionId"], "session-1");
    assert_eq!(request["params"]["replayFrom"]["type"], "start");
    for (id, role, text) in [
        ("user-1", "user_message", "First question"),
        ("agent-1", "agent_message", "First reply"),
        ("user-2", "user_message", "Later question"),
        ("agent-2", "agent_message", "Later reply"),
    ] {
        fork.handle_update(&json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":role,"messageId":id,"content":[{"type":"text","text":text}]
        }}}));
    }
    fork.handle_message(Path::new("/"), &json!({"id":2,"result":{}}));
    assert_eq!(next_request(&rx)["method"], "session/new");
    assert_eq!(fork.messages.len(), 2);
    fork.handle_message(
        Path::new("/"),
        &json!({"id":2,"result":{"sessionId":"fork-session"}}),
    );
    fork.start_prompt("Next task".into()).unwrap();
    let request = next_request(&rx);
    let text = request["params"]["prompt"][0]["text"].as_str().unwrap();
    assert!(text.contains("First reply"));
    assert!(!text.contains("Later reply"));
    assert_eq!(request["params"]["sessionId"], "fork-session");
    assert!(fork.config.fork_source.is_none());
    assert!(!fork.config.fork_pending);
}

#[test]
fn restore_failure_keeps_the_harness_reference_instead_of_replacing_it() {
    let mut session = agent(ProtocolVersion::V1);
    session.config.session_id = Some("session-1".into());
    session.restoring = Some(RestoreMode::Load);
    session.handle_message(
        Path::new("/"),
        &json!({"id":2,"error":{"message":"temporarily unavailable"}}),
    );
    assert_eq!(session.status, Status::Error);
    assert_eq!(session.snapshot().session_id.as_deref(), Some("session-1"));
}

#[test]
fn fork_replay_requires_the_original_reply_when_history_has_been_trimmed() {
    let mut source = agent(ProtocolVersion::V2);
    source.log(Role::User, "Earlier question");
    source.upsert_message_text(
        Role::Agent,
        "original-reply",
        "Original reply".into(),
        Vec::new(),
        false,
    );
    let mut fork =
        SessionController::new(source.fork_config(2, 1).unwrap(), ImageStore::for_tests());
    fork.messages.clear();
    fork.upsert_message_text(
        Role::Agent,
        "different-reply",
        "Other reply".into(),
        Vec::new(),
        false,
    );
    assert!(!fork.restore_fork_history());
}

/// Run against an authenticated local adapter, for example:
/// AGENTAPS_SMOKE_COMMAND=codex-acp cargo test harness_history_reloads_after_restart -- --ignored --nocapture
#[test]
#[ignore = "requires an authenticated ACP harness and AGENTAPS_SMOKE_COMMAND"]
fn harness_history_reloads_after_restart() {
    use crate::config::{self, Config, ProjectConfig};
    use std::{fs, sync::mpsc, time::Instant};
    let command = shell_words::split(
        &std::env::var("AGENTAPS_SMOKE_COMMAND")
            .expect("Set AGENTAPS_SMOKE_COMMAND to the adapter command"),
    )
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("agentaps/config.json");
    let connect = |config: AgentConfig| {
        let (tx, rx) = mpsc::channel();
        let mut session = SessionController::new(config, ImageStore::for_tests());
        session.connection =
            Some(Connection::spawn(session.config.id, &command, temp.path(), None, tx).unwrap());
        session
            .send(
                json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":initialize_params()}),
            )
            .unwrap();
        (session, rx)
    };
    let wait_until = |session: &mut SessionController,
                      rx: &mpsc::Receiver<crate::acp::Event>,
                      ready: &dyn Fn(&SessionController) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(120);
        while !ready(session) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "Harness timed out: status {:?}",
                session.status
            );
            match rx.recv_timeout(remaining).expect("Harness did not respond") {
                crate::acp::Event::Message { value, .. } => {
                    if value.get("method").is_some() && value.get("id").is_some() {
                        panic!("Unexpected harness request: {:?}", value["method"]);
                    }
                    session.handle_message(temp.path(), &value);
                }
                crate::acp::Event::Disconnected { reason, .. } => {
                    panic!("Harness disconnected: {reason}")
                }
            }
            assert_ne!(
                session.status,
                Status::Error,
                "Harness failed: {:?}",
                session
                    .messages
                    .iter()
                    .filter(|entry| entry.role == Role::System)
                    .map(|entry| &entry.text)
                    .collect::<Vec<_>>()
            );
        }
    };
    let config: AgentConfig = serde_json::from_value(json!({"id":1,"command":command})).unwrap();
    let (mut session, rx) = connect(config);
    wait_until(&mut session, &rx, &|session| {
        session.status == Status::Idle && session.session_id.is_some()
    });
    eprintln!("Initialized harness with {:?}", session.protocol);
    let token = format!("AGENTAPS_REPLAY_{}", std::process::id());
    let prompt = format!("Reply with exactly {token}. Do not use tools or modify any files.");
    session.start_prompt(prompt.clone().into()).unwrap();
    wait_until(&mut session, &rx, &|session| {
        !session.active_work && !session.awaiting_response
    });
    assert!(
        session
            .messages
            .iter()
            .any(|entry| entry.role == Role::Agent && entry.text.contains(&token)),
        "Harness reply did not contain the smoke-test token"
    );
    let session_id = session.session_id.clone().unwrap();
    let config = Config {
        projects: vec![ProjectConfig {
            path: temp.path().to_owned(),
            ssh_host: None,
            agents: vec![session.snapshot()],
        }],
        sidebar_order: vec![1],
        ..Config::default()
    };
    config::save_to(&config_path, &config).unwrap();
    let data = fs::read(&config_path).unwrap();
    let saved_value: Value = serde_json::from_slice(&data).unwrap();
    let saved_agent = &saved_value["projects"][0]["agents"][0];
    assert!(saved_agent.get("messages").is_none());
    assert!(saved_agent.get("prompt_history").is_none());
    assert!(saved_agent.get("active_prompt").is_none());
    assert!(
        data.len() < 64 * 1024,
        "Config is unexpectedly large: {} bytes",
        data.len()
    );
    drop(session);
    drop(rx);
    let mut saved: Config = serde_json::from_slice(&data).unwrap();
    let (mut session, rx) = connect(saved.projects[0].agents.remove(0));
    assert!(session.messages.is_empty());
    wait_until(&mut session, &rx, &|session| {
        session.restoring.is_none()
            && session.status == Status::Idle
            && session.session_id.is_some()
    });
    assert_eq!(session.session_id.as_deref(), Some(session_id.as_str()));
    assert_eq!(
        session
            .messages
            .iter()
            .filter(|entry| entry.role == Role::User && entry.text == prompt)
            .count(),
        1,
        "User message was not replayed exactly once"
    );
    assert_eq!(
        session
            .messages
            .iter()
            .filter(|entry| entry.role == Role::Agent && entry.text.contains(&token))
            .count(),
        1,
        "Agent reply was not replayed exactly once"
    );
    assert!(session.config.prompt_history.contains(&prompt));
    eprintln!(
        "PASS: history replayed after adapter restart from a {} byte config without transcript text",
        data.len()
    );
}

#[test]
#[cfg(unix)]
fn prompts_send_saved_images_to_agents_that_accept_them() {
    let temp = tempfile::tempdir().unwrap();
    let mut agent = agent(ProtocolVersion::V1);
    agent.images = ImageStore::new(temp.path());
    agent.active_work = false;
    let image = agent.images.save("image/png", b"pixels").unwrap();
    let prompt = Prompt {
        text: "What is this?".into(),
        images: vec![image.clone()],
        ..Prompt::default()
    };
    assert_eq!(
        agent.start_prompt(prompt.clone()),
        Err("This agent does not accept images.".into())
    );

    agent.accepts_images = true;
    let command = vec![
        "/bin/sh".into(),
        "-c".into(),
        "while IFS= read -r line; do printf '%s\\n' \"$line\"; done".into(),
    ];
    let (events_tx, events_rx) = mpsc::channel();
    agent.connection =
        Some(Connection::spawn(1, &command, Path::new("/"), None, events_tx).unwrap());
    agent.start_prompt(prompt).unwrap();
    let Event::Message { value, .. } = events_rx.recv_timeout(Duration::from_secs(2)).unwrap()
    else {
        panic!("agent disconnected before receiving the prompt");
    };
    assert_eq!(
        value["params"]["prompt"],
        json!([
            {"type":"text","text":"What is this?"},
            {"type":"image","mimeType":"image/png","data":"cGl4ZWxz"}
        ])
    );
    let sent = agent.messages.last().unwrap();
    assert_eq!(sent.text, "What is this?");
    assert_eq!(sent.images, [image]);

    agent.handle_prompt_response(&json!({"stopReason":"end_turn"}));
    let image = agent.images.save("image/jpeg", b"photo").unwrap();
    agent
        .start_prompt(Prompt {
            text: String::new(),
            images: vec![image],
            ..Prompt::default()
        })
        .unwrap();
    let Event::Message { value, .. } = events_rx.recv_timeout(Duration::from_secs(2)).unwrap()
    else {
        panic!("agent disconnected before receiving the image");
    };
    assert_eq!(
        value["params"]["prompt"],
        json!([{"type":"image","mimeType":"image/jpeg","data":"cGhvdG8="}])
    );
}

#[test]
fn agent_images_are_saved_and_shown_with_their_message() {
    let temp = tempfile::tempdir().unwrap();
    for protocol in [ProtocolVersion::V1, ProtocolVersion::V2] {
        let mut agent = agent(protocol);
        agent.images = ImageStore::new(temp.path());
        let content = json!([
            {"type":"text","text":"Here is the chart"},
            {"type":"image","mimeType":"image/png","data":"Y2hhcnQ="},
            {"type":"image","mimeType":"image/png","data":"not base64!"}
        ]);
        let update = if protocol == ProtocolVersion::V2 {
            json!({"sessionUpdate":"agent_message","messageId":"reply","content":content})
        } else {
            json!({"sessionUpdate":"agent_message_chunk","content":content[0]})
        };
        agent.handle_update(&json!({"method":"session/update","params":{
            "sessionId":"session-1","update":update
        }}));
        if protocol == ProtocolVersion::V1 {
            for block in &content.as_array().unwrap()[1..] {
                agent.handle_update(&json!({"method":"session/update","params":{
                    "sessionId":"session-1",
                    "update":{"sessionUpdate":"agent_message_chunk","content":block}
                }}));
            }
        }
        let reply = agent.messages.last().unwrap();
        assert_eq!(reply.role, Role::Agent);
        assert_eq!(reply.images.len(), 1);
        assert_eq!(
            std::fs::read(agent.images.path(&reply.images[0])).unwrap(),
            b"chart"
        );
        assert!(reply.text.starts_with("Here is the chart"));
        assert!(reply.text.ends_with("[image]"));
        assert_eq!(reply.transcript_text().matches("[image]").count(), 2);
    }
}

#[test]
fn image_support_follows_agent_capabilities() {
    assert!(protocol::accepts_images(
        ProtocolVersion::V1,
        &json!({"agentCapabilities":{"promptCapabilities":{"image":true}}})
    ));
    assert!(!protocol::accepts_images(
        ProtocolVersion::V1,
        &json!({"agentCapabilities":{"promptCapabilities":{}}})
    ));
    assert!(protocol::accepts_images(
        ProtocolVersion::V2,
        &json!({"capabilities":{"session":{"prompt":{"image":{}}}}})
    ));
    assert!(!protocol::accepts_images(
        ProtocolVersion::V2,
        &json!({"capabilities":{"session":{"prompt":{"image":null}}}})
    ));
}

#[test]
#[cfg(unix)]
fn prompts_embed_text_files_and_link_the_rest() {
    let mut agent = agent(ProtocolVersion::V1);
    agent.active_work = false;
    let notes = ChatFile {
        name: "notes.md".into(),
        uri: "file:///project/notes.md".into(),
        text: Some("# Notes".into()),
    };
    let archive = ChatFile {
        name: "build.zip".into(),
        uri: "file:///project/build.zip".into(),
        text: None,
    };
    let prompt = Prompt {
        text: "Read these".into(),
        files: vec![notes.clone(), archive.clone()],
        ..Prompt::default()
    };
    let command = vec![
        "/bin/sh".into(),
        "-c".into(),
        "while IFS= read -r line; do printf '%s\\n' \"$line\"; done".into(),
    ];
    let (events_tx, events_rx) = mpsc::channel();
    agent.connection =
        Some(Connection::spawn(1, &command, Path::new("/"), None, events_tx).unwrap());
    let sent = || {
        let Event::Message { value, .. } = events_rx.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("agent disconnected before receiving the prompt");
        };
        value["params"]["prompt"].clone()
    };
    let link = |file: &ChatFile| json!({"type":"resource_link","uri":file.uri,"name":file.name});

    agent.start_prompt(prompt.clone()).unwrap();
    assert_eq!(
        sent(),
        json!([{"type":"text","text":"Read these"}, link(&notes), link(&archive)])
    );
    assert_eq!(
        agent.messages.last().unwrap().text,
        "Read these\n[file: notes.md]\n[file: build.zip]"
    );

    agent.handle_prompt_response(&json!({"stopReason":"end_turn"}));
    agent.accepts_embedded_context = true;
    agent
        .start_prompt(Prompt {
            text: String::new(),
            ..prompt
        })
        .unwrap();
    assert_eq!(
        sent(),
        json!([
            {"type":"resource","resource":{
                "uri":"file:///project/notes.md","mimeType":"text/plain","text":"# Notes"
            }},
            link(&archive)
        ])
    );
    assert_eq!(
        agent.messages.last().unwrap().text,
        "[file: notes.md]\n[file: build.zip]"
    );
}

#[test]
fn echoed_files_show_as_file_names() {
    let notes = url::Url::from_file_path(std::env::temp_dir().join("my notes.md")).unwrap();
    let (text, _) = protocol::message_content(&json!([
        {"type":"text","text":"Read these"},
        {"type":"resource","resource":{"uri":notes.as_str(),"text":"# Notes"}},
        {"type":"resource_link","uri":"file:///project/build.zip","name":"build.zip"}
    ]));
    assert_eq!(text, "Read these\n[file: my notes.md]\n[file: build.zip]");
}

#[test]
fn embedded_files_follow_agent_capabilities() {
    assert!(protocol::accepts_embedded_context(
        ProtocolVersion::V1,
        &json!({"agentCapabilities":{"promptCapabilities":{"embeddedContext":true}}})
    ));
    assert!(!protocol::accepts_embedded_context(
        ProtocolVersion::V1,
        &json!({"agentCapabilities":{"promptCapabilities":{}}})
    ));
    assert!(protocol::accepts_embedded_context(
        ProtocolVersion::V2,
        &json!({"capabilities":{"session":{"prompt":{"embeddedContext":{}}}}})
    ));
    assert!(!protocol::accepts_embedded_context(
        ProtocolVersion::V2,
        &json!({"capabilities":{"session":{"prompt":{}}}})
    ));
}

#[test]
fn queued_prompts_keep_images_and_load_from_older_configs() {
    let old: AgentConfig =
        serde_json::from_str(r#"{"id":1,"command":["agent"],"pending_prompts":["Next"]}"#).unwrap();
    assert_eq!(old.pending_prompts, [Prompt::from("Next")]);

    let mut config = old;
    config.pending_prompts.push(Prompt {
        text: "Look".into(),
        images: vec![ChatImage {
            mime_type: "image/png".into(),
            sha256: "abc".into(),
        }],
        ..Prompt::default()
    });
    let saved = serde_json::to_value(&config).unwrap();
    assert_eq!(
        saved["pending_prompts"],
        json!([{"text":"Next"},{"text":"Look","images":[{"mime_type":"image/png","sha256":"abc"}]}])
    );
    let restored: AgentConfig = serde_json::from_value(saved).unwrap();
    assert_eq!(restored.pending_prompts, config.pending_prompts);
}

#[test]
fn echoed_file_context_renders_as_truncated_code_block() {
    let body: String = (1..=10).map(|n| format!("line {n}\n")).collect();
    let message = format!(
        "Summarise [@notes.md](file:///tmp/notes.md)\n<context ref=\"file:///tmp/notes.md\">\n# Title\n```rust\n{body}</context>\nthanks"
    );
    let preview = file_context_preview(&message);
    assert_eq!(
        preview,
        "Summarise [@notes.md](file:///tmp/notes.md)\n````\n# Title\n```rust\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\n… 4 more lines\n````\nthanks"
    );
    assert_eq!(file_context_preview("plain text"), "plain text");
}
