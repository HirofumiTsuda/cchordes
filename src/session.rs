//! A running child process on a PTY, with its terminal emulator state.

use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// Lines of scrollback kept per session.
const SCROLLBACK_LINES: usize = 10_000;

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
    /// Last status seen on the live screen (not the scrolled-back view).
    working: bool,
    /// Set when the session stopped working or exited; the app takes it once.
    just_finished: bool,
    /// Finished while it was not focused and nobody has looked at it yet.
    pub attention: bool,
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
            parser: vt100::Parser::new(nz(rows), nz(cols), SCROLLBACK_LINES),
            master: pair.master,
            writer,
            child,
            rx,
            exited: false,
            working: false,
            just_finished: false,
            attention: false,
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
                    self.just_finished = true;
                    break;
                }
            }
        }
        // While scrolled back, `contents()` shows old lines; keep the last live value.
        if self.parser.screen().scrollback() == 0 {
            let now = self.parser.screen().contents().contains("esc to interrupt");
            self.just_finished |= self.working && !now;
            self.working = now;
        }
    }

    /// True once after the session stopped working or exited.
    pub fn take_just_finished(&mut self) -> bool {
        std::mem::take(&mut self.just_finished)
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
        if self
            .writer
            .write_all(bytes)
            .and_then(|_| self.writer.flush())
            .is_err()
        {
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
        if self.working {
            Status::Working
        } else {
            Status::Waiting
        }
    }

    /// Visible rows of the pane (used as the page size when scrolling).
    pub fn rows(&self) -> u16 {
        self.size.0
    }

    /// How many lines the view is scrolled back from the live screen.
    pub fn scroll_offset(&self) -> usize {
        self.parser.screen().scrollback()
    }

    /// Scroll back (`delta > 0`) or toward the live screen (`delta < 0`).
    pub fn scroll_by(&mut self, delta: isize) {
        let cur = self.scroll_offset();
        self.parser
            .screen_mut()
            .set_scrollback(cur.saturating_add_signed(delta));
    }

    pub fn scroll_to_top(&mut self) {
        self.parser.screen_mut().set_scrollback(usize::MAX);
    }

    pub fn scroll_to_live(&mut self) {
        self.parser.screen_mut().set_scrollback(0);
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn spawn_sh(script: &str, rows: u16) -> Session {
        Session::spawn(
            "t".into(),
            "sh",
            &["-c".into(), script.into()],
            std::env::temp_dir(),
            rows,
            40,
        )
        .unwrap()
    }

    fn pump_until(s: &mut Session, f: impl Fn(&Session) -> bool) {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            s.pump();
            if f(s) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out; screen:\n{}", s.parser.screen().contents());
    }

    #[test]
    fn scrolls_back_through_output_and_returns_to_live() {
        let mut s = spawn_sh("seq 1 100; sleep 30", 10);
        pump_until(&mut s, |s| s.parser.screen().contents().contains("100"));
        assert!(s.parser.screen().contents().contains("100"));
        assert!(!s.parser.screen().contents().contains("\n5\n"));

        s.scroll_to_top();
        assert!(s.scroll_offset() > 80);
        let top = s.parser.screen().contents();
        assert!(top.starts_with("1\n"), "top of scrollback was:\n{top}");

        s.scroll_to_live();
        assert_eq!(s.scroll_offset(), 0);
        assert!(s.parser.screen().contents().contains("100"));
        s.kill();
    }

    #[test]
    fn scroll_by_is_clamped() {
        let mut s = spawn_sh("seq 1 30; sleep 30", 10);
        pump_until(&mut s, |s| s.parser.screen().contents().contains("30"));
        s.scroll_by(-5);
        assert_eq!(s.scroll_offset(), 0);
        s.scroll_by(10_000);
        assert_eq!(s.scroll_offset(), s.parser.screen().scrollback_len());
        s.kill();
    }

    #[test]
    fn status_ignores_scrolled_back_view() {
        let mut s = spawn_sh("seq 1 30; printf 'esc to interrupt'; sleep 30", 10);
        pump_until(&mut s, |s| s.status() == Status::Working);
        s.scroll_to_top();
        s.pump();
        assert_eq!(s.status(), Status::Working);
        s.kill();
    }

    #[test]
    fn view_stays_put_when_new_output_arrives() {
        let mut s = spawn_sh("seq 1 30; sleep 1; seq 31 60; sleep 30", 10);
        pump_until(&mut s, |s| s.parser.screen().contents().contains("30"));
        s.scroll_by(5);
        let before = s.parser.screen().contents();
        pump_until(&mut s, |s| s.parser.screen().scrollback_len() > 40);
        assert!(s.scroll_offset() > 0);
        assert_eq!(s.parser.screen().contents(), before);
        s.kill();
    }

    #[test]
    fn resize_reaches_the_child() {
        let mut s = spawn_sh("sleep 0.3; stty size; sleep 30", 10);
        s.resize(7, 33);
        pump_until(&mut s, |s| s.parser.screen().contents().contains("7 33"));
        s.kill();
    }

    #[test]
    fn exit_is_detected() {
        let mut s = spawn_sh("echo bye", 10);
        pump_until(&mut s, |s| s.exited);
        assert_eq!(s.status(), Status::Done);
        assert!(s.parser.screen().contents().contains("bye"));
    }

    #[test]
    fn paste_and_send_reach_the_child() {
        let mut s = spawn_sh("cat", 10);
        s.paste("pasted text");
        s.send(b"\r");
        pump_until(&mut s, |s| {
            s.parser.screen().contents().contains("pasted text")
        });
        s.kill();
    }
}
