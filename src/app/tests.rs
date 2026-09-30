use super::*;

#[test]
fn diff_file_expands_beneath_its_row_without_hiding_other_files() {
    use crate::diff_view::{Hunk, Line, Mark, Side};

    let files = [
        DiffFile {
            path: "first.txt".into(),
            hunks: vec![Hunk {
                header: "@@ -0,0 +1 @@".into(),
                lines: vec![Line {
                    old: None,
                    new: Some(Side {
                        number: 1,
                        text: "added line".into(),
                        mark: Mark::Added,
                    }),
                }],
            }],
            note: None,
        },
        DiffFile {
            path: "second.txt".into(),
            hunks: Vec::new(),
            note: Some("Second file note".into()),
        },
    ];
    let stats = [(1, 0), (0, 0)];
    let rows = diff_list_rows(&files, &stats, Some("first.txt"), DiffPresentation::Unified);
    assert!(matches!(rows[0], DiffListRow::Summary { files: 2, .. }));
    assert!(
        matches!(&rows[1], DiffListRow::File { path, expanded: true, .. } if path == "first.txt")
    );
    assert!(
        matches!(&rows[2], DiffListRow::Content(DiffRow::Hunk(header)) if header == "@@ -0,0 +1 @@")
    );
    assert!(
        matches!(&rows[3], DiffListRow::Content(DiffRow::Unified { side, .. }) if side.text == "added line")
    );
    assert!(
        matches!(&rows[4], DiffListRow::File { path, expanded: false, .. } if path == "second.txt")
    );
    assert_eq!(rows.len(), 5);

    let collapsed = diff_list_rows(&files, &stats, None, DiffPresentation::Unified);
    assert_eq!(collapsed.len(), 3);
}

#[test]
fn branch_labels_unborn_and_detached_heads_without_git() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("agentaps-branch-{}-{stamp}", std::process::id()));
    let repo = gix::init(&path).unwrap();
    std::fs::write(repo.git_dir().join("HEAD"), "ref: refs/heads/topic\n").unwrap();
    assert_eq!(branch(&path), "topic");

    let id = repo.write_blob(b"detached head fixture").unwrap();
    std::fs::write(repo.git_dir().join("HEAD"), format!("{id}\n")).unwrap();
    let detached = branch(&path);
    assert!(id.to_string().starts_with(&detached));
    assert!(detached.len() < id.to_string().len());

    drop(repo);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn session_search_prefers_direct_name_matches() {
    let direct = session_search_score(
        "agentaps",
        Path::new("/dev/agentaps"),
        "main",
        "Codex",
        None,
    );
    let path_only = session_search_score(
        "agentaps",
        Path::new("/dev/agentaps/examples"),
        "main",
        "Codex",
        None,
    );
    assert!(direct > path_only);
    assert!(
        session_search_score("codex", Path::new("/dev/project"), "main", "Codex", None).is_some()
    );
    assert!(
        session_search_score(
            "login",
            Path::new("/dev/project"),
            "main",
            "Codex",
            Some("Fix login flow")
        )
        .is_some()
    );
    assert_eq!(
        session_search_score("missing", Path::new("/dev/project"), "main", "Codex", None),
        None
    );
}

#[test]
fn workspace_view_keeps_sidebar_selection_exclusive() {
    let first = SessionLocation {
        project_index: 0,
        agent_index: 0,
    };
    let second = SessionLocation {
        project_index: 0,
        agent_index: 1,
    };
    let conversation = WorkspaceView::Conversation(first);
    assert_eq!(conversation.highlighted_session(), Some(first));
    let archive = conversation.toggle_archive();
    assert_eq!(archive.highlighted_session(), None);
    assert_eq!(archive.displayed_session(), Some(first));
    assert_eq!(archive.toggle_archive(), conversation);

    let folders = conversation.open_picker(PickerStep::Folders);
    assert_eq!(folders.highlighted_session(), None);
    assert_eq!(folders.displayed_session(), None);
    assert_eq!(folders.return_to(), Some(first));

    let agents = folders.open_picker(PickerStep::Agents { project_index: 0 });
    assert_eq!(agents.highlighted_session(), None);
    assert_eq!(agents.return_to(), Some(first));

    assert_eq!(agents.toggle_archive(), archive);

    let updated = agents.session_archived(first, Some(second));
    assert_eq!(updated.highlighted_session(), None);
    assert_eq!(updated.return_to(), Some(second));
    assert_eq!(
        updated.toggle_archive().toggle_archive(),
        WorkspaceView::Conversation(second)
    );
    assert_eq!(
        conversation.session_archived(first, None),
        WorkspaceView::Empty
    );
}

