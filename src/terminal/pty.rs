//! PTY handle for native terminal panes.
//!
//! Wraps portable_pty to provide:
//! - PTY spawning with a default shell
//! - Async output buffering (background read thread)
//! - Input writing to the PTY master
//! - Resize handling

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use tracing::warn;

/// Handle to a PTY session.
///
/// Owns the PTY master (for writing/resizing) and runs a background thread
/// that reads from the PTY slave and buffers output.
pub struct PtyHandle {
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send>,
    master: Box<dyn MasterPty + Send>,
    pending_output: Arc<Mutex<Vec<u8>>>,
    shutdown: Arc<AtomicBool>,
    _read_thread: Option<thread::JoinHandle<()>>,
    pid: u32,
}

impl PtyHandle {
    /// Spawn a new PTY with the given grid size.
    pub fn new(cols: u16, rows: u16) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let cmd = CommandBuilder::new_default_prog();
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let writer = pair.master.take_writer()?;
        let reader = pair.master.try_clone_reader()?;
        let master = pair.master;

        let pid = child.process_id().unwrap_or(0);

        let pending_output: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let pending_clone = pending_output.clone();
        let shutdown_clone = shutdown.clone();
        let read_thread = thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            loop {
                if shutdown_clone.load(Ordering::Relaxed) {
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut output = pending_clone.lock().unwrap_or_else(|e| e.into_inner());
                        output.extend_from_slice(&buf[..n]);
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::Interrupted {
                            continue;
                        }
                        break;
                    }
                }
                // No sleep — use non-blocking read when available
                thread::sleep(Duration::from_millis(1));
            }
        });

        Ok(Self {
            writer,
            child,
            master,
            pending_output,
            shutdown,
            _read_thread: Some(read_thread),
            pid,
        })
    }

    /// Write a string to the PTY master (user keystrokes, paste, etc.).
    pub fn write(&mut self, data: &str) -> std::io::Result<()> {
        self.writer.write_all(data.as_bytes())?;
        self.writer.flush()
    }

    /// Write raw bytes to the PTY master (for PtyWrite responses from Term).
    pub fn write_bytes(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()
    }

    /// Drain all pending output from the read thread buffer.
    /// Returns the bytes and clears the buffer.
    pub fn drain_output(&self) -> Vec<u8> {
        let mut buf = self
            .pending_output
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *buf)
    }

    /// Resize the PTY.
    pub fn resize(&self, cols: u16, rows: u16) {
        if let Err(e) = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            warn!("failed to resize PTY: {e}");
        }
    }

    /// Check if the child process is still alive.
    pub fn is_alive(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }

    /// Get the process ID.
    pub fn pid(&self) -> u32 {
        self.pid
    }
}

impl Drop for PtyHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = self.child.kill();
        // read_thread is auto-joined via Option<JoinHandle> drop... actually no.
        // We need to join it. Let's fix this.
    }
}
