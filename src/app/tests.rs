use super::*;
use crate::session::test_agent;

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
    assert!(
        matches!(&rows[0], DiffListRow::File { path, expanded: true, .. } if path == "first.txt")
    );
    assert!(
        matches!(&rows[1], DiffListRow::Content(DiffRow::Hunk(header)) if header == "@@ -0,0 +1 @@")
    );
    assert!(
        matches!(&rows[2], DiffListRow::Content(DiffRow::Unified { side, .. }) if side.text == "added line")
    );
    assert!(
        matches!(&rows[3], DiffListRow::File { path, expanded: false, .. } if path == "second.txt")
    );
    assert_eq!(rows.len(), 4);

    let collapsed = diff_list_rows(&files, &stats, None, DiffPresentation::Unified);
    assert_eq!(collapsed.len(), 2);
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

    let changing_folder = conversation.open_picker(PickerStep::ChangeFolder { session: first });
    assert_eq!(changing_folder.return_to(), Some(first));
    assert_eq!(changing_folder.displayed_session(), None);
    assert_eq!(
        changing_folder.session_archived(first, Some(second)),
        WorkspaceView::NewSession {
            step: PickerStep::Folders,
            return_to: Some(second),
        }
    );

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
    let mut agent = test_agent(ProtocolVersion::V2);
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
    let mut agent = test_agent(ProtocolVersion::V1);
    agent.log(Role::Tool, "rg -n guardian src");
    agent.log(Role::Tool, "Guardian Review");
    agent.log(Role::Tool, "cat README.md");
    assert_eq!(tool_group_heading(&agent.messages), "Activity");
    assert_eq!(markdown_code_block("a ``` b"), "````\na ``` b\n````");
}

#[test]
fn tool_group_heading_tracks_live_and_finished_steps() {
    let mut agent = test_agent(ProtocolVersion::V2);
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
    let mut agent = test_agent(ProtocolVersion::V1);
    SessionController::handle_update(
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

    SessionController::handle_update(
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

    let mut v2_agent = self::test_agent(ProtocolVersion::V2);
    SessionController::handle_update(
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
    config::save_to(
        &temp.path().join("agentaps/config.json"),
        &Config::default(),
    )
    .unwrap();
    let config_env = if cfg!(windows) {
        "APPDATA"
    } else {
        "XDG_CONFIG_HOME"
    };
    let original_config_home = std::env::var_os(config_env);
    // SAFETY: no other test in this process reads the config directory while this
    // test runs; Workspace::new is only constructed here.
    unsafe { std::env::set_var(config_env, temp.path()) };
    cx.update(|cx| {
        cx.set_app_identity(APP_IDENTIFIER, "Agentaps");
        gpui_kit::init(cx);
        theme::apply(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-=", ZoomIn, None),
            KeyBinding::new("cmd--", ZoomOut, None),
            KeyBinding::new("cmd-0", ZoomReset, None),
        ]);
        cx.bind_keys(key_bindings());
    });
    let (workspace, cx) = cx.add_window_view(Workspace::new);
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
    let default_max_rows = cx.update(|_, cx| workspace.read(cx).conversation.max_rows);
    cx.simulate_keystrokes("cmd-=");
    assert_eq!(
        zoom_state(cx),
        (1.1, px(BASE_FONT_SIZE * 1.1), px(BASE_MONO_FONT_SIZE * 1.1)),
        "Cmd+= should zoom in"
    );
    cx.update(|_, cx| workspace.update(cx, |this, _| this.persistence.wait().unwrap()));
    let (saved, _) = config::load().expect("zooming should save the config");
    assert_eq!(saved.font_scale, 1.1, "Cmd+= should persist the zoom level");
    assert!(
        cx.update(|_, cx| workspace.read(cx).conversation.max_rows) < default_max_rows,
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
    cx.update(|_, cx| workspace.update(cx, |this, _| this.persistence.wait().unwrap()));
    let (saved, _) = config::load().expect("zooming should save the config");
    assert_eq!(saved.font_scale, 1.0, "ZoomReset should persist the reset");

    // The acknowledgement notice hides itself after ZOOM_NOTICE_TIMEOUT.
    let notice = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            workspace
                .read(cx)
                .notice
                .as_ref()
                .map(|notice| notice.message().to_owned())
        })
    };
    cx.dispatch_action(ZoomIn);
    cx.dispatch_action(ZoomIn);
    assert!(cx.update(|_, cx| matches!(workspace.read(cx).notice, Some(Notice::Info(_)))));
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
    cx.update(|_, cx| {
        workspace.update(cx, |this, _| {
            this.notice = Some(Notice::Error("Another notice".into()))
        })
    });
    cx.executor().advance_clock(ZOOM_NOTICE_TIMEOUT);
    cx.run_until_parked();
    assert_eq!(notice(cx), Some("Another notice".to_string()));

    // Git results carry their severity into the banner and can be dismissed.
    for (action, result, expected) in [
        (
            crate::git_sync::SyncAction::Pull,
            Ok((0, 0)),
            Notice::Info("Commits pulled.".into()),
        ),
        (
            crate::git_sync::SyncAction::Push,
            Ok((0, 0)),
            Notice::Info("Commits pushed.".into()),
        ),
        (
            crate::git_sync::SyncAction::Pull,
            Err("connection lost".into()),
            Notice::Error("Could not sync commits: connection lost".into()),
        ),
    ] {
        cx.update(|_, cx| {
            workspace.update(cx, |this, cx| {
                this.sync
                    .tx
                    .send(SyncUpdate::OperationFinished {
                        path: temp.path().to_owned(),
                        host: None,
                        action,
                        result,
                        counts: None,
                    })
                    .unwrap();
                this.poll_sync_counts(cx);
                assert_eq!(this.notice, Some(expected));
                this.dismiss_notice(cx);
                assert_eq!(this.notice, None);
            });
        });
    }

    // Theme changes must retain zoom, save the selection, and restore it on launch.
    cx.update(|_, cx| {
        workspace.update(cx, |this, cx| {
            this.select_theme(crate::appearance::Choice::SolarizedLight, cx);
            assert_eq!(
                Theme::global(cx).mode,
                gpui_kit::component::theme::ThemeMode::Light
            );
            assert_eq!(Theme::global(cx).font_size, px(BASE_FONT_SIZE * 1.3));
            assert_eq!(theme::palette(cx).color(BG), Theme::global(cx).background);
        })
    });
    cx.update(|_, cx| workspace.update(cx, |this, _| this.persistence.wait().unwrap()));
    assert_eq!(
        config::load().unwrap().0.theme,
        crate::appearance::Choice::SolarizedLight
    );

    // Finish the previous snapshot before replacing the writer with a failing one.
    cx.update(|_, cx| workspace.update(cx, |this, _| this.persistence.wait().unwrap()));
    let blocked = temp.path().join("blocked");
    std::fs::write(&blocked, "").unwrap();
    cx.update(|_, cx| {
        workspace.update(cx, |this, _| {
            this.persistence =
                crate::persistence::Persistence::at_path(false, Ok(blocked.join("config.json")));
        })
    });
    cx.dispatch_action(ZoomIn);
    cx.update(|_, cx| {
        workspace.update(cx, |this, _| {
            let error = this.persistence.wait().unwrap_err();
            this.notice = Some(Notice::Error(format!("Could not save config: {error}")));
        })
    });
    assert!(notice(cx).unwrap().starts_with("Could not save config:"));
    cx.executor().advance_clock(ZOOM_NOTICE_TIMEOUT);
    cx.run_until_parked();
    assert!(notice(cx).unwrap().starts_with("Could not save config:"));

    // Restore the saved zoom in a fresh workspace after resetting the theme.
    // SAFETY: this test is the only test constructing Workspace in this process.
    unsafe { std::env::set_var(config_env, temp.path()) };
    cx.update(|_, cx| theme::apply(cx));
    let (restored, cx) = cx.add_window_view(Workspace::new);
    cx.update(|_, cx| {
        let restored = restored.read(cx);
        assert_eq!(restored.font_scale, 1.3);
        assert_eq!(
            restored.theme_choice,
            crate::appearance::Choice::SolarizedLight
        );
        assert_eq!(
            Theme::global(cx).mode,
            gpui_kit::component::theme::ThemeMode::Light
        );
        assert_eq!(Theme::global(cx).font_size, px(BASE_FONT_SIZE * 1.3));
        assert_eq!(
            Theme::global(cx).mono_font_size,
            px(BASE_MONO_FONT_SIZE * 1.3)
        );
        assert!(
            restored.conversation.max_rows < default_max_rows,
            "restoring zoom should also limit the composer height"
        );
    });

    // No-op reset must not schedule a timer that removes another notice.
    cx.dispatch_action(ZoomReset);
    cx.executor().advance_clock(ZOOM_NOTICE_TIMEOUT);
    cx.run_until_parked();
    cx.update(|_, cx| {
        restored.update(cx, |this, _| {
            this.notice = Some(Notice::Error("Keep this notice".into()))
        })
    });
    cx.dispatch_action(ZoomReset);
    cx.executor().advance_clock(ZOOM_NOTICE_TIMEOUT);
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| restored
            .read(cx)
            .notice
            .as_ref()
            .map(|notice| notice.message().to_owned())),
        Some("Keep this notice".to_string())
    );
    cx.update(|_, cx| restored.update(cx, |this, _| this.persistence.wait().unwrap()));
    mobile::verify_session_switching_and_mobile_routing(&restored, temp.path(), cx);
    verify_resets_and_folder_moves_preserve_harness_references(&restored, temp.path(), cx);
    verify_project_session_sidebar(&restored, cx);
    verify_project_agent_dialog(&restored, cx);
    verify_inline_session_renaming(&restored, cx);
    verify_workspace_shortcuts(&restored, cx);
    verify_settings_apply_and_persist(&restored, temp.path(), cx);
    verify_key_bindings(&restored, cx);
    verify_notifications_for_background_sessions(&restored, temp.path(), cx);
    verify_repository_cloning(&restored, temp.path(), cx);
    verify_split_panes(&restored, temp.path(), cx);
    verify_project_archiving(&restored, cx);
    // SAFETY: restore the process environment modified for this test.
    unsafe {
        if let Some(original) = original_config_home {
            std::env::set_var(config_env, original);
        } else {
            std::env::remove_var(config_env);
        }
    }
}

