use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use winit::event_loop::EventLoopProxy;

pub struct PtyManager {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    pub rx: Receiver<Vec<u8>>,
}

impl PtyManager {
    pub fn new(
        cols: u16,
        rows: u16,
        proxy: EventLoopProxy<()>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let pty_system = native_pty_system();
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system.openpty(size)?;

        // Default to powershell.exe on Windows
        let mut cmd = CommandBuilder::new("powershell.exe");
        cmd.arg("-NoLogo");

        let child = pair.slave.spawn_command(cmd)?;

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let (tx, rx) = channel();

        // Background reader thread: reads stdout from ConPTY and wakes event loop instantly
        thread::Builder::new()
            .name("conpty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        // EOF - child shell process exited! Notify event loop immediately to reap pane
                        let _ = proxy.send_event(());
                        break;
                    }
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                    // Notify winit event loop immediately without polling
                    let _ = proxy.send_event(());
                }
                // Ensure event loop is notified on thread exit
                let _ = proxy.send_event(());
            })?;

        Ok(Self {
            master: pair.master,
            writer,
            child,
            rx,
        })
    }

    pub fn is_child_alive(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            _ => false,
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


