use std::{
    env,
    ffi::{OsStr, OsString},
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

const SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// Apps opened from the Dock, Finder, or a desktop launcher inherit a minimal
/// PATH that misses user installs such as Homebrew, `~/.local/bin`, or Nix
/// profiles, so agents are not found. Ask the user's login shell for the PATH
/// it builds and put it ahead of the inherited one.
///
/// Must run before any other thread starts, because it modifies the process
/// environment.
pub fn import_login_path() {
    // Launched from a terminal: PATH already comes from the user's shell.
    if env::var_os("TERM").is_some() {
        return;
    }
    let Some(shell) = env::var_os("SHELL").filter(|shell| !shell.is_empty()) else {
        return;
    };
    let Some(login_path) = login_shell_path(&shell) else {
        return;
    };
    let inherited = env::var_os("PATH").unwrap_or_default();
    if let Some(merged) = merge_paths(&login_path, &inherited) {
        // SAFETY: called at the start of main, before any other thread exists.
        unsafe { env::set_var("PATH", merged) };
    }
}

fn login_shell_path(shell: &OsStr) -> Option<OsString> {
    // Interactive login mode reads the files where users usually extend PATH,
    // such as .zprofile, .zshrc, or Nushell's env.nu and config.nu.
    // `/usr/bin/env` prints the environment the same way for any shell.
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c", "/usr/bin/env"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout.read_to_end(&mut output);
        let _ = sender.send(output);
    });
    let output = receiver.recv_timeout(SHELL_TIMEOUT);
    if output.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    parse_path(&String::from_utf8_lossy(&output.ok()?))
}

/// Finds PATH in `env` output. Startup files may print their own lines first,
/// so the last match wins.
fn parse_path(env_output: &str) -> Option<OsString> {
    env_output
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("PATH="))
        .filter(|path| !path.is_empty())
        .map(OsString::from)
}

/// Login shell entries first, then inherited entries it lacks.
fn merge_paths(login: &OsStr, inherited: &OsStr) -> Option<OsString> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for path in env::split_paths(login).chain(env::split_paths(inherited)) {
        if !path.as_os_str().is_empty() && !paths.contains(&path) {
            paths.push(path);
        }
    }
    env::join_paths(paths).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_last_path_line_from_env_output() {
        let output =
            "welcome\nHOME=/home/user\nPATH=/early\nPATH=/opt/homebrew/bin:/usr/bin\nTERM=dumb\n";
        assert_eq!(
            parse_path(output),
            Some(OsString::from("/opt/homebrew/bin:/usr/bin"))
        );
    }

    #[test]
    fn ignores_missing_or_empty_path() {
        assert_eq!(parse_path("HOME=/home/user\nMYPATH=/nope\n"), None);
        assert_eq!(parse_path("PATH=\n"), None);
    }

    #[test]
    fn merges_login_path_first_without_duplicates() {
        let merged = merge_paths(
            OsStr::new("/opt/homebrew/bin:/usr/bin:/home/user/.local/bin"),
            OsStr::new("/usr/bin:/bin:/usr/sbin"),
        );
        assert_eq!(
            merged,
            Some(OsString::from(
                "/opt/homebrew/bin:/usr/bin:/home/user/.local/bin:/bin:/usr/sbin"
            ))
        );
    }

    #[test]
    fn reads_path_from_a_login_shell() {
        let path = login_shell_path(OsStr::new("/bin/sh")).unwrap();
        assert!(env::split_paths(&path).any(|entry| !entry.as_os_str().is_empty()));
    }
}