#[test]
fn v1_tool_updates_replace_the_same_call_and_keep_approval_status() {
    let mut review_agent = test_agent(ProtocolVersion::V1);
    for update in [
        json!({"sessionUpdate":"tool_call","toolCallId":"guardian-1",
            "title":"Guardian Review","status":"in_progress"}),
        json!({"sessionUpdate":"tool_call_update","toolCallId":"guardian-1",
            "status":"completed"}),
    ] {
        SessionController::handle_update(
            &mut review_agent,
            &json!({"params":{"sessionId":"session-1","update":update}}),
        );
    }
    assert_eq!(review_agent.messages.len(), 1);
    assert_eq!(review_agent.messages[0].text, "Guardian Review · completed");
    assert_eq!(tool_group_heading(&review_agent.messages), "Activity");
    let mut tool = test_agent(ProtocolVersion::V2);
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

fn verify_resets_and_folder_moves_preserve_harness_references(
    workspace: &Entity<Workspace>,
    path: &Path,
    cx: &mut gpui_kit::VisualTestContext,
) {
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let mut original = test_agent(ProtocolVersion::V2);
            original.config.command.clear(); // The folder move should not launch another harness.
            original.active_work = false;
            original.status = Status::Idle;
            let command = if cfg!(windows) {
                vec![
                    "powershell".into(),
                    "-NoProfile".into(),
                    "-Command".into(),
                    "$input | ForEach-Object { $_ }".into(),
                ]
            } else {
                vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "while IFS= read -r request; do printf '%s\\n' \"$request\"; done".into(),
                ]
            };
            let (events_tx, _events_rx) = mpsc::channel();
            original.connection =
                Some(Connection::spawn(1, &command, path, None, None, events_tx).unwrap());
            original.config.custom_title = Some("Keep my session name".into());
            original.log(Role::User, "Previous conversation");
            this.projects = vec![ProjectView {
                path: path.to_owned(),
                ssh_host: None,
                branch: "main".into(),
                sync_counts: None,
                agents: vec![AgentView {
                    controller: original,
                    elicitations: Vec::new(),
                }],
            }];
            this.next_agent_id = 200;
            this.sidebar_order = vec![1];
            this.deferred_connections.clear();
            this.reset_context(0, 0, cx);
            assert_eq!(this.projects[0].agents.len(), 2);
            assert_eq!(this.projects[0].agents[0].config.id, 1);
            assert_eq!(this.sidebar_order, [1]);
            assert_eq!(
                this.projects[0].agents[0].config.session_title(),
                Some("Keep my session name")
            );
            assert!(this.projects[0].agents[0].connection.is_some());
            assert_eq!(this.projects[0].agents[1].config.id, 200);
            let archived = &this.projects[0].agents[1];
            assert!(archived.config.archived);
            assert_eq!(archived.config.session_id.as_deref(), Some("session-1"));
            assert!(archived.messages.is_empty());
            assert!(
                this.projects[0].agents[0]
                    .messages
                    .iter()
                    .all(|entry| entry.role != Role::User)
            );
            this.projects[0].agents[0].controller.session_id = Some("reset-session".into());
            this.projects[0].agents[0].config.session_has_activity = true;
            this.projects.push(ProjectView {
                path: path.join("other"),
                ssh_host: None,
                branch: "main".into(),
                sync_counts: None,
                agents: Vec::new(),
            });
            this.move_session_to_project(
                SessionLocation {
                    project_index: 0,
                    agent_index: 0,
                },
                1,
                window,
                cx,
            );
            assert!(this.projects[0].agents[0].config.archived);
            assert_eq!(
                this.projects[0].agents[0].config.session_id.as_deref(),
                Some("reset-session")
            );
            assert_eq!(this.projects[1].agents.len(), 1);
            assert!(!this.projects[1].agents[0].config.archived);
            this.persistence.wait().unwrap();
            let (saved, _) = config::load().unwrap();
            assert_eq!(
                saved.projects[0].agents[0].session_id.as_deref(),
                Some("reset-session")
            );
            assert_eq!(
                saved.projects[0].agents[1].session_id.as_deref(),
                Some("session-1")
            );
            assert!(
                saved
                    .projects
                    .iter()
                    .flat_map(|project| &project.agents)
                    .all(|agent| agent.messages.is_empty())
            );
            this.projects.clear();
            this.view = WorkspaceView::Empty;
            this.persistence.dirty = false;
        })
    });
}

