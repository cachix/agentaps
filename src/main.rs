mod acp;
mod app;
mod appearance;
mod audio;
mod claude_auth;
mod cli;
mod config;
mod diff_view;
mod diff_watch;
mod discovery;
mod file_search;
mod folder_search;
mod git_clone;
mod git_diff;
mod git_sync;
mod images;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod memory;
mod mobile;
mod panes;
mod persistence;
mod remote;
mod session;
#[cfg(unix)]
mod shell_env;
#[cfg(target_os = "linux")]
mod system_notifications;
mod theme;
mod theming;

fn main() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    memory::configure_allocator();

    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "new-session")
    {
        if std::env::args_os().nth(2).is_some() {
            eprintln!("Usage: agentaps new-session");
            std::process::exit(2);
        }
        match cli::new_session() {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }

    #[cfg(target_os = "linux")]
    match std::env::args().nth(1).as_deref() {
        Some("--gtk-theme-probe") => theming::exit_after_probe(theming::Backend::Gtk),
        Some("--qt-theme-probe") => theming::exit_after_probe(theming::Backend::Qt),
        _ => {}
    }

    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--version")
    {
        println!("agentaps {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    #[cfg(unix)]
    shell_env::import_login_path();
    app::run();
}
