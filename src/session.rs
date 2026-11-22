//! A running child process on a PTY, with its terminal emulator state.

use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Working,
    Waiting,
    Done,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Working => "working",
            Status::Waiting => "waiting",
            Status::Done => "done",
        }
    }
}

pub struct Session {
    pub name: String,
    pub cwd: PathBuf,
    pub parser: vt100::Parser,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    rx: Receiver<Vec<u8>>,
    pub exited: bool,
    size: (u16, u16),
}

fn nz(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n).unwrap_or(NonZeroU16::MIN)
}

impl Session {
    pub fn spawn(
        name: String,
        program: &str,
        args: &[String],
        cwd: PathBuf,
        rows: u16,
        cols: u16,
    ) -> Result<Self> {
        let (rows, cols) = (rows.max(1), cols.max(1));
        let pair = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");
        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("failed to start `{program}`"))?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            name,
            cwd,
            parser: vt100::Parser::new(nz(rows), nz(cols), 0),
            master: pair.master,
            writer,
            child,
            rx,
            exited: false,
            size: (rows, cols),
        })
    }

    /// Feed pending PTY output into the emulator; detect exit.
    pub fn pump(&mut self) {
        if self.exited {
            return;
        }
        loop {
            match self.rx.try_recv() {
                Ok(bytes) => self.parser.process(&bytes),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let _ = self.child.wait();
                    self.exited = true;
                    break;
                }
            }
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = (rows.max(1), cols.max(1));
        if self.size == (rows, cols) {
            return;
        }
        self.size = (rows, cols);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.parser.set_size(nz(rows), nz(cols));
    }

    pub fn send(&mut self, bytes: &[u8]) {
        if self.exited {
            return;
        }
        if self.writer.write_all(bytes).and_then(|_| self.writer.flush()).is_err() {
            self.exited = true;
        }
    }

    pub fn paste(&mut self, text: &str) {
        if self.parser.screen().bracketed_paste() {
            let s = format!("\x1b[200~{text}\x1b[201~");
            self.send(s.as_bytes());
        } else {
            self.send(text.as_bytes());
        }
    }

    pub fn application_cursor(&self) -> bool {
        self.parser.screen().application_cursor()
    }

    /// Claude Code shows "esc to interrupt" while it is working.
    pub fn status(&self) -> Status {
        if self.exited {
            return Status::Done;
        }
        let text = self.parser.screen().contents();
        if text.contains("esc to interrupt") {
            Status::Working
        } else {
            Status::Waiting
        }
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
