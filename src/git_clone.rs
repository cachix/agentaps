//! Background Git cloning with bounded progress output and cancellation.
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

pub(crate) enum Update {
    Progress(String),
    Finished(Result<PathBuf, String>),
}

pub(crate) struct Task {
    pub updates: Receiver<Update>,
    cancel: Arc<AtomicBool>,
}

impl Task {
    pub fn start(url: String, parent: PathBuf, name: String) -> Self {
        let (tx, updates) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancelled = cancel.clone();
        thread::spawn(move || {
            let result = clone(&url, &parent, &name, &cancelled, &tx);
            let _ = tx.send(Update::Finished(result));
        });
        Self { updates, cancel }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(crate) fn repository_name(source: &str) -> Result<String, String> {
    let source = source.trim();
    if source.is_empty()
        || source.starts_with('-')
        || source.chars().any(char::is_control)
        || (!source.contains("://") && source.contains("::"))
    {
        return Err("Enter a repository URL or Git SSH address.".into());
    }
    let path = if source.contains("://") {
        let url = url::Url::parse(source).map_err(|_| "Enter a valid repository URL.")?;
        if !matches!(url.scheme(), "https" | "http" | "ssh" | "git" | "file") {
            return Err("Use an HTTPS, SSH, Git, or file repository URL.".into());
        }
        url.path().to_owned()
    } else {
        // Git also accepts scp-style addresses and local paths.
        source.to_owned()
    };
    let name = path
        .trim_end_matches('/')
        .rsplit(['/', ':', '\\'])
        .next()
        .unwrap_or("");
    let name = name.strip_suffix(".git").unwrap_or(name);
    validate_name(name)?;
    Ok(name.into())
}

pub(crate) fn validate_name(name: &str) -> Result<(), String> {
    let mut parts = Path::new(name).components();
    if name.trim().is_empty()
        || name.contains(['/', '\\', ':'])
        || name.chars().any(char::is_control)
        || !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
        || matches!(name, "." | "..")
    {
        return Err("Choose a single folder name without path separators.".into());
    }
    Ok(())
}

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    // SAFETY: the child owns a process group with its own PID, established below.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child.id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn clone(
    url: &str,
    parent: &Path,
    name: &str,
    cancel: &AtomicBool,
    tx: &Sender<Update>,
) -> Result<PathBuf, String> {
    repository_name(url)?;
    clone_into(parent, name, cancel, |destination| {
        run_clone(url, destination, cancel, tx)
    })
}

fn clone_into(
    parent: &Path,
    name: &str,
    cancel: &AtomicBool,
    run: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    validate_name(name)?;
    if cancel.load(Ordering::Acquire) {
        return Err("Clone cancelled.".into());
    }
    let parent = parent
        .canonicalize()
        .map_err(|error| format!("Could not open destination folder: {error}"))?;
    let destination = parent.join(name);
    // Reserve a new directory atomically. Existing folders are never modified.
    std::fs::create_dir(&destination)
        .map_err(|error| format!("Could not create {}: {error}", destination.display()))?;
    let result = run(&destination);
    if let Err(error) = result {
        return match std::fs::remove_dir_all(&destination) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(format!(
                "{error}\nCould not remove the incomplete clone at {}: {cleanup}",
                destination.display()
            )),
        };
    }
    Ok(destination)
}

fn run_clone(
    url: &str,
    destination: &Path,
    cancel: &AtomicBool,
    tx: &Sender<Update>,
) -> Result<(), String> {
    let mut command = Command::new("git");
    command
        .args(["clone", "--progress", "--"])
        .arg(url.trim())
        .arg(destination)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("SSH_ASKPASS_REQUIRE", "never");
    run_command(command, cancel, tx)
}