#[test]
fn sidebar_icons_are_bundled() {
    assert!(
        gpui_kit::AssetSource::load(&AppAssets, "icons/inbox.svg")
            .unwrap()
            .is_some()
    );
    assert!(
        gpui_kit::AssetSource::load(&AppAssets, "icons/undo-2.svg")
            .unwrap()
            .is_some()
    );
    assert!(
        gpui_kit::AssetSource::load(&AppAssets, "icons/mobile.svg")
            .unwrap()
            .is_some()
    );
}

#[test]
fn prompt_recall_walks_history_and_restores_draft() {
    let history = vec!["first".into(), "second\nline".into()];
    let mut recall = None;
    assert_eq!(
        PromptRecall::step(&mut recall, 7, &history, "draft", false),
        None
    );
    assert_eq!(
        PromptRecall::step(&mut recall, 7, &history, "draft", true),
        Some("second\nline".into())
    );
    assert_eq!(
        PromptRecall::step(&mut recall, 7, &history, "second\nline", true),
        Some("first".into())
    );
    assert_eq!(
        PromptRecall::step(&mut recall, 7, &history, "first", true),
        Some("first".into())
    );
    assert_eq!(
        PromptRecall::step(&mut recall, 7, &history, "first", false),
        Some("second\nline".into())
    );
    assert_eq!(
        PromptRecall::step(&mut recall, 7, &history, "second\nline", false),
        Some("draft".into())
    );
    assert_eq!(
        PromptRecall::step(&mut recall, 8, &history, "other draft", true),
        Some("second\nline".into())
    );
    assert_eq!(recall.unwrap().draft, "other draft");
}

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
    let restored = AgentView::new(config);
    assert_eq!(restored.config.prompt_history, ["sent", "queued"]);
}

