use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossbeam_channel::Receiver;
use notify::RecursiveMode;
use tracing::{info, warn};

pub struct SyncWatcher {
    rx: Option<Receiver<Vec<PathBuf>>>,
    running: std::sync::Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SyncWatcher {
    pub fn new() -> Self {
        Self {
            rx: None,
            running: std::sync::Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    pub fn start(&mut self, dir: &Path) -> Result<(), anyhow::Error> {
        if self.is_running() {
            self.stop();
        }

        let (tx, rx) = crossbeam_channel::bounded::<Vec<PathBuf>>(100);
        let dir = dir.to_path_buf();
        let running = self.running.clone();
        running.store(true, Ordering::Release);

        let thread = std::thread::spawn(move || {
            let (deb_tx, deb_rx) =
                crossbeam_channel::bounded::<notify_debouncer_mini::DebounceEventResult>(100);
            let mut debouncer =
                match notify_debouncer_mini::new_debouncer(Duration::from_secs(2), deb_tx) {
                    Ok(d) => d,
                    Err(e) => {
                        warn!("Failed to create debouncer: {}", e);
                        return;
                    }
                };

            if let Err(e) = debouncer.watcher().watch(&dir, RecursiveMode::Recursive) {
                warn!("Failed to watch {}: {}", dir.display(), e);
                return;
            }

            loop {
                match deb_rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(Ok(events)) => {
                        let paths: Vec<PathBuf> = events.iter().map(|e| e.path.clone()).collect();
                        if !paths.is_empty() {
                            let _ = tx.send(paths);
                        }
                    }
                    Ok(Err(e)) => {
                        warn!("Watcher error: {}", e);
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }

                if !running.load(Ordering::Acquire) {
                    break;
                }
            }
        });

        self.rx = Some(rx);
        self.thread = Some(thread);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.rx = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        info!("Stopped filesystem watcher");
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn poll_changes(&self) -> Vec<PathBuf> {
        let mut changed = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(paths) = rx.try_recv() {
                for path in paths {
                    if !changed.contains(&path) {
                        changed.push(path);
                    }
                }
            }
        }
        changed
    }
}

impl Default for SyncWatcher {
    fn default() -> Self {
        Self::new()
    }
}
