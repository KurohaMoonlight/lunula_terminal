use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc::{channel, Receiver};
use std::thread;

pub struct PtyManager {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    pub rx: Receiver<Vec<u8>>,
}

impl PtyManager {
    pub fn new(cols: u16, rows: u16) -> Result<Self, Box<dyn std::error::Error>> {
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

        pair.slave.spawn_command(cmd)?;

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let (tx, rx) = channel();

        // Background reader thread to read stdout from ConPTY
        thread::Builder::new()
            .name("conpty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            })?;

        Ok(Self {
            master: pair.master,
            writer,
            rx,
        })
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    pub fn write_win32_key(&mut self, vk: u16, sc: u16, uc: u16, ctrl_state: u32) {
        let _ = write!(
            self.writer,
            "\x1b[{vk};{sc};{uc};1;{ctrl_state};1_\x1b[{vk};{sc};{uc};0;{ctrl_state};1_"
        );
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
