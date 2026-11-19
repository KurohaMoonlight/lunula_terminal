//! ConPTY process management and lock-free wake coalescing reader thread.
//!
//! Optimization highlights:
//! - Uses `wake_pending: Arc<AtomicBool>` so high-throughput streams (`cargo build`, large `cat`)
//!   do not flood the Win32 message queue with redundant `EventLoopProxy::send_event(())` calls.
//! - Uses `exited: Arc<AtomicBool>` set immediately on EOF (`n == 0`) or pipe close so the main
//!   thread can check process liveness in `O(1)` without per-frame OS `try_wait()` syscalls.

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;
use winit::event_loop::EventLoopProxy;

pub struct PtyManager {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    pub rx: Receiver<Vec<u8>>,
    pub ipc_rx: Receiver<(Vec<u8>, std::sync::mpsc::Sender<()>)>,
    pub exited: Arc<AtomicBool>,
    pub wake_pending: Arc<AtomicBool>,
}

impl PtyManager {
    pub fn new(
        cols: u16,
        rows: u16,
        proxy: EventLoopProxy<()>,
        shell_cfg: &crate::config::ShellConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let pty_system = native_pty_system();
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system.openpty(size)?;

        // Launch configured shell with fallback
        let mut cmd = CommandBuilder::new(&shell_cfg.program);
        for arg in &shell_cfg.args {
            cmd.arg(arg);
        }

        // Ensure built-in `luna.exe` and `lfetch.exe` (next to lunula_terminal.exe) are automatically on PATH
        let mut lfetch_exe_opt = None;
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let current_path = std::env::var("PATH").unwrap_or_default();
                let new_path = format!("{};{}", exe_dir.display(), current_path);
                cmd.env("PATH", new_path);

                let target_lfetch = exe_dir.join("lfetch.exe");
                if target_lfetch.exists() {
                    lfetch_exe_opt = Some(target_lfetch);
                }
            }
        }
        cmd.env("EDITOR", "luna");
        cmd.env("LUNULA_TERMINAL", "1");
        cmd.env("TERM_PROGRAM", "lunula_terminal");
        cmd.env("TERM", "xterm-256color");

        // Auto-launch lfetch on startup if found and running PowerShell
        if let Some(lfetch_path) = lfetch_exe_opt {
            let prog_lower = shell_cfg.program.to_lowercase();
            if prog_lower.contains("powershell") || prog_lower.contains("pwsh") {
                cmd.arg("-NoExit");
                cmd.arg("-Command");
                cmd.arg(format!("& '{}'", lfetch_path.display()));
            }
        }

        // Direct local IPC bridge for binary-safe protocol passthrough (e.g. Kitty Graphics via limg)
        let ipc_listener_opt = std::net::TcpListener::bind("127.0.0.1:0").ok();
        if let Some(ref listener) = ipc_listener_opt {
            if let Ok(addr) = listener.local_addr() {
                cmd.env("LUNULA_IPC_PORT", addr.port().to_string());
            }
        }

        let child = pair.slave.spawn_command(cmd)?;

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let (tx, rx) = channel();
        let (ipc_tx, ipc_rx) = channel::<(Vec<u8>, std::sync::mpsc::Sender<()>)>();
        let exited = Arc::new(AtomicBool::new(false));
        let wake_pending = Arc::new(AtomicBool::new(false));

        let exited_clone = Arc::clone(&exited);
        let wake_clone = Arc::clone(&wake_pending);
        let tx_reader = tx.clone();
        let proxy_reader = proxy.clone();

        thread::Builder::new()
            .name("conpty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 16384];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        exited_clone.store(true, Ordering::Release);
                        let _ = proxy_reader.send_event(());
                        break;
                    }
                    if tx_reader.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                    // Coalesce wake events: only post to winit event loop if not already pending
                    if !wake_clone.swap(true, Ordering::AcqRel) {
                        let _ = proxy_reader.send_event(());
                    }
                }
                exited_clone.store(true, Ordering::Release);
                let _ = proxy_reader.send_event(());
            })?;

        if let Some(listener) = ipc_listener_opt {
            let tx_ipc = ipc_tx;
            let proxy_ipc = proxy;
            let wake_ipc = Arc::clone(&wake_pending);
            let exited_ipc = Arc::clone(&exited);

            thread::Builder::new()
                .name("lunula-ipc".into())
                .spawn(move || {
                    while let Ok((mut stream, _)) = listener.accept() {
                        if exited_ipc.load(Ordering::Acquire) {
                            break;
                        }
                        // Read the complete IPC payload before forwarding so it can never
                        // interleave mid-sequence with asynchronous ConPTY output.
                        let mut payload = Vec::new();
                        let _ = stream.read_to_end(&mut payload);
                        if !payload.is_empty() {
                            let (ack_tx, ack_rx) = channel::<()>();
                            if tx_ipc.send((payload, ack_tx)).is_err() {
                                break;
                            }
                            if !wake_ipc.swap(true, Ordering::AcqRel) {
                                let _ = proxy_ipc.send_event(());
                            }
                            // Wait for main thread to anchor the image at the current cursor row
                            let _ = ack_rx.recv_timeout(std::time::Duration::from_secs(2));
                            let _ = stream.write_all(b"K");
                            let _ = stream.flush();
                        }
                    }
                })?;
        }

        Ok(Self {
            master: pair.master,
            writer,
            child,
            rx,
            ipc_rx,
            exited,
            wake_pending,
        })
    }

    /// Clears the pending wake flag before draining the channel so new arrivals will trigger a wake.
    #[inline]
    pub fn clear_wake_flag(&self) {
        self.wake_pending.store(false, Ordering::Release);
    }

    /// Fast O(1) check via atomic EOF flag, falling back to `try_wait()` only when needed.
    #[inline]
    pub fn is_child_alive(&mut self) -> bool {
        if self.exited.load(Ordering::Acquire) {
            return false;
        }
        match self.child.try_wait() {
            Ok(None) => true,
            _ => {
                self.exited.store(true, Ordering::Release);
                false
            }
        }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }
}
