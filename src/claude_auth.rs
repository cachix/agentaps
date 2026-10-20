use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

/// Owns the browser login process so closing Agentaps also cancels sign-in.
pub(crate) struct Login {
    child: Child,
    urls: Receiver<String>,
    started: Instant,
    pub(crate) url: Option<String>,
}

fn login_url(line: &str) -> Option<String> {
    line.split_whitespace().find_map(|word| {
        let url = url::Url::parse(word).ok()?;
        (url.scheme() == "https"
            && matches!(
                url.host_str(),
                Some("claude.ai" | "console.anthropic.com" | "platform.claude.com")
            )
            && url.path().starts_with("/oauth/authorize"))
        .then(|| url.to_string())
    })
}

impl Login {
    pub(crate) fn start(executable: &Path, cwd: &Path) -> Result<Self, String> {
        let mut command = Command::new(executable);
        command
            .args(["auth", "login", "--claudeai"])
            .current_dir(cwd)
            .env_remove("ANTHROPIC_API_KEY")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("Could not start Claude sign-in: {error}"))?;
        let (tx, urls) = mpsc::channel();
        // Drain both streams, keeping only the browser URL. Never log credentials
        // or persist the OAuth URL in conversation history.
        let streams: Vec<Box<dyn std::io::Read + Send>> = vec![
            Box::new(child.stdout.take().unwrap()),
            Box::new(child.stderr.take().unwrap()),
        ];
        for stream in streams {
            let tx = tx.clone();
            thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    if let Some(url) = login_url(&line) {
                        let _ = tx.send(url);
                    }
                }
            });
        }
        Ok(Self {
            child,
            urls,
            started: Instant::now(),
            url: None,
        })
    }

    pub(crate) fn poll(&mut self) -> Option<Result<(), String>> {
        for url in self.urls.try_iter() {
            self.url = Some(url);
        }
        match self.child.try_wait() {
            Ok(Some(status)) if status.success() => Some(Ok(())),
            Ok(Some(_)) => Some(Err(
                "Claude sign-in did not complete. Try again and finish the browser sign-in.".into(),
            )),
            Err(error) => Some(Err(format!("Could not check Claude sign-in: {error}"))),
            Ok(None) if self.started.elapsed() >= Duration::from_secs(300) => Some(Err(
                "Claude sign-in timed out. Try again and finish the browser sign-in within five minutes.".into(),
            )),
            Ok(None) => None,
        }
    }
}

impl Drop for Login {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_claude_oauth_authorization_urls() {
        assert_eq!(
            login_url("Open https://claude.ai/oauth/authorize?state=example"),
            Some("https://claude.ai/oauth/authorize?state=example".into())
        );
        for line in [
            "https://example.com/oauth/authorize",
            "http://claude.ai/oauth/authorize",
            "https://claude.ai.evil.example/oauth/authorize",
            "https://claude.ai/docs",
            "access_token=secret",
        ] {
            assert_eq!(login_url(line), None);
        }
    }

    #[cfg(unix)]
    #[test]
    fn background_login_reports_success_and_failure() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("claude");
        for (exit_code, success) in [(0, true), (1, false)] {
            std::fs::write(&executable, format!("#!/bin/sh\nexit {exit_code}\n")).unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut login = Login::start(&executable, directory.path()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            let result = loop {
                if let Some(result) = login.poll() {
                    break result;
                }
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(result.is_ok(), success);
        }
    }
}