fn verify_workspace_shortcuts(workspace: &Entity<Workspace>, cx: &mut gpui_kit::VisualTestContext) {
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let displayed = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            let this = workspace.read(cx);
            this.view.displayed_session().map(|session| {
                this.projects[session.project_index].agents[session.agent_index]
                    .config
                    .id
            })
        })
    };
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let agents = [401, 402, 403, 404].map(|id| {
                let mut agent = test_agent(ProtocolVersion::V2);
                agent.config.id = id;
                agent.config.command.clear();
                agent.config.archived = id == 404;
                AgentView {
                    controller: agent,
                    elicitations: Vec::new(),
                }
            });
            this.projects = vec![ProjectView {
                path: PathBuf::from("."),
                ssh_host: None,
                branch: String::new(),
                sync_counts: None,
                agents: agents.into(),
            }];
            this.sidebar_order = vec![403, 401, 404, 402];
            this.sidebar_hidden = false;
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 0,
                }),
                window,
                cx,
            );
            this.conversation
                .composer
                .update(cx, |input, cx| input.focus(window, cx));
        });
    });

    // Session cycling follows sidebar order, skips archived sessions, and wraps.
    cx.dispatch_action(NextSession);
    assert_eq!(displayed(cx), Some(402));
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(
        displayed(cx),
        Some(403),
        "Ctrl+Tab should wrap to the first session"
    );
    cx.dispatch_action(PreviousSession);
    assert_eq!(displayed(cx), Some(402));

    // Numbered shortcuts pick sidebar positions and ignore positions past the end.
    cx.simulate_keystrokes(&format!("{primary}-1"));
    assert_eq!(displayed(cx), Some(403));
    cx.simulate_keystrokes(&format!("{primary}-2"));
    assert_eq!(displayed(cx), Some(401));
    cx.simulate_keystrokes(&format!("{primary}-9"));
    assert_eq!(displayed(cx), Some(401));
    cx.simulate_keystrokes(&format!("{primary}-3"));
    assert_eq!(displayed(cx), Some(402));
    cx.simulate_keystrokes(&format!("{primary}-5"));
    assert_eq!(
        displayed(cx),
        Some(402),
        "missing positions should be ignored"
    );

    // The sidebar shortcut works from the focused composer and persists.
    cx.simulate_keystrokes(&format!("{primary}-b"));
    assert!(cx.update(|_, cx| workspace.read(cx).sidebar_hidden));
    cx.update(|_, cx| workspace.update(cx, |this, _| this.persistence.wait().unwrap()));
    assert!(config::load().unwrap().0.sidebar_hidden);
    cx.dispatch_action(SearchSessions);
    cx.update(|window, cx| {
        let this = workspace.read(cx);
        assert!(!this.sidebar_hidden, "searching should reveal the sidebar");
        assert!(
            this.sidebar_search
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });

    // New Session repeats the displayed session's agent in the same project.
    cx.simulate_keystrokes(&format!("{primary}-n"));
    cx.update(|_, cx| {
        let this = workspace.read(cx);
        assert_eq!(this.projects[0].agents.len(), 5);
        assert_eq!(
            this.view.displayed_session(),
            Some(SessionLocation {
                project_index: 0,
                agent_index: 4,
            })
        );
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.set_view(WorkspaceView::Empty, window, cx)
        })
    });
    cx.simulate_keystrokes(&format!("{primary}-shift-n"));
    assert!(cx.update(|_, cx| matches!(
        workspace.read(cx).view,
        WorkspaceView::NewSession {
            step: PickerStep::Folders,
            ..
        }
    )));
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.set_view(WorkspaceView::Empty, window, cx)
        })
    });
    cx.dispatch_action(NewSession);
    assert!(cx.update(|_, cx| matches!(
        workspace.read(cx).view,
        WorkspaceView::NewSession {
            step: PickerStep::Folders,
            ..
        }
    )));

    cx.dispatch_action(OpenSettings);
    assert!(cx.update(|_, cx| workspace.read(cx).settings.is_some()));
    cx.dispatch_action(NextSession);
    cx.update(|_, cx| {
        let this = workspace.read(cx);
        assert!(
            this.settings.is_none(),
            "switching sessions should leave settings"
        );
        assert!(this.view.displayed_session().is_some());
    });
}

fn verify_inline_session_renaming(
    workspace: &Entity<Workspace>,
    cx: &mut gpui_kit::VisualTestContext,
) {
    let agent_id = 301;
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let mut agent = test_agent(ProtocolVersion::V2);
            agent.config.id = agent_id;
            agent.config.command.clear();
            this.projects = vec![ProjectView {
                path: PathBuf::from("."),
                ssh_host: None,
                branch: String::new(),
                sync_counts: None,
                agents: vec![AgentView {
                    controller: agent,
                    elicitations: Vec::new(),
                }],
            }];
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 0,
                }),
                window,
                cx,
            );
        });
    });
    for (draft, save) in [("Inline name", true), ("Discard this", false)] {
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.open_rename_session(agent_id, window, cx);
                let input = this.conversation.renaming.as_ref().unwrap().input.clone();
                input.update(cx, |input, cx| input.set_value(draft, window, cx));
            });
        });
        cx.run_until_parked();
        if save {
            cx.dispatch_action(Enter {
                secondary: false,
                shift: false,
            });
        } else {
            cx.dispatch_action(Escape);
        }
        cx.run_until_parked();
        cx.update(|_, cx| {
            workspace.update(cx, |this, _| {
                assert!(
                    this.conversation.renaming.is_none(),
                    "editor remained open: save={save}"
                );
                assert_eq!(
                    this.agent_mut(agent_id).unwrap().0.config.session_title(),
                    Some("Inline name")
                );
            });
        });
    }
}

fn verify_key_bindings(workspace: &Entity<Workspace>, cx: &mut gpui_kit::VisualTestContext) {
    let record = |shortcut, cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                if this.settings.is_none() {
                    this.open_settings(window, cx);
                }
                let page = this.settings.as_mut().unwrap();
                page.section = settings::SettingsSection::KeyBindings;
                page.recording = Some(shortcut);
                page.binding_error = None;
                window.focus(&page.focus, cx);
                cx.notify();
            })
        });
    };
    record(Shortcut::ZoomIn, cx);
    let scale = cx.update(|_, cx| workspace.read(cx).font_scale);
    cx.simulate_keystrokes("secondary-shift-n");
    cx.update(|_, cx| {
        let this = workspace.read(cx);
        assert_eq!(
            this.settings.as_ref().unwrap().recording,
            Some(Shortcut::ZoomIn)
        );
        assert!(this.settings.as_ref().unwrap().binding_error.is_some());
        assert_eq!(this.font_scale, scale);
    });
    cx.simulate_keystrokes("f6");
    cx.update(|_, cx| {
        let this = workspace.read(cx);
        assert_eq!(this.settings.as_ref().unwrap().recording, None);
        assert_eq!(
            this.font_scale, scale,
            "recording must not execute the shortcut"
        );
    });
    cx.simulate_keystrokes("f6");
    assert_eq!(
        cx.update(|_, cx| workspace.read(cx).font_scale),
        scale + FONT_SCALE_STEP
    );
    cx.simulate_keystrokes("secondary-=");
    assert_eq!(
        cx.update(|_, cx| workspace.read(cx).font_scale),
        scale + FONT_SCALE_STEP,
        "the previous binding must stop executing"
    );
    record(Shortcut::Send, cx);
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(!workspace.read(cx).shortcut_customized(Shortcut::Send)));
    record(Shortcut::Send, cx);
    cx.simulate_keystrokes("f7");
    record(Shortcut::Newline, cx);
    cx.simulate_keystrokes("f8");
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.persistence.wait().unwrap();
            let (saved, _) = config::load().unwrap();
            assert_eq!(saved.key_bindings, this.key_bindings);
            assert_eq!(
                key_bindings::validated(saved.key_bindings, saved.send_key),
                this.key_bindings
            );
            this.close_settings(window, cx);
            this.projects[0].agents[0].status = Status::Connecting;
            this.projects[0].agents[0].config.pending_prompts.clear();
            this.conversation.composer.update(cx, |input, cx| {
                input.set_value("custom", window, cx);
                input.set_cursor_position(Position::new(0, 6), window, cx);
                input.focus(window, cx);
            });
        })
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        let this = workspace.read(cx);
        assert!(this.projects[0].agents[0].config.pending_prompts.is_empty());
        assert_eq!(
            this.conversation.composer.read(cx).value().as_ref(),
            "custom"
        );
    });
    cx.simulate_keystrokes("f8");
    cx.update(|_, cx| {
        assert_eq!(
            workspace
                .read(cx)
                .conversation
                .composer
                .read(cx)
                .value()
                .as_ref(),
            "custom\n"
        )
    });
    cx.simulate_keystrokes("f7");
    cx.update(|_, cx| {
        let this = workspace.read(cx);
        assert_eq!(
            this.projects[0].agents[0].config.pending_prompts,
            [Prompt::from("custom")]
        );
    });
    record(Shortcut::Confirm, cx);
    cx.simulate_keystrokes("f9");
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.close_settings(window, cx);
            this.open_picker(PickerStep::Folders, window, cx);
            this.picker.folder_search = FolderSearch::new(vec![]);
            this.notice = None;
        })
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(workspace.read(cx).notice.is_none()));
    cx.simulate_keystrokes("f9");
    cx.update(|_, cx| assert!(workspace.read(cx).notice.is_some()));
    record(Shortcut::Dismiss, cx);
    cx.simulate_keystrokes("f10");
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(workspace.read(cx).settings.is_some()));
    cx.simulate_keystrokes("f10");
    cx.update(|_, cx| assert!(workspace.read(cx).settings.is_none()));
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.back_from_picker(window, cx);
            this.reset_settings(window, cx);
            assert!(this.key_bindings.is_empty());
            assert!(this.composer_submits_on_enter());
            this.notice = None;
        })
    });
}

