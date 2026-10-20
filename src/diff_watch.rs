use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::mpsc::Sender,
};

pub struct Change {
    pub generation: u64,
    pub rescan: bool,
}

/// The diff contains tracked files only. Watch their parent directories rather
/// than recursively traversing ignored build outputs, dependencies, or symlinks.
#[derive(Default)]
struct WatchPlan {
    files: BTreeSet<PathBuf>,
    directories: BTreeSet<PathBuf>,
    git_directories: BTreeSet<PathBuf>,
}

impl WatchPlan {
    fn new(path: &Path) -> notify::Result<Self> {
        let path = path.canonicalize()?;
        let repo = gix::discover(&path).map_err(git_error)?;
        let workdir = repo
            .workdir()
            .ok_or_else(|| notify::Error::generic("Git repository has no working directory"))?
            .canonicalize()?;
        let index = repo.index_or_load_from_head_or_empty().map_err(git_error)?;
        let mut plan = Self::default();
        plan.directories.insert(path.clone());
        for entry in index.entries() {
            let file = workdir.join(gix::path::from_bstr(entry.path(&index)).as_ref());
            if !file.starts_with(&path) {
                continue;
            }
            for parent in file.ancestors().skip(1) {
                if !parent.starts_with(&path) {
                    break;
                }
                plan.directories.insert(parent.to_path_buf());
            }
            plan.files.insert(file);
        }
        for root in [repo.git_dir(), repo.common_dir()] {
            let root = root.canonicalize()?;
            plan.git_directories.insert(root.clone());
            let mut pending = vec![root.join("refs")];
            while let Some(directory) = pending.pop() {
                if !fs::symlink_metadata(&directory).is_ok_and(|meta| meta.is_dir()) {
                    continue;
                }
                if !plan.git_directories.insert(directory.clone()) {
                    continue;
                }
                for entry in fs::read_dir(directory)? {
                    let entry = entry?;
                    if entry.file_type()?.is_dir() {
                        pending.push(entry.path());
                    }
                }
            }
        }
        // A linked worktree's .git file can itself be replaced.
        plan.files.insert(path.join(".git"));
        Ok(plan)
    }

    fn change(&self, event: &Event) -> Option<bool> {
        if matches!(event.kind, EventKind::Access(_)) {
            return None;
        }
        let mut changed = false;
        for path in &event.paths {
            if self.directories.contains(path)
                || self.git_directories.contains(path)
                || path
                    .parent()
                    .is_some_and(|parent| self.git_directories.contains(parent))
                || path.file_name().is_some_and(|name| name == ".git")
            {
                return Some(true);
            }
            changed |= self.files.contains(path);
        }
        changed.then_some(false)
    }
}

fn git_error(error: impl std::fmt::Display) -> notify::Error {
    notify::Error::generic(&error.to_string())
}

