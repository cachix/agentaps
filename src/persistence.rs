//! A single background writer keeps only the newest waiting snapshot.
use crate::config::{self, Config};
use std::{
    sync::{Arc, Condvar, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Queue {
    pending: Option<(u64, Config)>,
    closed: bool,
}

pub(crate) struct Persistence {
    pub dirty: bool,
    last_attempt: Instant,
    retry_delay: Duration,
    revision: u64,
    #[cfg(test)]
    latest_result: Option<(u64, Result<(), String>)>,
    queue: Arc<(Mutex<Queue>, Condvar)>,
    results: mpsc::Receiver<(u64, Result<(), String>)>,
    worker: Option<JoinHandle<()>>,
}

impl Persistence {
    pub fn new(dirty: bool) -> Self {
        Self::at_path(dirty, config::path())
    }

    pub(crate) fn at_path(dirty: bool, path: Result<std::path::PathBuf, String>) -> Self {
        Self::with_writer(dirty, move |snapshot| match &path {
            Ok(path) => config::save_to(path, snapshot),
            Err(error) => Err(error.clone()),
        })
    }

    fn with_writer(
        dirty: bool,
        mut save: impl FnMut(&Config) -> Result<(), String> + Send + 'static,
    ) -> Self {
        let queue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let shared = queue.clone();
        let (tx, results) = mpsc::channel();
        let worker = thread::spawn(move || {
            loop {
                let snapshot = {
                    let (mutex, wake) = &*shared;
                    let mut state = mutex.lock().unwrap();
                    while state.pending.is_none() && !state.closed {
                        state = wake.wait(state).unwrap();
                    }
                    match state.pending.take() {
                        Some(snapshot) => snapshot,
                        None => break,
                    }
                };
                let (revision, config) = snapshot;
                let _ = tx.send((revision, save(&config)));
            }
        });
        Self {
            dirty,
            last_attempt: Instant::now(),
            retry_delay: Duration::from_secs(1),
            revision: 0,
            #[cfg(test)]
            latest_result: None,
            queue,
            results,
            worker: Some(worker),
        }
    }

    pub fn due(&self) -> bool {
        self.dirty && self.last_attempt.elapsed() >= self.retry_delay
    }

    pub fn submit(&mut self, config: Config) {
        self.revision += 1;
        let (mutex, wake) = &*self.queue;
        mutex.lock().unwrap().pending = Some((self.revision, config));
        self.dirty = false;
        self.last_attempt = Instant::now();
        wake.notify_one();
    }

    #[cfg(test)]
    pub fn wait(&mut self) -> Result<(), String> {
        if let Some((revision, result)) = &self.latest_result
            && *revision == self.revision
        {
            return result.clone();
        }
        loop {
            let (revision, result) = self
                .results
                .recv_timeout(Duration::from_secs(5))
                .map_err(|error| error.to_string())?;
            if revision == self.revision {
                self.latest_result = Some((revision, result.clone()));
                return result;
            }
        }
    }

    /// Old completions must not affect newer snapshots or unsaved UI edits.
    pub fn poll(&mut self) -> Option<String> {
        let mut error = None;
        while let Ok((revision, result)) = self.results.try_recv() {
            if revision != self.revision {
                continue;
            }
            #[cfg(test)]
            {
                self.latest_result = Some((revision, result.clone()));
            }
            match result {
                Ok(()) => self.retry_delay = Duration::from_secs(1),
                Err(message) => {
                    self.dirty = true;
                    self.last_attempt = Instant::now();
                    self.retry_delay = Duration::from_secs(5);
                    error = Some(message);
                }
            }
        }
        error
    }
}

impl Drop for Persistence {
    fn drop(&mut self) {
        let (mutex, wake) = &*self.queue;
        mutex.lock().unwrap().closed = true;
        wake.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(id: u64) -> Config {
        Config {
            sidebar_order: vec![id],
            ..Config::default()
        }
    }

    #[test]
    fn coalesces_waiting_snapshots_and_flushes_on_shutdown() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (saved_tx, saved_rx) = mpsc::channel();
        let mut writer = Persistence::with_writer(false, move |config| {
            let id = config.sidebar_order[0];
            if id == 1 {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }
            saved_tx.send(id).unwrap();
            Ok(())
        });
        writer.submit(snapshot(1));
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        writer.submit(snapshot(2));
        writer.submit(snapshot(3));
        release_tx.send(()).unwrap();
        drop(writer);
        assert_eq!(saved_rx.try_iter().collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn failed_save_backs_off_and_can_be_retried() {
        let mut writer = Persistence::with_writer(false, |_| Err("disk full".into()));
        writer.submit(snapshot(1));
        let result = writer.results.recv_timeout(Duration::from_secs(2)).unwrap();
        // Put the completion through poll using an isolated result channel.
        let (tx, rx) = mpsc::channel();
        writer.results = rx;
        tx.send(result).unwrap();
        assert_eq!(writer.poll().as_deref(), Some("disk full"));
        assert!(writer.dirty);
        assert!(!writer.due());
        assert_eq!(writer.retry_delay, Duration::from_secs(5));
    }

    #[test]
    fn stale_failures_do_not_retry_or_replace_newer_snapshots() {
        let mut writer = Persistence::with_writer(false, |_| Ok(()));
        writer.revision = 2;
        let (tx, rx) = mpsc::channel();
        writer.results = rx;
        tx.send((1, Err("old failure".into()))).unwrap();
        assert!(writer.poll().is_none());
        assert!(!writer.dirty);
        writer.dirty = true;
        tx.send((2, Ok(()))).unwrap();
        writer.poll();
        assert!(writer.dirty);
    }
}