fn verify_settings_apply_and_persist(
    workspace: &Entity<Workspace>,
    path: &Path,
    cx: &mut gpui_kit::VisualTestContext,
) {
    let pending = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            workspace.read(cx).projects[0].agents[0]
                .config
                .pending_prompts
                .clone()
        })
    };
    let draft = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            workspace
                .read(cx)
                .conversation
                .composer
                .read(cx)
                .value()
                .to_string()
        })
    };
    let type_draft = |text: &str, cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.conversation.composer.update(cx, |input, cx| {
                    input.set_value(text, window, cx);
                    input.set_cursor_position(Position::new(0, text.len() as u32), window, cx);
                    input.focus(window, cx);
                });
            })
        });
    };
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let mut agent = test_agent(ProtocolVersion::V2);
            agent.config.command.clear();
            agent.status = Status::Connecting;
            this.projects = vec![ProjectView {
                path: path.to_owned(),
                ssh_host: None,
                branch: "main".into(),
                sync_counts: None,
                agents: vec![AgentView {
                    controller: agent,
                    elicitations: Vec::new(),
                }],
            }];
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 0,
                }),
                window,
                cx,
            );
            this.set_send_key(SendKey::CtrlEnter, cx);
            this.set_reduced_motion(true, cx);
            assert!(cx.reduce_motion());
            this.persistence.wait().unwrap();
        })
    });
    let (saved, _) = config::load().expect("changing a setting should save the config");
    assert_eq!(saved.send_key, SendKey::CtrlEnter);
    assert!(saved.reduced_motion);

    type_draft("first", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(draft(cx), "first\n", "Enter should add a line");
    cx.simulate_keystrokes("shift-enter");
    assert!(pending(cx).is_empty());
    cx.simulate_keystrokes("secondary-enter");
    assert_eq!(pending(cx), [Prompt::from("first\n")]);
    assert!(draft(cx).is_empty());

    for (send_key, keystroke) in [
        (SendKey::ShiftEnter, "shift-enter"),
        (SendKey::AltEnter, "alt-enter"),
    ] {
        cx.update(|_, cx| workspace.update(cx, |this, cx| this.set_send_key(send_key, cx)));
        type_draft("line", cx);
        cx.simulate_keystrokes("enter");
        assert_eq!(draft(cx), "line\n", "{send_key:?}: Enter should add a line");
        cx.simulate_keystrokes(keystroke);
        assert!(draft(cx).is_empty(), "{send_key:?} should send");
    }

    cx.update(|window, cx| workspace.update(cx, |this, cx| this.reset_settings(window, cx)));
    type_draft("second", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(pending(cx).len(), 4);
    assert_eq!(pending(cx)[3], Prompt::from("second"));
    cx.simulate_keystrokes("up");
    assert_eq!(draft(cx), "second");
    assert_eq!(pending(cx).len(), 3, "editing must dequeue the original");
    type_draft("edited second", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        pending(cx).len(),
        4,
        "resubmitting must not duplicate the prompt"
    );
    assert_eq!(pending(cx)[3], Prompt::from("edited second"));

    // The button's handler restores attachments and refuses stale clicks or drafts.
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let agent = &mut this.projects[0].agents[0];
            let agent_id = agent.config.id;
            let prompt = Prompt {
                text: "with attachments\n🙂".into(),
                images: vec![ChatImage {
                    mime_type: "image/png".into(),
                    sha256: "queued-image".into(),
                }],
                files: vec![ChatFile {
                    name: "notes.txt".into(),
                    uri: "file:///notes.txt".into(),
                    text: Some("notes".into()),
                }],
            };
            agent.config.pending_prompts[1] = prompt.clone();
            agent.config.prompt_history[1] = prompt.text.clone();
            this.edit_queued_prompt(agent_id, 0, &prompt, window, cx);
            assert_eq!(this.projects[0].agents[0].config.pending_prompts.len(), 4);
            this.conversation
                .composer
                .update(cx, |input, cx| input.set_value("draft", window, cx));
            this.edit_queued_prompt(agent_id, 1, &prompt, window, cx);
            assert_eq!(
                this.conversation.composer.read(cx).value().as_ref(),
                "draft"
            );
            this.conversation
                .composer
                .update(cx, |input, cx| input.set_value("", window, cx));
            this.draft_files.insert(agent_id, prompt.files.clone());
            this.edit_queued_prompt(agent_id, 1, &prompt, window, cx);
            assert_eq!(this.projects[0].agents[0].config.pending_prompts.len(), 4);
            this.draft_files.remove(&agent_id);
            this.edit_queued_prompt(agent_id, 1, &prompt, window, cx);
            assert_eq!(this.projects[0].agents[0].config.pending_prompts.len(), 3);
            assert_eq!(
                this.conversation.composer.read(cx).value().as_ref(),
                prompt.text
            );
            assert_eq!(
                this.conversation.composer.read(cx).cursor_position(),
                Position::new(1, 1)
            );
            assert_eq!(this.draft_images[&agent_id], prompt.images);
            assert_eq!(this.draft_files[&agent_id], prompt.files);
            assert!(this.conversation.prompt_recall.is_none());
            assert!(
                !this.projects[0].agents[0]
                    .config
                    .prompt_history
                    .contains(&prompt.text)
            );
            this.send_prompt(window, cx);
            assert_eq!(
                this.projects[0].agents[0].config.pending_prompts.last(),
                Some(&prompt)
            );
        });
    });
    cx.update(|_, cx| workspace.update(cx, |this, _| this.persistence.wait().unwrap()));
    let (saved, _) = config::load().unwrap();
    assert_eq!(saved.send_key, SendKey::Enter);
    assert_eq!(saved.font_scale, 1.0);
    assert!(!saved.reduced_motion);
    assert!(cx.update(|_, cx| !cx.reduce_motion()));

    let selected_text_size = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            workspace
                .read(cx)
                .settings
                .as_ref()
                .and_then(|page| page.text_size.read(cx).selected_value().copied())
        })
    };
    let selected_theme = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            workspace
                .read(cx)
                .settings
                .as_ref()
                .and_then(|page| page.theme.read(cx).selected_value().copied())
        })
    };
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.select_theme(crate::appearance::Choice::Dracula, cx);
            this.open_settings(window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(selected_theme(cx), Some(crate::appearance::Choice::Dracula));
    assert_eq!(selected_text_size(cx), Some(100));
    cx.dispatch_action(ZoomIn);
    assert_eq!(
        selected_text_size(cx),
        Some(110),
        "zoom shortcuts should move the text size dropdown"
    );
    cx.update(|_, cx| {
        workspace.update(cx, |this, cx| {
            this.conversation.toggled_thought_rows.insert((1, 0));
            this.conversation.toggled_tool_groups.insert((1, 1));
            this.set_thoughts_expanded(true, cx);
            this.set_tool_activity_expanded(false, cx);
            this.set_diff_layout(DiffPresentation::Split, cx);
            this.set_nested_sidebar(false, cx);
            assert!(this.conversation.toggled_thought_rows.is_empty());
            assert!(this.conversation.toggled_tool_groups.is_empty());
            assert_eq!(this.diff.presentation, DiffPresentation::Split);
            this.persistence.wait().unwrap();
        })
    });
    let (saved, _) = config::load().unwrap();
    assert!(saved.thoughts_expanded);
    assert!(!saved.tool_activity_expanded);
    assert_eq!(saved.diff_layout, DiffPresentation::Split);
    assert!(!saved.nested_sidebar);
    let default_font = cx.update(|_, cx| Theme::global(cx).font_family.clone());
    cx.update(|_, cx| {
        workspace.update(cx, |this, cx| {
            this.set_font(Some("Agentaps Test Sans".into()), cx)
        });
        assert_eq!(Theme::global(cx).font_family.as_ref(), "Agentaps Test Sans");
        workspace.update(cx, |this, _| this.persistence.wait().unwrap());
    });
    assert_eq!(
        config::load().unwrap().0.font.as_deref(),
        Some("Agentaps Test Sans")
    );
    cx.update(|window, cx| workspace.update(cx, |this, cx| this.reset_settings(window, cx)));
    assert_eq!(
        cx.update(|_, cx| Theme::global(cx).font_family.clone()),
        default_font
    );
    cx.update(|_, cx| {
        let this = workspace.read(cx);
        assert!(!this.conversation.thoughts_expanded);
        assert!(this.conversation.tool_activity_expanded);
        assert_eq!(this.diff.presentation, DiffPresentation::Unified);
        assert!(this.nested_sidebar);
    });
    assert_eq!(selected_text_size(cx), Some(100));
    assert_eq!(
        selected_theme(cx),
        Some(crate::appearance::Choice::Agentaps)
    );
    cx.update(|_, cx| workspace.update(cx, |this, _| this.persistence.wait().unwrap()));
    let (saved, _) = config::load().unwrap();
    assert_eq!(saved.theme, crate::appearance::Choice::Agentaps);
    assert!(saved.nested_sidebar);
    cx.simulate_keystrokes("escape");
    assert_eq!(selected_text_size(cx), None);
    cx.update(|window, cx| workspace.update(cx, |this, cx| this.open_settings(window, cx)));
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.settings.as_mut().unwrap().section = settings::SettingsSection::Agents;
            let page = this.settings.as_ref().unwrap();
            let claude = page.claude_executable.clone();
            let name = page.new_agent_name.clone();
            let command = page.new_agent_command.clone();
            claude.update(cx, |input, cx| input.focus(window, cx));
            name.update(cx, |input, cx| input.set_value("Wrapped Codex", window, cx));
            command.update(cx, |input, cx| {
                input.set_value("codex-wrapper --profile 'work laptop'", window, cx)
            });
        })
    });
    cx.run_until_parked();
    cx.simulate_input("  /opt/claude  ");
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.add_saved_agent(window, cx);
            this.set_theme_source(ThemeSource::Qt, cx);
            assert_eq!(this.claude_executable.as_deref(), Some("/opt/claude"));
            let choice = &this.picker.available_agents[0];
            assert_eq!(choice.name, "Wrapped Codex");
            assert_eq!(choice.detail, "Saved in settings");
            assert_eq!(
                choice.command,
                ["codex-wrapper", "--profile", "work laptop"]
            );
            let page = this.settings.as_ref().unwrap();
            assert!(page.new_agent_name.read(cx).value().is_empty());
            this.persistence.wait().unwrap();
        })
    });
    let (saved, _) = config::load().unwrap();
    assert_eq!(saved.claude_executable.as_deref(), Some("/opt/claude"));
    assert_eq!(saved.saved_agents.len(), 1);
    assert_eq!(saved.theme_source, ThemeSource::Qt);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.set_agent_executable(KnownAgent::OpenCode, Some("/opt/opencode".into()), cx);
            let opencode = this
                .picker
                .available_agents
                .iter()
                .find(|choice| choice.name == "OpenCode")
                .unwrap();
            assert_eq!(opencode.command, ["/opt/opencode", "acp"]);
            this.remove_saved_agent(0, cx);
            assert!(
                this.picker
                    .available_agents
                    .iter()
                    .all(|choice| choice.name != "Wrapped Codex")
            );
            this.reset_settings(window, cx);
            assert!(this.claude_executable.is_none());
            assert!(this.agent_executables.is_empty());
            assert_eq!(this.theme_source, ThemeSource::Automatic);
            let page = this.settings.as_ref().unwrap();
            assert!(page.claude_executable.read(cx).value().is_empty());
        })
    });
    cx.update(|window, cx| workspace.update(cx, |this, cx| this.close_settings(window, cx)));
}