#[test]
fn interrupted_turn_waits_for_recovery_before_queued_prompts() {
    let mut config = agent(ProtocolVersion::V1).snapshot();
    config.was_working = true;
    config.prompt_history = vec!["Earlier".into(), "Active request".into(), "Queued".into()];
    config.pending_prompts.push("Queued".into());
    let mut restored = AgentView::new(config);
    restored.status = Status::Idle;
    restored.session_id = Some("session-1".into());

    assert!(restored.snapshot().was_working);
    assert!(restored.continuation_prompt().contains("Active request"));
    assert!(!restored.continuation_prompt().contains("Queued"));
    assert!(!restored.start_next_queued_prompt());

    restored.config.prompt_history.push("Continuation".into());
    let saved = serde_json::to_vec(&restored.snapshot()).unwrap();
    let reloaded = AgentView::new(serde_json::from_slice(&saved).unwrap());
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
    let mut restored = AgentView::new(config);
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
    let mut restored = AgentView::new(config);
    restored.protocol = Some(ProtocolVersion::V2);
    restored.session_id = Some("session-1".into());
    restored.status = Status::Idle;
    restored.recovery_due = Some(Instant::now() - Duration::from_secs(1));

    Workspace::handle_update(
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

fn agent(protocol: ProtocolVersion) -> AgentView {
    let mut agent = AgentView::new(AgentConfig {
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
        Workspace::handle_update(
            &mut agent,
            &json!({"params":{"sessionId":"session-1","update":{
                "sessionUpdate":"session_info_update","title":"  Fix   login flow  "
            }}}),
        );
        assert_eq!(agent.config.title.as_deref(), Some("Fix login flow"));

        Workspace::handle_update(
            &mut agent,
            &json!({"params":{"sessionId":"other-session","update":{
                "sessionUpdate":"session_info_update","title":"Unrelated work"
            }}}),
        );
        assert_eq!(agent.config.title.as_deref(), Some("Fix login flow"));

        let restored = AgentView::new(agent.snapshot());
        assert_eq!(restored.config.title.as_deref(), Some("Fix login flow"));

        Workspace::handle_update(
            &mut agent,
            &json!({"params":{"sessionId":"session-1","update":{
                "sessionUpdate":"session_info_update","updatedAt":"2026-09-28T00:00:00Z"
            }}}),
        );
        assert_eq!(agent.config.title.as_deref(), Some("Fix login flow"));

        Workspace::handle_update(
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

    Workspace::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"agent_message","messageId":"agent-1","content":[{"type":"text","text":"Hello"}]
        }}}),
    );
    Workspace::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"agent_message_chunk","messageId":"agent-1","content":{"type":"text","text":" world"}
        }}}),
    );
    assert_eq!(agent.messages.len(), 1);
    assert_eq!(agent.messages[0].text, "Hello world");
    assert!(!agent.awaiting_response);

    Workspace::handle_update(
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
    let mut reset = AgentView::new(serde_json::from_slice(&saved).unwrap());
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
fn fork_supplies_only_active_conversation_to_first_prompt() {
    let mut source = agent(ProtocolVersion::V1);
    source.log(Role::User, "Old question");
    source.log(Role::ContextReset, "Context reset");
    source.log(Role::User, "Current question");
    source.log(Role::Tool, "Reading file");
    source.log(Role::Agent, "Current answer");
    let config = source.fork_config(2, 4).unwrap();
    let mut fork = AgentView::new(config);
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
fn sidebar_drag_moves_in_both_directions() {
    let mut order = vec![1, 2, 3, 4];
    assert!(move_sidebar_id(&mut order, 1, 3));
    assert_eq!(order, vec![2, 3, 1, 4]);
    assert!(move_sidebar_id(&mut order, 4, 2));
    assert_eq!(order, vec![4, 2, 3, 1]);
    assert!(!move_sidebar_id(&mut order, 4, 4));
}

#[test]
fn adjacent_tool_calls_form_one_group() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.log(Role::Tool, "first tool");
    agent.log(Role::Tool, "second tool");
    agent.log(Role::Agent, "answer");
    agent.log(Role::Tool, "later tool");
    assert_eq!(tool_run_end(&agent.messages, 0), 2);
    assert_eq!(tool_run_end(&agent.messages, 3), 4);
    agent.config.archived = true;
    assert!(agent.snapshot().archived);
}

#[test]
fn tool_activity_describes_search_read_and_review() {
    assert_eq!(
        tool_description("rg -n 'guardian|review|tool call' src"),
        ("Search guardian|review|tool call in src".into(), true)
    );
    assert_eq!(
        tool_description("/bin/bash -lc \"sed -n '1,80p' src/main.rs\""),
        ("Read src/main.rs".into(), true)
    );
    assert_eq!(
        tool_description("cd /project && head -80 README.md && ls && git log --oneline | wc -l"),
        ("Inspect project (4 steps)".into(), true)
    );
    assert_eq!(
        tool_description("cat src/app.rs && cat src/app/tests.rs && cat src/app/render.rs"),
        ("Read files (3 steps)".into(), true)
    );
    assert_eq!(
        tool_description("cargo fmt --all -- --check && git diff --check"),
        ("Check formatting · Check patch whitespace".into(), true)
    );
    assert_eq!(
        tool_description("cargo test --locked file_mention_completes_at_cursor"),
        ("Run targeted test".into(), true)
    );
    let mut agent = agent(ProtocolVersion::V1);
    agent.log(Role::Tool, "rg -n guardian src");
    agent.log(Role::Tool, "Guardian Review");
    agent.log(Role::Tool, "cat README.md");
    assert_eq!(tool_group_heading(&agent.messages), "Activity");
    assert_eq!(markdown_code_block("a ``` b"), "````\na ``` b\n````");
}

#[test]
fn tool_group_heading_tracks_live_and_finished_steps() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.log(Role::Tool, "cat src/app.rs · completed");
    agent.log(Role::Tool, "cargo test --locked cursor_test · in_progress");
    agent.log(Role::Tool, "Guardian Review · completed");
    assert_eq!(
        tool_group_heading(&agent.messages),
        "Working · Running targeted test"
    );
    agent.messages[1].text = "cargo test --locked cursor_test · completed".into();
    assert_eq!(tool_group_heading(&agent.messages), "Completed");
    agent.log(Role::Tool, "Using tool · in_progress");
    assert_eq!(tool_group_heading(&agent.messages), "Working · Using tool");
    agent.messages.pop();
    agent.messages[2].text = "Guardian Review · pending".into();
    assert_eq!(tool_group_heading(&agent.messages), "Completed");
    agent.messages[2].text = "Guardian Review · failed".into();
    assert_eq!(tool_group_heading(&agent.messages), "Completed");
}

