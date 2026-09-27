use std::path::Path;
use std::process::Command;

/// Commits unique to HEAD and its configured upstream tracking branch.
pub fn counts(path: &Path) -> Option<(usize, usize)> {
    let repo = gix::discover(path).ok()?;
    let head = repo.head_ref().ok()??;
    let tracking = repo
        .branch_remote_tracking_ref_name(head.name(), gix::remote::Direction::Fetch)?
        .ok()?;
    let upstream = repo.find_reference(tracking.as_ref()).ok()?;
    let head_id = head.id().detach();
    let upstream_id = upstream.id().detach();
    if head_id == upstream_id {
        return Some((0, 0));
    }
    let ahead = repo
        .rev_walk([head_id])
        .with_hidden([upstream_id])
        .all()
        .ok()?
        .try_fold(0usize, |count, commit| commit.map(|_| count + 1).ok())?;
    let behind = repo
        .rev_walk([upstream_id])
        .with_hidden([head_id])
        .all()
        .ok()?
        .try_fold(0usize, |count, commit| commit.map(|_| count + 1).ok())?;
    Some((ahead, behind))
}

/// Refresh the tracking refs for the current branch without prompting for credentials.
pub fn fetch(path: &Path) {
    let Ok(repo) = gix::discover(path) else {
        return;
    };
    let Ok(Some(head)) = repo.head_ref() else {
        return;
    };
    if repo
        .branch_remote_ref_name(head.name(), gix::remote::Direction::Fetch)
        .and_then(Result::ok)
        .is_none()
    {
        return;
    }
    let _ = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["fetch", "--no-tags", "--quiet"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("SSH_ASKPASS_REQUIRE", "never")
        .output();
}

pub fn remote_counts(host: &str, path: &Path, fetch: bool) -> Option<(usize, usize)> {
    let path = path.to_str()?;
    let path = crate::remote::quote_shell(path);
    let fetch = if fetch {
        format!(
            "if git -C {path} rev-parse --verify '@{{upstream}}' >/dev/null 2>&1; then GIT_TERMINAL_PROMPT=0 SSH_ASKPASS_REQUIRE=never git -C {path} fetch --no-tags --quiet >/dev/null 2>&1; fi; "
        )
    } else {
        String::new()
    };
    let command =
        format!("{fetch}git -C {path} rev-list --left-right --count 'HEAD...@{{upstream}}'");
    let output = Command::new("ssh")
        .args([
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "ConnectTimeout=5",
            host,
            &command,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let output = std::str::from_utf8(&output.stdout).ok()?;
    let mut counts = output.split_whitespace();
    let ahead = counts.next()?.parse().ok()?;
    let behind = counts.next()?.parse().ok()?;
    (counts.next().is_none()).then_some((ahead, behind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn counts_commits_against_configured_tracking_branch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(path)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        };
        git(&["init", "-qb", "main"]);
        let commit = |message| {
            git(&[
                "-c",
                "user.name=Agentaps",
                "-c",
                "user.email=agentaps@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                message,
            ]);
        };
        commit("base");
        let base = git(&["rev-parse", "HEAD"]);
        assert_eq!(counts(path), None);

        git(&[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ]);
        git(&["config", "branch.main.remote", "origin"]);
        git(&["config", "branch.main.merge", "refs/heads/main"]);
        git(&["update-ref", "refs/remotes/origin/main", &base]);
        assert_eq!(counts(path), Some((0, 0)));

        commit("local");
        let local = git(&["rev-parse", "HEAD"]);
        assert_eq!(counts(path), Some((1, 0)));

        git(&["reset", "--hard", &base]);
        commit("remote");
        let remote = git(&["rev-parse", "HEAD"]);
        git(&["update-ref", "refs/remotes/origin/main", &remote]);
        git(&["reset", "--hard", &local]);
        assert_eq!(counts(path), Some((1, 1)));

        git(&["reset", "--hard", &base]);
        assert_eq!(counts(path), Some((0, 1)));
    }

    #[test]
    fn fetch_updates_tracking_ref_before_counting() {
        let dir = tempfile::tempdir().unwrap();
        let remote = dir.path().join("remote.git");
        let local = dir.path().join("local");
        let other = dir.path().join("other");
        let git = |cwd: &Path, args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(
            dir.path(),
            &["init", "--bare", "-q", remote.to_str().unwrap()],
        );
        git(
            dir.path(),
            &["init", "-qb", "main", local.to_str().unwrap()],
        );
        git(
            &local,
            &[
                "-c",
                "user.name=Agentaps",
                "-c",
                "user.email=agentaps@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "initial",
            ],
        );
        git(
            &local,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&local, &["push", "-qu", "origin", "main"]);
        git(
            dir.path(),
            &[
                "clone",
                "-qb",
                "main",
                remote.to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
        git(
            &other,
            &[
                "-c",
                "user.name=Agentaps",
                "-c",
                "user.email=agentaps@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "new remote commit",
            ],
        );
        git(&other, &["push", "-q", "origin", "main"]);

        assert_eq!(counts(&local), Some((0, 0)));
        fetch(&local);
        assert_eq!(counts(&local), Some((0, 1)));
    }
}