fn verify_notifications_for_background_sessions(
    workspace: &Entity<Workspace>,
    path: &Path,
    cx: &mut gpui_kit::VisualTestContext,
) {
    let finish = |agent_id: u64, cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                let before = this.attention(agent_id);
                this.agent_mut(agent_id).unwrap().0.status = Status::Done;
                this.notify_attention(agent_id, before, window, cx);
                this.agent_mut(agent_id).unwrap().0.status = Status::Idle;
            })
        })
    };
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let agents = [401, 402].map(|id| {
                let mut agent = test_agent(ProtocolVersion::V2);
                agent.config.id = id;
                agent.config.command.clear();
                agent.config.custom_title = Some(format!("Session {id}"));
                AgentView {
                    controller: agent,
                    elicitations: Vec::new(),
                }
            });
            this.projects = vec![ProjectView {
                path: path.to_owned(),
                ssh_host: None,
                branch: "main".into(),
                sync_counts: None,
                agents: agents.into(),
            }];
            this.set_notifications(true, cx);
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 0,
                }),
                window,
                cx,
            );
        })
    });

    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let shown = cx.shown_system_notifications().len();
    finish(401, cx);
    assert_eq!(
        cx.shown_system_notifications().len(),
        shown,
        "the session on screen should not notify"
    );
    cx.deactivate_window();
    finish(401, cx);
    assert_eq!(
        cx.shown_system_notifications().len(),
        shown + 1,
        "a session in a background window should notify"
    );
    finish(402, cx);
    let notification = cx.shown_system_notifications().last().cloned().unwrap();
    assert_eq!(
        notification.title.as_ref(),
        format!(
            "{}: Session 402",
            path.file_name().unwrap().to_string_lossy()
        )
    );
    assert_eq!(notification.body.as_ref(), "Finished");

    cx.simulate_system_notification_response(gpui_kit::SystemNotificationResponse {
        tag: notification.tag,
        action_id: None,
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| workspace.read(cx).view.displayed_session()),
        Some(SessionLocation {
            project_index: 0,
            agent_index: 1,
        })
    );

    cx.update(|_, cx| workspace.update(cx, |this, cx| this.set_notifications(false, cx)));
    let shown = cx.shown_system_notifications().len();
    finish(401, cx);
    assert_eq!(cx.shown_system_notifications().len(), shown);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.open_settings(window, cx);
            let agent = &mut this.projects[0].agents[1];
            agent.active_work = true;
            agent.status = Status::Done;
            this.mark_displayed_agent_viewed();
            assert_eq!(this.projects[0].agents[1].status, Status::Done);
            this.handle_escape(window, cx);
            assert!(this.projects[0].agents[1].active_work);
            assert!(!this.projects[0].agents[1].cancel_requested);
            this.close_settings(window, cx);
            this.projects[0].agents[1].active_work = false;
        })
    });
}