#[test]
fn v1_tool_updates_replace_the_same_call_and_keep_approval_status() {
    let mut review_agent = agent(ProtocolVersion::V1);
    for update in [
        json!({"sessionUpdate":"tool_call","toolCallId":"guardian-1",
            "title":"Guardian Review","status":"in_progress"}),
        json!({"sessionUpdate":"tool_call_update","toolCallId":"guardian-1",
            "status":"completed"}),
    ] {
        Workspace::handle_update(
            &mut review_agent,
            &json!({"params":{"sessionId":"session-1","update":update}}),
        );
    }
    assert_eq!(review_agent.messages.len(), 1);
    assert_eq!(review_agent.messages[0].text, "Guardian Review · completed");
    assert_eq!(tool_group_heading(&review_agent.messages), "Activity");
    let mut tool = agent(ProtocolVersion::V2);
    tool.upsert_tool_call(
        &json!({"toolCallId":"command-1","title":"cat README.md","status":"in_progress"}),
    );
    tool.messages[0].text.push_str("\noutput text");
    tool.upsert_tool_call(&json!({"toolCallId":"command-1","status":"completed"}));
    assert_eq!(
        tool.messages[0].text,
        "cat README.md · completed\noutput text"
    );
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
    Workspace::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"config_option_update","configOptions":options
        }}}),
    );
    assert_eq!(agent.model.as_deref(), Some("Opus (1M context)"));
    assert_eq!(agent.effort_option.as_ref().unwrap().label(), "High");
    Workspace::handle_update(
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
    let restored = AgentView::new(serde_json::from_slice(&encoded).unwrap());
    assert_eq!(restored.config.session_id.as_deref(), Some("session-1"));
    assert_eq!(restored.model.as_deref(), Some("Example Model"));
    assert_eq!(restored.context, Some((3_000, 64_000)));
    assert_eq!(restored.messages[0].text, "Please check the build");
    assert_eq!(restored.messages[1].text, "cargo test · completed");
    assert!(restored.messages[2].text.contains("active"));
    assert_eq!(restored.config.pending_prompts, vec!["Follow up next"]);
}

#[test]
fn file_mention_completes_at_cursor_without_changing_surrounding_text() {
    let draft = "Check 🦀 @src/ma and @other";
    let cursor = Position::new(0, "Check 🦀 @src/ma".encode_utf16().count() as u32);
    let (range, query) = file_mention(draft, cursor).unwrap();
    assert_eq!(query, "src/ma");
    let (completed, position) = completed_file_text(draft, range, "src/main.rs");
    assert_eq!(completed, "Check 🦀 @src/main.rs and @other");
    assert_eq!(
        position.character,
        "Check 🦀 @src/main.rs ".encode_utf16().count() as u32
    );
    assert!(file_mention(&completed, position).is_none());
    let draft = "Review @src/main soon";
    let cursor = Position::new(0, "Review @src/ma".encode_utf16().count() as u32);
    let (range, query) = file_mention(draft, cursor).unwrap();
    assert_eq!(query, "src/ma");
    let (completed, position) = completed_file_text(draft, range, "src/main.rs");
    assert_eq!(completed, "Review @src/main.rs soon");
    assert!(file_mention(&completed, position).is_none());
    let draft = "Review @src/ma";
    let cursor = Position::new(0, draft.encode_utf16().count() as u32);
    let (range, _) = file_mention(draft, cursor).unwrap();
    let (completed, position) = completed_file_text(draft, range, "src/main.rs");
    assert_eq!(completed, "Review @src/main.rs ");
    assert!(file_mention(&completed, position).is_none());
    assert!(file_mention("email@example.com", Position::new(0, 17)).is_none());
}

