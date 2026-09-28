mod acp;
mod app;
mod config;
mod diff_view;
mod diff_watch;
mod discovery;
mod file_search;
mod folder_search;
mod git_diff;
mod git_sync;
mod mobile;
mod remote;
#[cfg(unix)]
mod shell_env;
mod theme;

fn main() {
    #[cfg(unix)]
    shell_env::import_login_path();
    app::run();
}