fn run_command(
    mut command: Command,
    cancel: &AtomicBool,
    tx: &Sender<Update>,
) -> Result<(), String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not run Git: {error}"))?;
    let mut stderr = child.stderr.take().expect("Git stderr is piped");
    let (output_tx, output_rx) = mpsc::sync_channel(32);
    let reader = thread::spawn(move || {
        let mut buffer = [0; 1024];
        while let Ok(count) = stderr.read(&mut buffer) {
            if count == 0 {
                break;
            }
            if output_tx
                .send(String::from_utf8_lossy(&buffer[..count]).into_owned())
                .is_err()
            {
                break;
            }
        }
    });
    let mut output = String::new();
    let mut last_progress = String::new();
    let status = loop {
        for chunk in output_rx.try_iter() {
            output.push_str(&chunk);
            if output.len() > 8192 {
                let mut offset = output.len() - 8192;
                while !output.is_char_boundary(offset) {
                    offset += 1;
                }
                output.drain(..offset);
            }
        }
        if let Some(line) = output
            .split(['\r', '\n'])
            .rev()
            .find(|line| !line.trim().is_empty())
        {
            // Only one update per polling interval, so output cannot flood the UI.
            if line.trim() != last_progress {
                last_progress = line.trim().to_owned();
                let _ = tx.send(Update::Progress(last_progress.clone()));
            }
        }
        if cancel.load(Ordering::Acquire) {
            terminate(&mut child);
            return Err("Clone cancelled.".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                terminate(&mut child);
                return Err(format!("Could not wait for Git: {error}"));
            }
        }
    };
    // Drain concurrently so a full bounded channel cannot block the reader.
    for chunk in output_rx {
        output.push_str(&chunk);
    }
    let _ = reader.join();
    if status.success() {
        Ok(())
    } else {
        Err(if output.trim().is_empty() {
            format!("Git clone exited with {status}.")
        } else {
            output.trim().to_owned()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn names_support_urls_ssh_addresses_and_local_sources() {
        for source in [
            "https://github.com/user/project.git",
            "ssh://git@example.com/user/project.git",
            "git@example.com:user/project.git",
            "/tmp/project.git",
        ] {
            assert_eq!(repository_name(source).unwrap(), "project");
        }
        for source in [
            "",
            "--upload-pack=bad",
            "https://example.com/..",
            "https://example.com/",
            "ext::bad",
        ] {
            assert!(repository_name(source).is_err(), "{source}");
        }
        for name in ["", ".", "..", "../outside", "a/b", "a\\b", "/tmp", "C:foo"] {
            assert!(validate_name(name).is_err(), "{name}");
        }
    }

    #[test]
    fn clones_a_repository_and_preserves_existing_destinations() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .arg(&source)
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(source.join("hello.txt"), "hello").unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&source)
                .args(["add", "."])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&source)
                .args([
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-qm",
                    "Initial"
                ])
                .status()
                .unwrap()
                .success()
        );
        let url = source.to_string_lossy().into_owned();
        let task = Task::start(url.clone(), temp.path().into(), "copy".into());
        let result = loop {
            if let Update::Finished(result) =
                task.updates.recv_timeout(Duration::from_secs(10)).unwrap()
            {
                break result;
            }
        }
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(result.join("hello.txt")).unwrap(),
            "hello"
        );
        assert!(result.join(".git").is_dir());
        let (tx, _) = mpsc::channel();
        assert!(clone(&url, temp.path(), "copy", &AtomicBool::new(false), &tx).is_err());
        assert_eq!(
            std::fs::read_to_string(result.join("hello.txt")).unwrap(),
            "hello"
        );
        assert!(
            clone(
                &temp.path().join("missing").to_string_lossy(),
                temp.path(),
                "failed",
                &AtomicBool::new(false),
                &tx
            )
            .is_err()
        );
        assert!(!temp.path().join("failed").exists());
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_stops_the_process_group_and_removes_the_partial_clone() {
        let temp = tempfile::tempdir().unwrap();
        let started = temp.path().join("started");
        let cancel = Arc::new(AtomicBool::new(false));
        let cancelled = cancel.clone();
        let marker = started.clone();
        let cancellation = thread::spawn(move || {
            let start = Instant::now();
            while !marker.exists() {
                assert!(start.elapsed() < Duration::from_secs(5));
                thread::sleep(Duration::from_millis(10));
            }
            cancelled.store(true, Ordering::Release);
        });
        let mut command = Command::new("sh");
        command
            .args(["-c", "touch \"$1\"; sleep 30", "clone-test"])
            .arg(&started);
        let (tx, _) = mpsc::channel();
        let start = Instant::now();
        assert!(
            clone_into(temp.path(), "partial", &cancel, |destination| {
                std::fs::write(destination.join("partial.txt"), "incomplete").unwrap();
                run_command(command, &cancel, &tx)
            })
            .unwrap_err()
            .contains("cancelled")
        );
        cancellation.join().unwrap();
        assert!(started.exists());
        assert!(!temp.path().join("partial").exists());
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(
            clone("/tmp/source", temp.path(), "cancelled", &cancel, &tx)
                .unwrap_err()
                .contains("cancelled")
        );
        assert!(!temp.path().join("cancelled").exists());
    }
}