#[test]
fn slash_commands_follow_agent_snapshots_and_complete_with_input_hint() {
    let mut agent = agent(ProtocolVersion::V1);
    Workspace::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"available_commands_update","availableCommands":[
                {"name":"review","description":"Review changes","input":{"hint":"files"}},
                {"name":"reset","description":"Reset chat"}
            ]
        }}}),
    );
    assert_eq!(
        matching_slash_commands(&agent.config.available_commands, "/re").len(),
        2
    );
    assert_eq!(
        matching_slash_commands(&agent.config.available_commands, "/review ").len(),
        0
    );
    assert_eq!(
        completed_slash_text(&agent.config.available_commands[0]),
        "/review "
    );
    assert_eq!(
        completed_slash_text(&agent.config.available_commands[1]),
        "/reset"
    );

    Workspace::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"available_commands_update","availableCommands":[
                {"name":"plan","description":"Plan work"}
            ]
        }}}),
    );
    assert_eq!(
        matching_slash_commands(&agent.config.available_commands, "/re").len(),
        0
    );

    let mut v2_agent = self::agent(ProtocolVersion::V2);
    Workspace::handle_update(
        &mut v2_agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"available_commands_update","availableCommands":[
                {"name":"plan","description":"Plan work"}
            ]
        }}}),
    );
    assert_eq!(v2_agent.config.available_commands[0].name, "plan");
}

