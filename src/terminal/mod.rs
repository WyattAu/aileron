use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tracing::{info, warn};
use uuid::Uuid;

pub struct TerminalPane {
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send>,
    master: Box<dyn MasterPty + Send>,
    pending_output: Arc<Mutex<Vec<u8>>>,
    shutdown: Arc<AtomicBool>,
    read_thread: Option<thread::JoinHandle<()>>,
    #[allow(dead_code)]
    cols: u16,
    #[allow(dead_code)]
    rows: u16,
    #[allow(dead_code)]
    pid: u32,
}

impl TerminalPane {
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

        let pending_output = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let pending_clone = pending_output.clone();
        let shutdown_clone = shutdown.clone();
        let read_thread = thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            loop {
                if shutdown_clone.load(Ordering::Relaxed) {
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut output = pending_clone.lock().unwrap();
                        output.extend_from_slice(&buf[..n]);
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::Interrupted {
                            continue;
                        }
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(1));
            }
        });

        Ok(Self {
            writer,
            child,
            master,
            pending_output,
            shutdown,
            read_thread: Some(read_thread),
            cols,
            rows,
            pid,
        })
    }

    pub fn write_input(&mut self, data: &str) {
        if let Err(e) = self.writer.write_all(data.as_bytes()) {
            warn!("failed to write to PTY: {e}");
        }
        if let Err(e) = self.writer.flush() {
            warn!("failed to flush PTY writer: {e}");
        }
    }

    pub fn flush_output(&self) -> Option<String> {
        let mut output = self.pending_output.lock().unwrap();
        if output.is_empty() {
            return None;
        }
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&*output);
        output.clear();
        Some(encoded)
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.cols = cols;
        self.rows = rows;
    }

    pub fn is_alive(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }
}

impl Drop for TerminalPane {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = self.child.kill();
        if let Some(handle) = self.read_thread.take() {
            let _ = handle.join();
        }
        info!("terminal pane (pid {}) dropped", self.pid);
    }
}