fn verify_repository_cloning(
    workspace: &Entity<Workspace>,
    path: &Path,
    cx: &mut gpui_kit::VisualTestContext,
) {
    let (original_pane, clone_pane) = cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let original = this.pane_id;
            this.pane_bounds.insert(
                original,
                std::rc::Rc::new(std::cell::Cell::new(Bounds::new(
                    Default::default(),
                    size(px(1200.), px(800.)),
                ))),
            );
            this.split_pane(crate::panes::Direction::Right, window, cx);
            (original, this.pane_id)
        })
    });
    assert_ne!(original_pane, clone_pane);
    let source = path.join("clone-source");
    assert!(
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.open_clone(window, cx);
            this.start_clone(cx);
            assert!(this.picker.clone.error.is_some());
            this.picker.input.update(cx, |input, cx| {
                input.set_value(source.to_string_lossy().to_string(), window, cx);
                cx.emit(InputEvent::Change);
            });
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            assert_eq!(
                this.picker.clone.name.read(cx).value().as_ref(),
                "clone-source"
            );
            this.picker
                .clone
                .name
                .update(cx, |input, cx| input.set_value("my-project", window, cx));
            this.picker.input.update(cx, |input, cx| {
                input.set_value("https://example.com/other.git", window, cx);
                cx.emit(InputEvent::Change);
            });
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            assert_eq!(
                this.picker.clone.name.read(cx).value().as_ref(),
                "my-project",
                "editing the URL preserves the custom destination name"
            );
            this.picker.input.update(cx, |input, cx| {
                input.set_value(source.to_string_lossy().to_string(), window, cx);
                cx.emit(InputEvent::Change);
            });
            this.choose_clone_parent(window, cx);
        })
    });
    cx.run_until_parked();
    assert!(cx.did_prompt_for_paths());
    cx.update(|_, cx| {
        workspace.update(cx, |this, cx| {
            this.activate_pane(original_pane, cx);
        })
    });
    cx.simulate_path_prompt_response(|options| {
        assert!(options.directories && !options.files && !options.multiple);
        Some(vec![path.to_owned()])
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            assert_eq!(
                this.pane_id, original_pane,
                "the destination chooser must preserve focus"
            );
            assert!(this.picker.clone.parent.is_none());
            this.activate_pane(clone_pane, cx);
            assert_eq!(this.picker.clone.parent.as_deref(), Some(path));
            this.start_clone(cx);
            assert!(this.picker.clone.task.is_some());
            // Repeated Enter cannot start a second clone.
            this.start_clone(cx);
            this.activate_pane(original_pane, cx);
            window.focus(&this.workspace_focus, cx);
        })
    });
    let start = Instant::now();
    loop {
        let running = cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.tick_panes(window, cx);
                assert_eq!(this.pane_id, original_pane);
                assert_eq!(window.focused(cx), Some(this.workspace_focus.clone()));
                this.with_pane(clone_pane, cx, |this, _| this.picker.clone.task.is_some())
                    .unwrap()
            })
        });
        if !running {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.activate_pane(clone_pane, cx);
            assert_eq!(this.picker.clone.error, None);
            let WorkspaceView::NewSession {
                step: PickerStep::Agents { project_index },
                ..
            } = this.view
            else {
                panic!("successful cloning must open the agent picker");
            };
            assert_eq!(
                this.projects[project_index].path,
                path.join("my-project").canonicalize().unwrap()
            );
            assert!(this.projects[project_index].agents.is_empty());
            assert!(path.join("my-project/.git").is_dir());
            this.open_clone(window, cx);
            this.back_from_picker(window, cx);
            assert!(matches!(
                this.view,
                WorkspaceView::NewSession {
                    step: PickerStep::Folders,
                    ..
                }
            ));
            this.close_pane(window, cx);
            assert_eq!(this.pane_id, original_pane);
        })
    });
}

