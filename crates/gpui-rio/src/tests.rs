
use super::{Terminal, TerminalOptions, TerminalStyle, TerminalTheme, key_bytes};
use gpui_kit::{
    Context, Entity, IntoElement, Keystroke, Render, TestAppContext, Window, div, prelude::*,
};
use rio_vt::{
    config::colors::ColorRgb,
    crosswords::{Mode, pos::Column},
    event::{Msg, WindowSize},
};
use std::time::{Duration, Instant};

#[test]
fn encodes_interrupts_and_application_navigation() {
    let bytes = |key: &str, mode| key_bytes(&Keystroke::parse(key).unwrap(), mode).unwrap();
    assert_eq!(bytes("ctrl-c", Mode::empty()), b"\x03");
    assert_eq!(bytes("ctrl-d", Mode::empty()), b"\x04");
    assert_eq!(bytes("up", Mode::empty()), b"\x1b[A");
    assert_eq!(bytes("up", Mode::APP_CURSOR), b"\x1bOA");
    assert_eq!(bytes("ctrl-left", Mode::empty()), b"\x1b[1;5D");
    assert_eq!(bytes("shift-tab", Mode::empty()), b"\x1b[Z");
    assert_eq!(bytes("ctrl-delete", Mode::empty()), b"\x1b[3;5~");
    assert!(key_bytes(&Keystroke::parse("a").unwrap(), Mode::empty()).is_none());
}

struct TerminalView(Entity<Terminal>);

impl Render for TerminalView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

#[gpui_kit::test]
fn renders_and_routes_text_input_to_the_shell(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let directory = tempfile::tempdir().unwrap();
    let color = ColorRgb {
        r: 20,
        g: 20,
        b: 20,
    };
    let (view, cx) = cx.add_window_view(|window, cx| {
        let terminal = spawn(
            "/bin/sh",
            directory.path(),
            TerminalTheme::new(
                color,
                ColorRgb {
                    r: 230,
                    g: 230,
                    b: 230,
                },
                [color; 16],
            ),
            cx,
        )
        .unwrap();
        let focus = terminal.read(cx).focus.clone();
        window.focus(&focus, cx);
        TerminalView(terminal)
    });
    let terminal = cx.update(|_, cx| view.read(cx).0.clone());
    // Reconfiguration and hiding preserve the entity and its running PTY.
    cx.update(|_, cx| {
        terminal.update(cx, |terminal, _| {
            let mut style = terminal.style.clone();
            style.font_size = 11.;
            terminal.update_style(style.clone());
            terminal.set_visible(false);
            assert!(!terminal.visible);
            assert!(terminal.is_alive());
            assert!(terminal.style == style);
            terminal.set_visible(true);
        })
    });
    cx.simulate_input("PS1=RIO_PROMPT; printf 'INPUT_%s\\n' READY");
    cx.simulate_keystrokes("enter");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = cx.update(|_, cx| {
            let terminal = terminal.read(cx);
            assert!(f32::from(terminal.bounds.size.width) > 0.);
            let grid = terminal.grid.lock();
            let rows = grid.visible_rows();
            let text: Vec<String> = rows
                .iter()
                .map(|row| {
                    (0..grid.columns())
                        .map(|col| row[Column(col)].c())
                        .collect()
                })
                .collect();
            text.iter().any(|row| row.contains("INPUT_READY"))
                && text.iter().any(|row| row.starts_with("RIO_PROMPT"))
        });
        if ready {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "GPUI text input did not reach the PTY"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    cx.simulate_keystrokes("ctrl-d");
    let deadline = Instant::now() + Duration::from_secs(5);
    while cx.update(|_, cx| terminal.read(cx).is_alive()) {
        if Instant::now() >= deadline {
            let text = cx.update(|_, cx| {
                let grid = terminal.read(cx).grid.lock();
                grid.visible_rows()
                    .iter()
                    .map(|row| {
                        (0..grid.columns())
                            .map(|col| match row[Column(col)].c() {
                                '\0' => ' ',
                                c => c,
                            })
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            panic!("Ctrl+D did not close the shell: {text}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[gpui_kit::test]
fn shell_uses_project_directory_resizes_and_stops_when_dropped(cx: &mut TestAppContext) {
    let directory = tempfile::Builder::new()
        .prefix("rio project ")
        .tempdir()
        .unwrap();
    let color = ColorRgb { r: 0, g: 0, b: 0 };
    let terminal = cx.update(|cx| {
        spawn(
            "/bin/sh",
            directory.path(),
            TerminalTheme::new(color, color, [color; 16]),
            cx,
        )
        .unwrap()
    });
    let grid = cx.update(|cx| {
        let terminal = terminal.read(cx);
        terminal
            .sender
            .send(Msg::Resize(WindowSize {
                rows: 31,
                cols: 91,
                width: 0,
                height: 0,
            }))
            .unwrap();
        // Split the marker so echoed input cannot satisfy the output assertion.
        terminal.write(
            b"printf 'RIO_%s\\n' READY; printf 'PID=%s\\n' \"$$\"; pwd; stty size\n".to_vec(),
        );
        terminal.grid.clone()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        let text = {
            let grid = grid.lock();
            grid.visible_rows()
                .iter()
                .map(|row| {
                    (0..grid.columns())
                        .map(|col| match row[Column(col)].c() {
                            '\0' => ' ',
                            c => c,
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        if text.contains("RIO_READY")
            && text.contains(&directory.path().to_string_lossy().to_string())
            && text.contains("31 91")
            && let Some(pid) = text.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("PID=")
                    .and_then(|pid| pid.parse::<i32>().ok())
            })
        {
            break pid;
        }
        assert!(
            Instant::now() < deadline,
            "PTY did not produce the expected shell output: {text}"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    cx.update(|_| drop(terminal));
    let deadline = Instant::now() + Duration::from_secs(5);
    // Signal 0 only checks existence. ESRCH confirms the PTY owner reaped the shell.
    while unsafe { libc::kill(pid, 0) } == 0 {
        assert!(
            Instant::now() < deadline,
            "dropping the terminal did not stop its shell"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

fn spawn(
    shell: &str,
    directory: &std::path::Path,
    theme: TerminalTheme,
    cx: &mut gpui_kit::App,
) -> anyhow::Result<Entity<Terminal>> {
    Terminal::spawn(
        TerminalOptions::new(
            shell,
            directory,
            TerminalStyle {
                theme,
                font_family: "monospace".into(),
                font_size: 13.,
            },
        ),
        cx,
    )
}