pub struct TerminalManager {
    panes: HashMap<Uuid, TerminalPane>,
    input_rx: HashMap<Uuid, mpsc::Receiver<String>>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            panes: HashMap::new(),
            input_rx: HashMap::new(),
        }
    }

    pub fn create_terminal(
        &mut self,
        pane_id: Uuid,
        cols: u16,
        rows: u16,
    ) -> Result<(mpsc::Sender<String>, (u16, u16))> {
        let pane = TerminalPane::new(cols, rows)?;
        let (tx, rx) = mpsc::channel::<String>();
        self.panes.insert(pane_id, pane);
        self.input_rx.insert(pane_id, rx);
        Ok((tx, (cols, rows)))
    }

    pub fn write_input(&mut self, pane_id: &Uuid, data: &str) {
        if let Some(pane) = self.panes.get_mut(pane_id) {
            pane.write_input(data);
        }
    }

    pub fn flush_output(&mut self, pane_id: &Uuid) -> Option<String> {
        self.panes.get(pane_id).and_then(|pane| pane.flush_output())
    }

    pub fn poll_input(&mut self) {
        let pane_ids: Vec<Uuid> = self.input_rx.keys().copied().collect();
        for id in pane_ids {
            if let Some(rx) = self.input_rx.get_mut(&id) {
                while let Ok(data) = rx.try_recv() {
                    if let Some(pane) = self.panes.get_mut(&id) {
                        pane.write_input(&data);
                    }
                }
            }
        }
    }

    pub fn remove(&mut self, pane_id: &Uuid) {
        self.input_rx.remove(pane_id);
        self.panes.remove(pane_id);
    }

    pub fn is_terminal(&self, pane_id: &Uuid) -> bool {
        self.panes.contains_key(pane_id)
    }

    pub fn resize(&mut self, pane_id: &Uuid, cols: u16, rows: u16) {
        if let Some(pane) = self.panes.get_mut(pane_id) {
            pane.resize(cols, rows);
        }
    }

    pub fn terminal_pane_ids(&self) -> Vec<Uuid> {
        self.panes.keys().copied().collect()
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn terminal_html() -> String {
    r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Aileron Terminal</title>
<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/xterm/5.3.0/xterm.min.css">
<script src="https://cdnjs.cloudflare.com/ajax/libs/xterm/5.3.0/xterm.min.js"></script>
<script src="https://cdnjs.cloudflare.com/ajax/libs/xterm-addon-fit/0.8.0/xterm-addon-fit.min.js"></script>
<style>
  html, body {
    margin: 0;
    padding: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: #1a1a2e;
  }
  #terminal-container {
    width: 100%;
    height: 100%;
    padding: 4px;
    box-sizing: border-box;
  }
</style>
</head>
<body>
<div id="terminal-container"></div>
<script>
(function() {
  var term = new Terminal({
    cursorBlink: true,
    cursorStyle: 'bar',
    fontFamily: 'Menlo, Monaco, Consolas, "Courier New", monospace',
    fontSize: 14,
    lineHeight: 1.2,
    theme: {
      background: '#1a1a2e',
      foreground: '#e0e0e0',
      cursor: '#4db4ff',
      cursorAccent: '#1a1a2e',
      selectionBackground: '#264f78',
      black: '#2e2e2e',
      red: '#ff6b6b',
      green: '#51cf66',
      yellow: '#ffd43b',
      blue: '#4db4ff',
      magenta: '#cc5de8',
      cyan: '#22b8cf',
      white: '#e0e0e0',
      brightBlack: '#868e96',
      brightRed: '#ff8787',
      brightGreen: '#69db7c',
      brightYellow: '#ffe066',
      brightBlue: '#74c0fc',
      brightMagenta: '#da77f2',
      brightCyan: '#3bc9db',
      brightWhite: '#ffffff',
    },
  });

  var fitAddon = new FitAddon.FitAddon();
  term.loadAddon(fitAddon);
  term.open(document.getElementById('terminal-container'));

  term.onData(function(data) {
    if (window.ipc) {
      window.ipc.postMessage(JSON.stringify({ t: 'i', d: data }));
    }
  });

  function sendResize() {
    if (window.ipc) {
      window.ipc.postMessage(JSON.stringify({
        t: 'r',
        rows: term.rows,
        cols: term.cols,
      }));
    }
  }

  window._terminalWrite = function(base64data) {
    var raw = atob(base64data);
    var bytes = new Uint8Array(raw.length);
    for (var i = 0; i < raw.length; i++) {
      bytes[i] = raw.charCodeAt(i);
    }
    term.write(bytes);
  };

  window._terminalFit = function() {
    fitAddon.fit();
    sendResize();
  };

  window.addEventListener('resize', function() {
    fitAddon.fit();
    sendResize();
  });

  setTimeout(function() {
    fitAddon.fit();
    sendResize();
  }, 100);
})();
</script>
</body>
</html>"#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_manager_create_and_remove() {
        let mut manager = TerminalManager::new();
        let id = Uuid::new_v4();
        let fake_id = Uuid::new_v4();

        assert!(!manager.is_terminal(&id));
        assert!(manager.terminal_pane_ids().is_empty());

        manager.input_rx.insert(fake_id, mpsc::channel().1);
        manager.panes.insert(fake_id, create_mock_pane());
        assert!(manager.is_terminal(&fake_id));
        assert_eq!(manager.terminal_pane_ids().len(), 1);

        manager.remove(&fake_id);
        assert!(!manager.is_terminal(&fake_id));
        assert!(manager.terminal_pane_ids().is_empty());
    }

    #[test]
    fn test_terminal_manager_is_terminal() {
        let mut manager = TerminalManager::new();
        let id = Uuid::new_v4();
        assert!(!manager.is_terminal(&id));

        manager.input_rx.insert(id, mpsc::channel().1);
        manager.panes.insert(id, create_mock_pane());
        assert!(manager.is_terminal(&id));
    }

    #[test]
    fn test_terminal_manager_unknown_pane() {
        let mut manager = TerminalManager::new();
        let unknown_id = Uuid::new_v4();

        assert_eq!(manager.flush_output(&unknown_id), None);
        manager.write_input(&unknown_id, "test");
        manager.resize(&unknown_id, 80, 24);
        manager.remove(&unknown_id);
        manager.poll_input();
    }

    #[test]
    fn test_terminal_html_contains_xterm() {
        let html = terminal_html();
        assert!(html.contains("xterm/5.3.0/xterm.min.css"));
        assert!(html.contains("xterm/5.3.0/xterm.min.js"));
        assert!(html.contains("xterm-addon-fit/0.8.0/xterm-addon-fit.min.js"));
    }

    #[test]
    fn test_terminal_html_has_ipc_bridge() {
        let html = terminal_html();
        assert!(html.contains("_terminalWrite"));
        assert!(html.contains("ipc.postMessage"));
        assert!(html.contains("_terminalFit"));
    }

    #[test]
    fn test_terminal_html_has_theme() {
        let html = terminal_html();
        assert!(html.contains("#1a1a2e"));
        assert!(html.contains("#e0e0e0"));
        assert!(html.contains("#4db4ff"));
    }

    fn create_mock_pane() -> TerminalPane {
        let pending_output = Arc::new(Mutex::new(Vec::new()));
        TerminalPane {
            writer: Box::new(std::io::sink()),
            child: Box::new(MockChild),
            master: Box::new(MockMaster),
            pending_output,
            shutdown: Arc::new(AtomicBool::new(false)),
            read_thread: None, // No read thread for mock
            cols: 80,
            rows: 24,
            pid: 0,
        }
    }

    struct MockChild;

    impl std::fmt::Debug for MockChild {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "MockChild")
        }
    }

    impl portable_pty::ChildKiller for MockChild {
        fn kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn clone_killer(&self) -> Box<dyn portable_pty::ChildKiller + Send + Sync> {
            Box::new(MockKiller)
        }
    }

    struct MockKiller;

    impl std::fmt::Debug for MockKiller {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "MockKiller")
        }
    }

    impl portable_pty::ChildKiller for MockKiller {
        fn kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn clone_killer(&self) -> Box<dyn portable_pty::ChildKiller + Send + Sync> {
            Box::new(MockKiller)
        }
    }

    impl portable_pty::Child for MockChild {
        fn try_wait(&mut self) -> std::io::Result<Option<portable_pty::ExitStatus>> {
            Ok(None)
        }
        fn wait(&mut self) -> std::io::Result<portable_pty::ExitStatus> {
            Ok(portable_pty::ExitStatus::with_exit_code(0))
        }
        fn process_id(&self) -> Option<u32> {
            Some(0)
        }
    }

    struct MockMaster;

    impl portable_pty::MasterPty for MockMaster {
        fn resize(&self, _size: PtySize) -> Result<(), anyhow::Error> {
            Ok(())
        }
        fn get_size(&self) -> Result<PtySize, anyhow::Error> {
            Ok(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
        }
        fn try_clone_reader(&self) -> Result<Box<dyn Read + Send>, anyhow::Error> {
            Ok(Box::new(std::io::empty()))
        }
        fn take_writer(&self) -> Result<Box<dyn Write + Send>, anyhow::Error> {
            Ok(Box::new(std::io::sink()))
        }
        fn process_group_leader(&self) -> Option<std::os::fd::RawFd> {
            None
        }
        fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
            None
        }
    }
}