fn verify_split_panes(
    workspace: &Entity<Workspace>,
    path: &Path,
    cx: &mut gpui_kit::VisualTestContext,
) {
    use crate::panes::{Direction, Layout, Node};
    let first_file = path.join("first-pane.txt");
    let second_file = path.join("second-pane.txt");
    std::fs::write(&first_file, "First pane attachment").unwrap();
    std::fs::write(&second_file, "Second pane attachment").unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let mut agents = Vec::new();
            for id in [701, 702] {
                let mut agent = test_agent(ProtocolVersion::V2);
                agent.config.id = id;
                agent.status = Status::Connecting;
                agent.active_work = false;
                for index in 0..40 {
                    agent.log(Role::User, format!("Session {id}, message {index}"));
                }
                agents.push(AgentView {
                    controller: agent,
                    elicitations: Vec::new(),
                });
            }
            this.projects = vec![ProjectView {
                path: path.to_owned(),
                ssh_host: None,
                branch: "main".into(),
                sync_counts: None,
                agents,
            }];
            this.sidebar_order = vec![701, 702];
            this.deferred_connections.clear();
            this.pane_layout = Layout::default();
            this.pane_id = 1;
            this.next_pane_id = 2;
            this.inactive_panes.clear();
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 0,
                }),
                window,
                cx,
            );
            this.pane_bounds.insert(
                1,
                std::rc::Rc::new(std::cell::Cell::new(Bounds::new(
                    Default::default(),
                    size(px(1200.), px(800.)),
                ))),
            );
            this.split_pane(Direction::Right, window, cx);
            assert_eq!(this.pane_id, 3);
            assert_eq!(this.view, WorkspaceView::Empty);
            this.set_thoughts_expanded(true, cx);
            this.set_tool_activity_expanded(false, cx);
            this.with_pane(1, cx, |this, _| {
                assert!(this.conversation.thoughts_expanded);
                assert!(!this.conversation.tool_activity_expanded);
            });
            this.set_thoughts_expanded(false, cx);
            this.set_tool_activity_expanded(true, cx);
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 1,
                }),
                window,
                cx,
            );
            this.sync_chat_rows(0, 1);
            this.conversation.chat_list.scroll_to(gpui_kit::ListOffset {
                item_ix: 5,
                offset_in_item: px(0.),
            });
            this.conversation.composer.update(cx, |input, cx| {
                input.set_value("second pane prompt", window, cx);
                input.focus(window, cx);
            });
            this.with_pane(1, cx, |this, cx| {
                this.drop_paths(
                    &gpui_kit::ExternalPaths([first_file.clone()].into_iter().collect()),
                    cx,
                );
                this.sync_chat_rows(0, 0);
                this.conversation.chat_list.scroll_to(gpui_kit::ListOffset {
                    item_ix: 12,
                    offset_in_item: px(0.),
                });
                this.conversation.composer.update(cx, |input, cx| {
                    input.set_value("first pane draft", window, cx)
                });
            });
            assert_eq!(
                this.pane_id, 3,
                "rendering another pane must preserve focus"
            );
            this.choose_attachments(window, cx);
        });
    });
    cx.run_until_parked();
    // The composer focused by keyboard remains the routing source even if the
    // sidebar/pane selection changes before its input event is delivered.
    cx.update(|_, cx| {
        workspace.update(cx, |this, cx| {
            this.activate_pane(1, cx);
        });
    });
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && options.multiple && !options.directories);
        Some(vec![second_file.clone()])
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        workspace.update(cx, |this, _| {
            assert_eq!(
                this.pane_layout.focused, 1,
                "the chooser must not steal pane focus"
            );
            assert_eq!(this.draft_files[&701][0].name, "first-pane.txt");
            assert_eq!(this.draft_files[&702][0].name, "second-pane.txt");
        })
    });
    cx.dispatch_action(Enter {
        secondary: false,
        shift: false,
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            assert_eq!(this.pane_layout.focused, 3);
            assert_eq!(
                this.projects[0].agents[1].config.pending_prompts[0].text,
                "second pane prompt"
            );
            assert_eq!(
                this.projects[0].agents[1].config.pending_prompts[0].files[0].name,
                "second-pane.txt"
            );
            assert!(this.projects[0].agents[0].config.pending_prompts.is_empty());
            assert_eq!(this.conversation.chat_list.logical_scroll_top().item_ix, 5);
            this.with_pane(1, cx, |this, cx| {
                assert_eq!(
                    this.conversation.composer.read(cx).value().as_ref(),
                    "first pane draft"
                );
                assert_eq!(this.conversation.chat_list.logical_scroll_top().item_ix, 12);
            });
            // Sidebar selection focuses the existing pane instead of displaying
            // the same session in two panes.
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 0,
                }),
                window,
                cx,
            );
            assert_eq!(this.pane_id, 1);
            assert_eq!(
                this.saved_pane_layout().root.panes(),
                [(1, Some(701)), (3, Some(702))]
            );
            this.diff.visible = true;
            this.diff.counts = Some((99, 99));
            let request = this.diff.request_id;
            this.activate_pane(3, cx);
            assert!(this.diff.visible);
            assert_ne!(this.diff.request_id, request);
            assert_eq!(this.diff.counts, None);
            this.persist();
            this.persistence.wait().unwrap();
        });
    });
    let saved = config::load().unwrap().0.pane_layout.unwrap();
    assert_eq!(saved.root.panes(), [(1, Some(701)), (3, Some(702))]);
    assert_eq!(saved.focused, 3);
    // Exercise pane-bound callbacks after closing their source pane.
    let callback = cx.update(|_, cx| {
        workspace.update(cx, |this, cx| {
            this.pane_listener(cx, |this, _: &(), window, cx| this.send_prompt(window, cx))
        })
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.close_pane(window, cx);
            assert_eq!(this.pane_id, 1);
            assert!(matches!(this.pane_layout.root, Node::Pane { id: 1, .. }));
            assert!(!this.projects[0].agents[1].config.archived);
        });
        callback(&(), window, cx);
        workspace.update(cx, |this, cx| {
            assert_eq!(
                this.conversation.composer.read(cx).value().as_ref(),
                "first pane draft"
            );
            assert!(this.projects[0].agents[0].config.pending_prompts.is_empty());
            // A hidden session's draft follows it to another pane, and survives
            // closing that pane. Drafts belong to sessions, not presentation slots.
            let mut third = test_agent(ProtocolVersion::V2);
            third.config.id = 703;
            third.status = Status::Connecting;
            third.active_work = false;
            this.projects[0].agents.push(AgentView {
                controller: third,
                elicitations: Vec::new(),
            });
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 2,
                }),
                window,
                cx,
            );
            this.pane_bounds.insert(
                1,
                std::rc::Rc::new(std::cell::Cell::new(Bounds::new(
                    Default::default(),
                    size(px(800.), px(800.)),
                ))),
            );
            this.split_pane(Direction::Down, window, cx);
            let moved_pane = this.pane_id;
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 0,
                }),
                window,
                cx,
            );
            assert_eq!(this.pane_id, moved_pane);
            assert_eq!(this.draft_files[&701][0].name, "first-pane.txt");
            assert_eq!(
                this.conversation.composer.read(cx).value().as_ref(),
                "first pane draft"
            );
            this.close_pane(window, cx);
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 0,
                }),
                window,
                cx,
            );
            assert_eq!(
                this.conversation.composer.read(cx).value().as_ref(),
                "first pane draft"
            );
            let first = SessionLocation {
                project_index: 0,
                agent_index: 0,
            };
            let second = SessionLocation {
                project_index: 0,
                agent_index: 1,
            };
            // Sessions in the same checkout share the loaded diff, including
            // its rows and selected file, without needing a filesystem event.
            this.diff.visible = true;
            this.diff.loading = false;
            this.diff.files = vec![DiffFile {
                path: "changed.txt".into(),
                hunks: Vec::new(),
                note: None,
            }];
            this.diff.file_stats = vec![(1, 0)];
            this.diff.selected_file = Some("changed.txt".into());
            this.diff.rows = Arc::new(diff_list_rows(
                &this.diff.files,
                &this.diff.file_stats,
                this.diff.selected_file.as_deref(),
                this.diff.presentation,
            ));
            this.diff.list.reset(this.diff.rows.len());
            let rows = this.diff.rows.clone();
            let request = this.diff.request_id;
            this.set_view(WorkspaceView::Conversation(second), window, cx);
            assert!(this.diff.visible);
            assert!(Arc::ptr_eq(&this.diff.rows, &rows));
            assert_eq!(this.diff.request_id, request);
            assert_eq!(this.diff.selected_file.as_deref(), Some("changed.txt"));
            this.set_view(WorkspaceView::Conversation(first), window, cx);

            // A picker reserves its previous session. Selecting that session
            // from another pane must reveal it in its original pane.
            this.open_picker(PickerStep::Folders, window, cx);
            this.split_pane(Direction::Right, window, cx);
            let other_pane = this.pane_id;
            this.set_view(WorkspaceView::Conversation(second), window, cx);
            this.set_view(WorkspaceView::Conversation(first), window, cx);
            assert_eq!(this.pane_id, 1);
            assert_eq!(this.view.displayed_session(), Some(first));
            assert_eq!(
                this.saved_pane_layout().root.panes(),
                [(1, Some(701)), (other_pane, Some(702))]
            );
            assert_eq!(
                this.conversation.composer.read(cx).value().as_ref(),
                "first pane draft"
            );
            this.activate_pane(other_pane, cx);
            this.close_pane(window, cx);

            // Archived sessions disappear from their pane without affecting
            // another session or an agent process.
            this.projects[0].agents[0].config.archived = true;
            this.render_panes(window, cx);
            assert_eq!(this.view, WorkspaceView::Empty);
            this.restore_panes(Some(saved.clone()), window, cx);
            assert_eq!(this.pane_layout.focused, 3);
            assert_eq!(
                this.saved_pane_layout().root.panes(),
                [(2, None), (3, Some(702))]
            );
            assert_eq!(this.view.displayed_session().unwrap().agent_index, 1);
            this.close_pane(window, cx);
            assert_eq!(this.view, WorkspaceView::Empty);
        });
    });
}

fn verify_project_archiving(workspace: &Entity<Workspace>, cx: &mut gpui_kit::VisualTestContext) {
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let agent = |id, archived| {
                let mut controller = test_agent(ProtocolVersion::V2);
                controller.config.id = id;
                controller.config.command.clear();
                controller.config.archived = archived;
                AgentView {
                    controller,
                    elicitations: Vec::new(),
                }
            };
            this.projects = vec![
                ProjectView {
                    path: PathBuf::from("/projects/archive"),
                    ssh_host: None,
                    branch: "main".into(),
                    sync_counts: None,
                    agents: vec![agent(801, false), agent(802, false), agent(803, true)],
                },
                ProjectView {
                    path: PathBuf::from("/projects/keep"),
                    ssh_host: None,
                    branch: "main".into(),
                    sync_counts: None,
                    agents: vec![agent(804, false)],
                },
            ];
            this.sidebar_order = vec![801, 802, 803, 804];
            let first = SessionLocation {
                project_index: 0,
                agent_index: 0,
            };
            let other = SessionLocation {
                project_index: 1,
                agent_index: 0,
            };
            this.set_view(WorkspaceView::Conversation(first), window, cx);
            this.archive_project(0, window, cx);
            assert!(
                this.projects[0]
                    .agents
                    .iter()
                    .all(|agent| agent.config.archived)
            );
            assert!(!this.projects[1].agents[0].config.archived);
            assert_eq!(this.view, WorkspaceView::Conversation(other));
            assert_eq!(this.sidebar_results(""), vec![other]);
            let saved: Config =
                serde_json::from_slice(&serde_json::to_vec(&this.config()).unwrap()).unwrap();
            assert!(saved.projects[0].agents.iter().all(|agent| agent.archived));
            assert_eq!(saved.projects[0].agents.len(), 3);

            this.archive_project(0, window, cx);
            assert_eq!(this.view, WorkspaceView::Conversation(other));
            this.set_view(this.view.toggle_archive(), window, cx);
            assert_eq!(this.sidebar_results("").len(), 3);
            this.archive_project(1, window, cx);
            assert_eq!(this.view, WorkspaceView::Archive { return_to: None });
            assert_eq!(this.sidebar_results("").len(), 4);
            this.set_view(this.view.toggle_archive(), window, cx);
            assert_eq!(this.view, WorkspaceView::Empty);
            assert!(this.sidebar_results("").is_empty());
        });
    });
}