#[test]
fn queued_prompt_stays_saved_when_agent_cannot_send_it() {
    let mut agent = agent(ProtocolVersion::V2);
    agent.config.pending_prompts.push("Next request".into());
    assert!(!agent.start_next_queued_prompt());
    Workspace::handle_update(
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
    Workspace::handle_update(
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
    Workspace::start_new_session(&mut agent, Path::new("/"));
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
    Workspace::handle_update(
        &mut agent,
        &json!({"params":{"sessionId":"session-1","update":{
            "sessionUpdate":"state_update","state":"idle","stopReason":"end_turn"
        }}}),
    );
    assert!(agent.active_work);
    assert!(agent.awaiting_response);
}

#[test]
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
    Workspace::handle_update(
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

#[test]
fn submitting_keeps_explicit_newlines() {
    assert_eq!(submitted_prompt("first\nsecond\n"), "first\nsecond");
    assert_eq!(submitted_prompt("first\n\n"), "first\n");
}

#[test]
fn shell_mode_requires_first_character_and_preserves_command_text() {
    assert_eq!(
        shell_command("!  printf 'hello'\n"),
        Some("  printf 'hello'\n")
    );
    assert_eq!(shell_command(" !printf hello"), None);
    assert_eq!(shell_command("! \n"), None);
    assert_eq!(
        shell_command_in_message("!printf hello"),
        Some("printf hello")
    );
    assert_eq!(
        shell_command_in_message(&prompt_for_agent("!printf hello")),
        Some("printf hello")
    );
    assert_eq!(prompt_for_agent("ask ! for help"), "ask ! for help");
    assert_eq!(
        prompt_for_agent("!  printf 'hello'\nnext"),
        "Run this shell command exactly as written, then report its output:\n\n  printf 'hello'\nnext"
    );
}

#[gpui_kit::test]
fn zoom_shortcuts_and_menu_actions_change_the_font_scale(cx: &mut gpui_kit::TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let original_config_home = std::env::var_os("XDG_CONFIG_HOME");
    // SAFETY: no other test in this process reads the config directory while this
    // test runs; Workspace::new is only constructed here.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", temp.path()) };
    cx.update(|cx| {
        gpui_kit::init(cx);
        theme::apply(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-=", ZoomIn, None),
            KeyBinding::new("cmd--", ZoomOut, None),
            KeyBinding::new("cmd-0", ZoomReset, None),
        ]);
    });
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(window, cx));
    // The zoom must land on the theme: in the real app the gpui-component
    // Root plugin resets window.rem_size to theme.font_size before every
    // frame, so theme.font_size is what actually reaches the paint.
    let zoom_state = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            let theme = Theme::global(cx);
            (
                workspace.read(cx).font_scale,
                theme.font_size,
                theme.mono_font_size,
            )
        })
    };

    assert_eq!(
        zoom_state(cx),
        (1.0, px(BASE_FONT_SIZE), px(BASE_MONO_FONT_SIZE))
    );
    let default_max_rows = cx.update(|_, cx| workspace.read(cx).composer_max_rows);
    cx.simulate_keystrokes("cmd-=");
    assert_eq!(
        zoom_state(cx),
        (1.1, px(BASE_FONT_SIZE * 1.1), px(BASE_MONO_FONT_SIZE * 1.1)),
        "Cmd+= should zoom in"
    );
    let (saved, _) = config::load().expect("zooming should save the config");
    assert_eq!(saved.font_scale, 1.1, "Cmd+= should persist the zoom level");
    assert!(
        cx.update(|_, cx| workspace.read(cx).composer_max_rows) < default_max_rows,
        "zooming in should reserve more room above a long draft"
    );
    cx.simulate_keystrokes("cmd--");
    assert_eq!(
        zoom_state(cx),
        (1.0, px(BASE_FONT_SIZE), px(BASE_MONO_FONT_SIZE)),
        "Cmd+- should zoom out"
    );
    cx.dispatch_action(ZoomIn);
    assert_eq!(
        zoom_state(cx),
        (1.1, px(BASE_FONT_SIZE * 1.1), px(BASE_MONO_FONT_SIZE * 1.1)),
        "menu-dispatched ZoomIn should zoom in"
    );
    cx.dispatch_action(ZoomReset);
    assert_eq!(
        zoom_state(cx),
        (1.0, px(BASE_FONT_SIZE), px(BASE_MONO_FONT_SIZE)),
        "menu-dispatched ZoomReset should reset"
    );
    let (saved, _) = config::load().expect("zooming should save the config");
    assert_eq!(saved.font_scale, 1.0, "ZoomReset should persist the reset");

    // The acknowledgement notice hides itself after ZOOM_NOTICE_TIMEOUT.
    let notice =
        |cx: &mut gpui_kit::VisualTestContext| cx.update(|_, cx| workspace.read(cx).notice.clone());
    cx.dispatch_action(ZoomIn);
    cx.dispatch_action(ZoomIn);
    assert_eq!(notice(cx), Some("Zoom 120%".to_string()));
    cx.executor().advance_clock(ZOOM_NOTICE_TIMEOUT);
    cx.run_until_parked();
    assert_eq!(
        notice(cx),
        None,
        "zoom notice should hide after the timeout"
    );

    // A later error or unrelated notice must survive the zoom timer.
    cx.dispatch_action(ZoomIn);
    cx.update(|_, cx| workspace.update(cx, |this, _| this.notice = Some("Another notice".into())));
    cx.executor().advance_clock(ZOOM_NOTICE_TIMEOUT);
    cx.run_until_parked();
    assert_eq!(notice(cx), Some("Another notice".to_string()));

    let blocked = temp.path().join("blocked");
    std::fs::write(&blocked, "").unwrap();
    // SAFETY: this test is the only test constructing Workspace in this process.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &blocked) };
    cx.dispatch_action(ZoomIn);
    assert!(notice(cx).unwrap().starts_with("Could not save config:"));
    cx.executor().advance_clock(ZOOM_NOTICE_TIMEOUT);
    cx.run_until_parked();
    assert!(notice(cx).unwrap().starts_with("Could not save config:"));

    // Restore the saved zoom in a fresh workspace after resetting the theme.
    // SAFETY: this test is the only test constructing Workspace in this process.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", temp.path()) };
    cx.update(|_, cx| theme::apply(cx));
    let (restored, cx) = cx.add_window_view(|window, cx| Workspace::new(window, cx));
    cx.update(|_, cx| {
        let restored = restored.read(cx);
        assert_eq!(restored.font_scale, 1.3);
        assert_eq!(Theme::global(cx).font_size, px(BASE_FONT_SIZE * 1.3));
        assert_eq!(
            Theme::global(cx).mono_font_size,
            px(BASE_MONO_FONT_SIZE * 1.3)
        );
        assert!(
            restored.composer_max_rows < default_max_rows,
            "restoring zoom should also limit the composer height"
        );
    });

    // No-op reset must not schedule a timer that removes another notice.
    cx.dispatch_action(ZoomReset);
    cx.executor().advance_clock(ZOOM_NOTICE_TIMEOUT);
    cx.run_until_parked();
    cx.update(|_, cx| restored.update(cx, |this, _| this.notice = Some("Keep this notice".into())));
    cx.dispatch_action(ZoomReset);
    cx.executor().advance_clock(ZOOM_NOTICE_TIMEOUT);
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| restored.read(cx).notice.clone()),
        Some("Keep this notice".to_string())
    );
    // SAFETY: restore the process environment modified for this test.
    unsafe {
        if let Some(original) = original_config_home {
            std::env::set_var("XDG_CONFIG_HOME", original);
        } else {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }
}