pub fn start(
    path: &Path,
    generation: u64,
    tx: Sender<Change>,
) -> notify::Result<RecommendedWatcher> {
    let root = path.canonicalize()?;
    let plan = WatchPlan::new(&root)?;
    let mut directories = plan.git_directories.clone();
    for directory in &plan.directories {
        // Checking every ancestor also prevents descending into a tracked
        // directory that has been replaced by a symlink in the working tree.
        if directory
            .ancestors()
            .take_while(|ancestor| ancestor.starts_with(&root))
            .all(|ancestor| fs::symlink_metadata(ancestor).is_ok_and(|meta| meta.is_dir()))
        {
            directories.insert(directory.clone());
        }
    }
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let rescan = match event {
            Ok(event) => plan.change(&event),
            Err(_) => Some(true),
        };
        if let Some(rescan) = rescan {
            let _ = tx.send(Change { generation, rescan });
        }
    })?;
    for directory in directories {
        watcher.watch(&directory, RecursiveMode::NonRecursive)?;
    }
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{process::Command, sync::mpsc, time::Duration};

    fn git(path: &Path, args: &[&str]) {
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
    }

    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        dir
    }

    #[test]
    fn reports_tracked_checkout_write() {
        let dir = repository();
        let path = dir.path();
        fs::create_dir(path.join("src")).unwrap();
        fs::write(path.join("src/tracked.txt"), "before").unwrap();
        git(path, &["add", "."]);
        let (tx, rx) = mpsc::channel();
        let _watcher = start(path, 7, tx).unwrap();
        fs::write(path.join("src/tracked.txt"), "changed").unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap().generation,
            7
        );
    }

    #[test]
    fn excludes_ignored_trees_but_keeps_tracked_files() {
        let dir = repository();
        let path = dir.path().canonicalize().unwrap();
        fs::write(path.join(".gitignore"), ".devenv/\nignored/\n").unwrap();
        fs::create_dir_all(path.join(".devenv/profile/lib/node_modules")).unwrap();
        fs::create_dir_all(path.join("ignored/nested")).unwrap();
        fs::write(path.join("ignored/nested/tracked.txt"), "tracked").unwrap();
        git(&path, &["add", "-f", "ignored/nested/tracked.txt"]);
        let plan = WatchPlan::new(&path).unwrap();
        assert!(
            !plan
                .directories
                .iter()
                .any(|dir| dir.starts_with(path.join(".devenv")))
        );
        assert!(plan.directories.contains(&path.join("ignored/nested")));
        let modify = |file: &str| {
            Event::new(EventKind::Modify(notify::event::ModifyKind::Any)).add_path(path.join(file))
        };
        assert_eq!(
            plan.change(&modify("ignored/nested/tracked.txt")),
            Some(false)
        );
        assert_eq!(plan.change(&modify("ignored/nested/untracked.txt")), None);
        assert_eq!(plan.change(&modify(".devenv/profile/ignored.txt")), None);
        let (tx, rx) = mpsc::channel();
        let _watcher = start(&path, 7, tx).unwrap();
        fs::write(
            path.join(".devenv/profile/lib/node_modules/output.txt"),
            "ignored",
        )
        .unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        fs::write(path.join("ignored/nested/tracked.txt"), "changed").unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap().generation,
            7
        );
    }

    #[test]
    fn index_changes_request_a_new_watch_plan() {
        let dir = repository();
        let path = dir.path();
        let (tx, rx) = mpsc::channel();
        let watcher = start(path, 3, tx).unwrap();
        fs::create_dir(path.join("new")).unwrap();
        fs::write(path.join("new/tracked.txt"), "new").unwrap();
        git(path, &["add", "."]);
        let change = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(change.generation, 3);
        assert!(change.rescan);
        drop(watcher);
        let (tx, rx) = mpsc::channel();
        let _watcher = start(path, 4, tx).unwrap();
        fs::write(path.join("new/tracked.txt"), "changed").unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap().generation,
            4
        );
    }

    #[test]
    fn recreating_a_missing_tracked_directory_requests_a_new_watch_plan() {
        let dir = repository();
        let path = dir.path();
        fs::create_dir_all(path.join("src/nested")).unwrap();
        fs::write(path.join("src/nested/tracked.txt"), "before").unwrap();
        git(path, &["add", "."]);
        fs::remove_dir_all(path.join("src")).unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher = start(path, 7, tx).unwrap();
        fs::create_dir(path.join("src")).unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().rescan);
    }

    #[test]
    fn watches_linked_worktree_index_and_shared_refs() {
        let dir = repository();
        let path = dir.path();
        fs::write(path.join("tracked.txt"), "before").unwrap();
        git(path, &["add", "."]);
        git(
            path,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-qm",
                "Initial",
            ],
        );
        let worktree = tempfile::tempdir().unwrap();
        git(
            path,
            &[
                "worktree",
                "add",
                "-qb",
                "linked",
                worktree.path().to_str().unwrap(),
            ],
        );
        let plan = WatchPlan::new(worktree.path()).unwrap();
        assert!(
            plan.git_directories
                .contains(&path.join(".git/refs/heads").canonicalize().unwrap())
        );
        fs::write(worktree.path().join("tracked.txt"), "changed").unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher = start(worktree.path(), 9, tx).unwrap();
        git(worktree.path(), &["add", "."]);
        let change = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(change.generation, 9);
        assert!(change.rescan);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_dependency_or_replaced_directory_symlinks() {
        let dir = repository();
        let outside = tempfile::tempdir().unwrap();
        let path = dir.path();
        fs::create_dir(path.join("src")).unwrap();
        fs::write(path.join("src/tracked.txt"), "before").unwrap();
        git(path, &["add", "."]);
        fs::remove_file(path.join("src/tracked.txt")).unwrap();
        fs::remove_dir(path.join("src")).unwrap();
        std::os::unix::fs::symlink(outside.path(), path.join("src")).unwrap();
        std::os::unix::fs::symlink(outside.path(), path.join("dependencies")).unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher = start(path, 7, tx).unwrap();
        fs::write(outside.path().join("tracked.txt"), "outside").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
    }
}