fn verify_project_session_sidebar(
    workspace: &Entity<Workspace>,
    cx: &mut gpui_kit::VisualTestContext,
) {
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let agent = |id, title: &str, archived| {
                let mut controller = test_agent(ProtocolVersion::V2);
                controller.config.id = id;
                controller.config.command.clear();
                controller.config.custom_title = Some(title.into());
                controller.config.archived = archived;
                AgentView {
                    controller,
                    elicitations: Vec::new(),
                }
            };
            this.projects = vec![
                ProjectView {
                    path: PathBuf::from("/projects/first"),
                    ssh_host: None,
                    branch: "main".into(),
                    sync_counts: None,
                    agents: vec![
                        agent(401, "Fix sidebar", false),
                        agent(402, "Sidebar tests", false),
                        agent(403, "Old sidebar", true),
                    ],
                },
                ProjectView {
                    path: PathBuf::from("/projects/second"),
                    ssh_host: Some("remote".into()),
                    branch: "topic".into(),
                    sync_counts: None,
                    agents: vec![
                        agent(404, "Sidebar", false),
                        agent(405, "Archived sidebar", true),
                    ],
                },
            ];
            this.sidebar_order = vec![404, 402, 405, 401, 403];
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 0,
                    agent_index: 0,
                }),
                window,
                cx,
            );
            let ids = |this: &Workspace, query| {
                this.sidebar_results(query)
                    .into_iter()
                    .map(|location| {
                        this.projects[location.project_index].agents[location.agent_index]
                            .config
                            .id
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(ids(this, ""), vec![402, 401, 404]);
            assert_eq!(ids(this, "first"), vec![402, 401]);
            let search = this.sidebar_results("sidebar");
            assert_eq!(search.len(), 3);
            assert_eq!(
                search
                    .iter()
                    .map(|location| location.project_index)
                    .collect::<Vec<_>>(),
                vec![0, 0, 1]
            );
            this.move_agent(401, 404, cx);
            assert_eq!(this.sidebar_order, vec![404, 402, 405, 401, 403]);
            this.move_agent(401, 402, cx);
            assert_eq!(ids(this, ""), vec![401, 402, 404]);
            let view = this.view;
            this.toggle_project(0, cx);
            this.toggle_project(1, cx);
            assert_eq!(this.view, view, "collapsing keeps the conversation open");
            this.move_project(0, 1, cx);
            assert_eq!(this.sidebar_projects(), vec![1, 0]);
            assert_eq!(ids(this, ""), vec![404, 401, 402]);
            assert_eq!(
                this.view, view,
                "reordering preserves the open conversation"
            );
            assert!(this.collapsed_projects.contains(&0));
            assert!(this.collapsed_projects.contains(&1));
            assert_eq!(this.projects[0].agents[0].config.id, 401);
            let saved: Config =
                serde_json::from_slice(&serde_json::to_vec(&this.config()).unwrap()).unwrap();
            assert_eq!(saved.projects[0].path, PathBuf::from("/projects/second"));
            assert_eq!(saved.projects[0].ssh_host.as_deref(), Some("remote"));
            assert_eq!(
                saved.projects[0]
                    .agents
                    .iter()
                    .map(|agent| agent.id)
                    .collect::<Vec<_>>(),
                vec![404, 405]
            );
            assert_eq!(saved.projects[1].path, PathBuf::from("/projects/first"));
            this.move_project(0, 0, cx);
            this.move_project(9, 0, cx);
            assert_eq!(this.sidebar_projects(), vec![1, 0]);
            this.move_project(0, 1, cx);
            assert_eq!(this.sidebar_projects(), vec![0, 1]);
            assert_eq!(ids(this, ""), vec![401, 402, 404]);
            this.set_nested_sidebar(false, cx);
            assert_eq!(ids(this, ""), vec![404, 401, 402]);
            assert_eq!(
                ids(this, "sidebar")[0],
                404,
                "flat search ranks sessions across projects"
            );
            this.move_agent(401, 404, cx);
            assert_eq!(
                ids(this, ""),
                vec![401, 404, 402],
                "flat sessions can reorder across projects"
            );
            assert_eq!(
                this.view, view,
                "switching layouts preserves the conversation"
            );
            assert!(!this.config().nested_sidebar);
            this.set_nested_sidebar(true, cx);
            assert_eq!(ids(this, ""), vec![401, 402, 404]);
            assert_eq!(
                ids(this, "sidebar").len(),
                3,
                "collapsed sessions stay searchable"
            );
            this.toggle_project(0, cx);
            assert!(!this.collapsed_projects.contains(&0));
            assert!(this.collapsed_projects.contains(&1));
            this.set_view(
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 1,
                    agent_index: 0,
                }),
                window,
                cx,
            );
            assert!(
                this.collapsed_projects.is_empty(),
                "selecting a session reveals its project"
            );
            this.set_view(WorkspaceView::Archive { return_to: None }, window, cx);
            assert_eq!(ids(this, ""), vec![403, 405]);
            assert!(ids(this, "missing").is_empty());
        });
    });
}

fn verify_project_agent_dialog(
    workspace: &Entity<Workspace>,
    cx: &mut gpui_kit::VisualTestContext,
) {
    let previous = SessionLocation {
        project_index: 0,
        agent_index: 0,
    };
    let available = cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let available = std::mem::replace(
                &mut this.picker.available_agents,
                vec![AgentChoice {
                    name: "Test agent".into(),
                    detail: "Test only".into(),
                    command: Vec::new(),
                }],
            );
            this.set_view(WorkspaceView::Conversation(previous), window, cx);
            this.conversation.composer.update(cx, |input, cx| {
                input.replace_all("Unsent draft", window, cx)
            });
            this.toggle_project(1, cx);
            this.open_picker(PickerStep::ProjectAgents { project_index: 1 }, window, cx);
            assert_eq!(this.view.displayed_session(), Some(previous));
            assert_eq!(this.view.highlighted_session(), Some(previous));
            available
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            assert_eq!(this.view, WorkspaceView::Conversation(previous));
            assert_eq!(
                this.conversation.composer.read(cx).value().as_ref(),
                "Unsent draft"
            );
            assert!(this.composer_focused(window, cx));
            assert!(
                this.collapsed_projects.contains(&1),
                "dismissing the chooser keeps its project collapsed"
            );
            this.open_picker(PickerStep::ProjectAgents { project_index: 1 }, window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            // An empty command exercises session creation without starting a harness.
            assert_eq!(
                this.view,
                WorkspaceView::Conversation(SessionLocation {
                    project_index: 1,
                    agent_index: 2
                })
            );
            assert_eq!(this.projects[0].agents.len(), 3);
            assert_eq!(this.projects[1].agents.len(), 3);
            assert_eq!(this.projects[1].agents[2].name, "Test agent");
            assert!(
                !this.collapsed_projects.contains(&1),
                "a new session expands its project"
            );
            this.set_view(WorkspaceView::Empty, window, cx);
            this.open_picker(PickerStep::ProjectAgents { project_index: 0 }, window, cx);
            this.back_from_picker(window, cx);
            assert_eq!(this.view, WorkspaceView::Empty);
            this.picker.available_agents = available;
        })
    });
}
