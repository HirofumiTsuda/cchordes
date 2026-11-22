//! cchordes — a terminal for running hordes of Claude Code sessions.
//!
//! Split panes, a session list with live status, and a unified history view.
//! All commands go through a tmux-style prefix key (Ctrl-b).

mod history;
mod router;
mod session;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use history::{Entry, Message};
use session::{Session, Status};

enum Mode {
    Normal,
    Prefix,
    /// Scrolling back through the focused session's output.
    Scroll,
    Sessions,
    /// Free-form input routed to a session. `target == sessions.len()` means a new one.
    Dispatch {
        input: String,
        target: usize,
    },
    HistoryList {
        entries: Vec<Entry>,
        query: String,
        selected: usize,
    },
    HistoryView {
        title: String,
        messages: Vec<Message>,
        scroll: u16,
        /// Session id and working directory to resume with `r`.
        resume: (String, Option<PathBuf>),
    },
}

struct App {
    program: String,
    args: Vec<String>,
    sessions: Vec<Session>,
    focus: usize,
    vertical: bool,
    next_id: usize,
    mode: Mode,
    list_state: ListState,
    quit: bool,
    error: Option<String>,
}

fn main() -> Result<()> {
    let mut argv = std::env::args().skip(1);
    let program = argv.next().unwrap_or_else(|| "claude".to_string());
    let args: Vec<String> = argv.collect();

    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableBracketedPaste);
    let result = run(&mut terminal, program, args);
    let _ = execute!(std::io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    result
}

/// Split `area` into `n` equal tiles, side by side or stacked.
fn tiles(area: Rect, n: usize, vertical: bool) -> Vec<Rect> {
    if n == 0 {
        return vec![];
    }
    let constraints = vec![Constraint::Fill(1); n];
    let layout = if vertical {
        Layout::vertical(constraints)
    } else {
        Layout::horizontal(constraints)
    };
    layout.split(area).to_vec()
}

/// Screen area minus the bottom status line.
fn main_area(size: Rect) -> Rect {
    Rect::new(size.x, size.y, size.width, size.height.saturating_sub(1))
}

impl App {
    fn new(program: String, args: Vec<String>) -> Self {
        Self {
            program,
            args,
            sessions: vec![],
            focus: 0,
            vertical: false,
            next_id: 0,
            mode: Mode::Normal,
            list_state: ListState::default(),
            quit: false,
            error: None,
        }
    }

    /// Resize and pump every session to match the current layout.
    fn layout(&mut self, size: Rect) -> Vec<Rect> {
        let rects = tiles(main_area(size), self.sessions.len(), self.vertical);
        for (s, r) in self.sessions.iter_mut().zip(&rects) {
            let inner = Block::bordered().inner(*r);
            s.resize(inner.height, inner.width);
            s.pump();
        }
        rects
    }

    fn spawn(&mut self, size: Rect, extra: Option<&str>) {
        self.spawn_with(size, extra.map(str::to_string).into_iter().collect(), None);
    }

    /// Start a session with extra arguments, optionally in another directory.
    fn spawn_with(&mut self, size: Rect, extra: Vec<String>, cwd: Option<PathBuf>) {
        // Size is corrected by the next layout pass; start with a guess.
        let n = self.sessions.len() + 1;
        let tile = tiles(main_area(size), n, self.vertical)
            .last()
            .copied()
            .unwrap_or(size);
        let inner = Block::bordered().inner(tile);
        self.next_id += 1;
        let name = format!("{}#{}", self.program, self.next_id);
        let cwd = cwd
            .filter(|d| d.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| ".".into());
        let mut args = self.args.clone();
        args.extend(extra);
        match Session::spawn(name, &self.program, &args, cwd, inner.height, inner.width) {
            Ok(s) => {
                self.sessions.push(s);
                self.focus = self.sessions.len() - 1;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("{e:#}")),
        }
    }

    fn close_focused(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let mut s = self.sessions.remove(self.focus);
        s.kill();
        if self.sessions.is_empty() {
            self.quit = true;
        } else {
            self.focus = self.focus.min(self.sessions.len() - 1);
        }
    }

    fn move_focus(&mut self, delta: isize) {
        let n = self.sessions.len() as isize;
        if n > 0 {
            self.focus = (self.focus as isize + delta).rem_euclid(n) as usize;
        }
    }

    fn handle_key(&mut self, key: KeyEvent, size: Rect) {
        match &mut self.mode {
            Mode::Normal => {
                if key.code == KeyCode::Char('b') && key.modifiers == KeyModifiers::CONTROL {
                    self.mode = Mode::Prefix;
                } else if let Some(s) = self.sessions.get_mut(self.focus)
                    && let Some(bytes) = encode_key(key, s.application_cursor())
                {
                    s.send(&bytes);
                }
            }
            Mode::Prefix => {
                self.mode = Mode::Normal;
                match key.code {
                    KeyCode::Char('c') => self.spawn(size, None),
                    KeyCode::Char('x') => self.close_focused(),
                    KeyCode::Char('h') | KeyCode::Char('p') | KeyCode::Left | KeyCode::Up => {
                        self.move_focus(-1)
                    }
                    KeyCode::Char('l') | KeyCode::Char('n') | KeyCode::Right | KeyCode::Down => {
                        self.move_focus(1)
                    }
                    KeyCode::Char('v') => self.vertical = !self.vertical,
                    KeyCode::Char('s') => {
                        self.list_state.select(Some(self.focus));
                        self.mode = Mode::Sessions;
                    }
                    KeyCode::Char('H') => {
                        self.mode = Mode::HistoryList {
                            entries: history::load_all(),
                            query: String::new(),
                            selected: 0,
                        };
                    }
                    KeyCode::Char('i') => {
                        self.mode = Mode::Dispatch {
                            input: String::new(),
                            target: self.focus,
                        };
                    }
                    KeyCode::Char('[') => {
                        if !self.sessions.is_empty() {
                            self.mode = Mode::Scroll;
                        }
                    }
                    KeyCode::Char('q') => self.quit = true,
                    KeyCode::Char('b') => {
                        if let Some(s) = self.sessions.get_mut(self.focus) {
                            s.send(&[0x02]);
                        }
                    }
                    KeyCode::Char(c @ '1'..='9') => {
                        let i = c as usize - '1' as usize;
                        if i < self.sessions.len() {
                            self.focus = i;
                        }
                    }
                    _ => {}
                }
            }
            Mode::Scroll => {
                let Some(s) = self.sessions.get_mut(self.focus) else {
                    self.mode = Mode::Normal;
                    return;
                };
                let page = (s.rows().max(2) - 1) as isize;
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        s.scroll_to_live();
                        self.mode = Mode::Normal;
                    }
                    KeyCode::Up | KeyCode::Char('k') => s.scroll_by(1),
                    KeyCode::Down | KeyCode::Char('j') => s.scroll_by(-1),
                    KeyCode::PageUp | KeyCode::Char('u') => s.scroll_by(page),
                    KeyCode::PageDown | KeyCode::Char('d') | KeyCode::Char(' ') => {
                        s.scroll_by(-page)
                    }
                    KeyCode::Home | KeyCode::Char('g') => s.scroll_to_top(),
                    KeyCode::End | KeyCode::Char('G') => s.scroll_to_live(),
                    _ => {}
                }
            }
            Mode::Sessions => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Normal,
                KeyCode::Up | KeyCode::Char('k') => self.move_focus(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_focus(1),
                KeyCode::Enter => self.mode = Mode::Normal,
                KeyCode::Char('c') => {
                    self.spawn(size, None);
                    self.mode = Mode::Normal;
                }
                _ => {}
            },
            Mode::Dispatch { input, target } => {
                let n = self.sessions.len();
                match key.code {
                    KeyCode::Esc => self.mode = Mode::Normal,
                    KeyCode::Tab => *target = (*target + 1) % (n + 1),
                    KeyCode::Backspace => {
                        input.pop();
                        *target = predict(&self.sessions, input, self.focus);
                    }
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        input.push(c);
                        *target = predict(&self.sessions, input, self.focus);
                    }
                    KeyCode::Enter => {
                        let (target, input) = (*target, input.clone());
                        let (_, body) = router::split_mention(&input);
                        let body = body.trim().to_string();
                        self.mode = Mode::Normal;
                        if body.is_empty() {
                            return;
                        }
                        if target >= n {
                            self.spawn(size, Some(&body));
                        } else {
                            let s = &mut self.sessions[target];
                            s.paste(&body);
                            s.send(b"\r");
                            self.focus = target;
                        }
                    }
                    _ => {}
                }
            }
            Mode::HistoryList {
                entries,
                query,
                selected,
            } => {
                let visible: Vec<usize> = (0..entries.len())
                    .filter(|&i| history::matches(&entries[i], query))
                    .collect();
                match key.code {
                    KeyCode::Esc => self.mode = Mode::Normal,
                    KeyCode::Up => *selected = selected.saturating_sub(1),
                    KeyCode::Down => {
                        *selected = (*selected + 1).min(visible.len().saturating_sub(1))
                    }
                    KeyCode::Backspace => {
                        query.pop();
                        *selected = 0;
                    }
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        query.push(c);
                        *selected = 0;
                    }
                    KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(&i) = visible.get(*selected) {
                            let e = &entries[i];
                            let (id, cwd) = (e.session_id.clone(), e.cwd.clone());
                            self.mode = Mode::Normal;
                            self.spawn_with(size, vec!["--resume".into(), id], cwd);
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(&i) = visible.get(*selected) {
                            let e = &entries[i];
                            let title = format!("{} — {}", e.project, e.title);
                            let messages = history::load_messages(e);
                            self.mode = Mode::HistoryView {
                                title,
                                messages,
                                scroll: 0,
                                resume: (e.session_id.clone(), e.cwd.clone()),
                            };
                        }
                    }
                    _ => {}
                }
            }
            Mode::HistoryView { scroll, resume, .. } => match key.code {
                KeyCode::Char('r') => {
                    let (id, cwd) = resume.clone();
                    self.mode = Mode::Normal;
                    self.spawn_with(size, vec!["--resume".into(), id], cwd);
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.mode = Mode::HistoryList {
                        entries: history::load_all(),
                        query: String::new(),
                        selected: 0,
                    };
                }
                KeyCode::Down | KeyCode::Char('j') => *scroll = scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                KeyCode::PageDown | KeyCode::Char(' ') => *scroll = scroll.saturating_add(20),
                KeyCode::PageUp => *scroll = scroll.saturating_sub(20),
                KeyCode::Home | KeyCode::Char('g') => *scroll = 0,
                _ => {}
            },
        }
    }
}

/// Choose the target session for `input`: an explicit @mention, else the best
/// text match, else the focused session.
fn predict(sessions: &[Session], input: &str, focus: usize) -> usize {
    let names: Vec<String> = sessions.iter().map(|s| s.name.clone()).collect();
    let (mention, body) = router::split_mention(input);
    if let Some(i) = mention.and_then(|m| router::resolve_mention(m, &names)) {
        return i;
    }
    let hay: Vec<String> = sessions
        .iter()
        .map(|s| {
            format!(
                "{} {} {}",
                s.name,
                s.cwd.display(),
                s.parser.screen().contents()
            )
        })
        .collect();
    router::best_match(body, &hay).unwrap_or(focus.min(sessions.len().saturating_sub(1)))
}

fn run(terminal: &mut DefaultTerminal, program: String, args: Vec<String>) -> Result<()> {
    let mut app = App::new(program, args);
    let size = terminal.size()?;
    app.spawn(Rect::new(0, 0, size.width, size.height), None);
    if app.sessions.is_empty() {
        anyhow::bail!(app.error.unwrap_or_else(|| "failed to start".into()));
    }

    while !app.quit {
        let size_s = terminal.size()?;
        let size = Rect::new(0, 0, size_s.width, size_s.height);
        let rects = app.layout(size);

        terminal.draw(|f| draw(f, &app, &rects))?;

        if !event::poll(Duration::from_millis(16))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => app.handle_key(key, size),
            Event::Paste(text) => {
                if let (Mode::Normal, Some(s)) = (&app.mode, app.sessions.get_mut(app.focus)) {
                    s.paste(&text);
                }
            }
            _ => {}
        }
    }
    for s in &mut app.sessions {
        s.kill();
    }
    Ok(())
}

fn status_color(s: Status) -> Color {
    match s {
        Status::Working => Color::Yellow,
        Status::Waiting => Color::Green,
        Status::Done => Color::DarkGray,
    }
}

fn draw(frame: &mut Frame, app: &App, rects: &[Rect]) {
    let area = frame.area();
    for (i, (s, r)) in app.sessions.iter().zip(rects).enumerate() {
        let focused = i == app.focus;
        let st = s.status();
        let border = if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let mut title_spans = vec![
            Span::raw(format!(" {}:{} ", i + 1, s.name)),
            Span::styled(
                format!("[{}] ", st.label()),
                Style::default().fg(status_color(st)),
            ),
        ];
        if s.scroll_offset() > 0 {
            title_spans.push(Span::styled(
                format!("[scroll -{}] ", s.scroll_offset()),
                Style::default().fg(Color::Magenta),
            ));
        }
        let title = Line::from(title_spans);
        let block = Block::bordered().border_style(border).title(title);
        let inner = block.inner(*r);
        frame.render_widget(block, *r);
        render_screen(
            frame,
            inner,
            s.parser.screen(),
            focused && matches!(app.mode, Mode::Normal),
        );
    }

    // Bottom status line.
    let bar = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(1),
        area.width,
        1,
    );
    let text = match (&app.mode, &app.error) {
        (_, Some(e)) => format!(" error: {e}"),
        (Mode::Prefix, _) => {
            " c:new x:close h/l:focus v:layout i:send s:sessions H:history [:scroll 1-9:jump q:quit"
                .into()
        }
        (Mode::Scroll, _) => " scroll: j/k:line u/d:page g:top G:live Esc:back to live".into(),
        _ => " Ctrl-b: command prefix".into(),
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        bar,
    );

    match &app.mode {
        Mode::Sessions => draw_sessions(frame, app, area),
        Mode::Dispatch { input, target } => draw_dispatch(frame, app, area, input, *target),
        Mode::HistoryList {
            entries,
            query,
            selected,
        } => draw_history_list(frame, area, entries, query, *selected),
        Mode::HistoryView {
            title,
            messages,
            scroll,
            ..
        } => draw_history_view(frame, area, title, messages, *scroll),
        _ => {}
    }
}

fn draw_dispatch(frame: &mut Frame, app: &App, area: Rect, input: &str, target: usize) {
    let h = 5.min(area.height);
    let r = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(h + 1),
        area.width,
        h,
    );
    frame.render_widget(Clear, r);
    let dest = match app.sessions.get(target) {
        Some(s) => format!("{}:{}", target + 1, s.name),
        None => format!("new {} session", app.program),
    };
    let lines = vec![
        Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{input}_")),
        ]),
        Line::from(vec![
            Span::raw("  \u{2192} "),
            Span::styled(
                dest,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    let block =
        Block::bordered().title(" Send to \u{2014} @name/@N:pick Tab:next Enter:send Esc:cancel ");
    frame.render_widget(Paragraph::new(lines).block(block), r);
}

fn overlay(area: Rect) -> Rect {
    let w = area.width.saturating_sub(8).max(20).min(area.width);
    let h = area.height.saturating_sub(4).max(5).min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn draw_sessions(frame: &mut Frame, app: &App, area: Rect) {
    let r = overlay(area);
    frame.render_widget(Clear, r);
    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let st = s.status();
            ListItem::new(Line::from(vec![
                Span::raw(format!("{}  {:<20} ", i + 1, s.name)),
                Span::styled(st.label(), Style::default().fg(status_color(st))),
            ]))
        })
        .collect();
    let mut state = app.list_state;
    state.select(Some(app.focus));
    let list = List::new(items)
        .block(Block::bordered().title(" Sessions — j/k:move Enter:go c:new Esc:close "))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, r, &mut state);
}

fn draw_history_list(
    frame: &mut Frame,
    area: Rect,
    entries: &[Entry],
    query: &str,
    selected: usize,
) {
    let r = overlay(area);
    frame.render_widget(Clear, r);
    let visible: Vec<&Entry> = entries
        .iter()
        .filter(|e| history::matches(e, query))
        .collect();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|e| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>4} ", history::age(e)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{} ", e.project), Style::default().fg(Color::Cyan)),
                Span::raw(e.title.clone()),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(selected.min(visible.len() - 1)));
    }
    let list = List::new(items)
        .block(Block::bordered().title(format!(
            " History ({}) — search: {}_  Enter:open ^r:resume Esc:close ",
            visible.len(),
            query
        )))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, r, &mut state);
}

fn draw_history_view(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    messages: &[Message],
    scroll: u16,
) {
    frame.render_widget(Clear, area);
    let mut lines: Vec<Line> = vec![];
    for m in messages {
        let (label, color) = if m.role == "user" {
            ("you", Color::Cyan)
        } else {
            ("claude", Color::Green)
        };
        lines.push(Line::from(Span::styled(
            format!("▌ {label}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));
        for l in m.text.lines() {
            lines.push(Line::raw(l.to_string()));
        }
        lines.push(Line::raw(""));
    }
    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(Block::bordered().title(format!(" {title} — j/k:scroll r:resume Esc:back ")));
    frame.render_widget(para, area);
}

/// Copy the emulated screen into the ratatui buffer.
fn render_screen(frame: &mut Frame, area: Rect, screen: &vt100::Screen, show_cursor: bool) {
    let buf = frame.buffer_mut();
    for row in 0..area.height {
        for col in 0..area.width {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let Some(out) = buf.cell_mut((area.x + col, area.y + row)) else {
                continue;
            };

            let mut style = Style::default()
                .fg(convert_color(cell.fgcolor()))
                .bg(convert_color(cell.bgcolor()));
            if cell.bold() {
                style = style.add_modifier(Modifier::BOLD);
            }
            if cell.dim() {
                style = style.add_modifier(Modifier::DIM);
            }
            if cell.italic() {
                style = style.add_modifier(Modifier::ITALIC);
            }
            if cell.underline() {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if cell.inverse() {
                style = style.add_modifier(Modifier::REVERSED);
            }

            if cell.has_contents() {
                out.set_symbol(cell.contents());
            }
            out.set_style(style);
        }
    }

    if show_cursor && !screen.hide_cursor() {
        let (row, col) = screen.cursor_position();
        if row < area.height && col < area.width {
            frame.set_cursor_position(Position::new(area.x + col, area.y + row));
        }
    }
}

fn convert_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Translate a key event into the bytes a terminal would send to the child.
fn encode_key(key: KeyEvent, application_cursor: bool) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    let cursor = |final_byte: u8| {
        let prefix = if application_cursor { b'O' } else { b'[' };
        vec![0x1b, prefix, final_byte]
    };

    let mut bytes = match key.code {
        KeyCode::Char(c) if ctrl && c.is_ascii() => match c.to_ascii_uppercase() as u8 {
            b' ' => vec![0],
            b @ b'@'..=b'_' => vec![b & 0x1f],
            b'?' => vec![0x7f],
            _ => vec![c as u8],
        },
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => cursor(b'A'),
        KeyCode::Down => cursor(b'B'),
        KeyCode::Right => cursor(b'C'),
        KeyCode::Left => cursor(b'D'),
        KeyCode::Home => cursor(b'H'),
        KeyCode::End => cursor(b'F'),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        _ => return None,
    };

    if alt {
        bytes.insert(0, 0x1b);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::Instant;

    const W: u16 = 60;
    const H: u16 = 14;

    /// An `App` that kills its child processes when the test ends.
    struct TestApp(App);

    impl Drop for TestApp {
        fn drop(&mut self) {
            for s in &mut self.0.sessions {
                s.kill();
            }
        }
    }

    fn screen() -> Rect {
        Rect::new(0, 0, W, H)
    }

    fn app(program: &str, args: &[&str]) -> TestApp {
        let mut a = App::new(program.into(), args.iter().map(|s| s.to_string()).collect());
        a.spawn(screen(), None);
        assert_eq!(a.sessions.len(), 1, "spawn failed: {:?}", a.error);
        TestApp(a)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Replay keys: `C-b` is Ctrl-b, `Esc`, `Enter`, `Tab`, `Space`, anything else is typed char by char.
    fn press(app: &mut TestApp, keys: &str) {
        for tok in keys.split(' ') {
            match tok {
                "C-b" => app.0.handle_key(
                    KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
                    screen(),
                ),
                "Esc" => app.0.handle_key(key(KeyCode::Esc), screen()),
                "Enter" => app.0.handle_key(key(KeyCode::Enter), screen()),
                "Tab" => app.0.handle_key(key(KeyCode::Tab), screen()),
                "Space" => app.0.handle_key(key(KeyCode::Char(' ')), screen()),
                text => {
                    for c in text.chars() {
                        app.0.handle_key(key(KeyCode::Char(c)), screen());
                    }
                }
            }
        }
    }

    /// Draw one frame to a `TestBackend` and return it as text, without trailing spaces.
    fn render(app: &mut TestApp) -> String {
        let rects = app.0.layout(screen());
        let mut term = Terminal::new(TestBackend::new(W, H)).unwrap();
        term.draw(|f| draw(f, &app.0, &rects)).unwrap();
        let buf = term.backend().buffer();
        (0..H)
            .map(|y| {
                (0..W)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn wait_for(app: &mut TestApp, needle: &str) {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let text = render(app);
            if text.contains(needle) {
                return;
            }
            assert!(
                Instant::now() < end,
                "timed out waiting for {needle:?}:\n{text}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn tiles_split_evenly() {
        let r = tiles(Rect::new(0, 0, 100, 20), 2, false);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].width + r[1].width, 100);
        let r = tiles(Rect::new(0, 0, 100, 20), 4, true);
        assert_eq!(r.iter().map(|r| r.height).sum::<u16>(), 20);
    }

    #[test]
    fn new_session_close_and_focus_keys() {
        let mut a = app("cat", &[]);
        press(&mut a, "C-b c");
        assert_eq!((a.0.sessions.len(), a.0.focus), (2, 1));
        press(&mut a, "C-b h");
        assert_eq!(a.0.focus, 0);
        press(&mut a, "C-b l");
        assert_eq!(a.0.focus, 1);
        press(&mut a, "C-b 1");
        assert_eq!(a.0.focus, 0);
        press(&mut a, "C-b 9");
        assert_eq!(a.0.focus, 0, "jumping past the last pane is ignored");
        press(&mut a, "C-b x");
        assert_eq!(a.0.sessions.len(), 1);
        assert!(!a.0.quit);
        press(&mut a, "C-b x");
        assert!(a.0.quit, "closing the last session quits");
    }

    #[test]
    fn layout_and_quit_keys() {
        let mut a = app("cat", &[]);
        assert!(!a.0.vertical);
        press(&mut a, "C-b v");
        assert!(a.0.vertical);
        press(&mut a, "C-b v");
        assert!(!a.0.vertical);
        press(&mut a, "C-b q");
        assert!(a.0.quit);
    }

    #[test]
    fn normal_keys_reach_the_session() {
        let mut a = app("cat", &[]);
        press(&mut a, "hello");
        wait_for(&mut a, "hello");
        // `C-b b` sends a literal Ctrl-b (echoed by the tty as ^B).
        press(&mut a, "C-b b");
        wait_for(&mut a, "^B");
    }

    #[test]
    fn dispatch_routes_to_the_mentioned_session() {
        let mut a = app("cat", &[]);
        press(&mut a, "C-b c");
        press(&mut a, "C-b i @1 Space routed Enter");
        assert!(matches!(a.0.mode, Mode::Normal));
        assert_eq!(a.0.focus, 0, "focus follows the target");
        wait_for(&mut a, "routed");
    }

    #[test]
    fn dispatch_escape_cancels_and_tab_picks_new_session() {
        let mut a = app("cat", &[]);
        press(&mut a, "C-b i abc Esc");
        assert!(matches!(a.0.mode, Mode::Normal));
        press(&mut a, "C-b i abc Tab Enter");
        assert_eq!(
            a.0.sessions.len(),
            2,
            "the last choice starts a new session"
        );
    }

    #[test]
    fn scroll_mode_enters_and_leaves() {
        let mut a = app("sh", &["-c", "seq 1 100; sleep 30"]);
        wait_for(&mut a, "100");
        press(&mut a, "C-b [");
        assert!(matches!(a.0.mode, Mode::Scroll));
        press(&mut a, "g");
        assert!(a.0.sessions[0].scroll_offset() > 0);
        press(&mut a, "Esc");
        assert!(matches!(a.0.mode, Mode::Normal));
        assert_eq!(a.0.sessions[0].scroll_offset(), 0);
    }

    #[test]
    fn history_key_opens_and_closes_the_list() {
        let mut a = app("cat", &[]);
        press(&mut a, "C-b H");
        assert!(matches!(a.0.mode, Mode::HistoryList { .. }));
        press(&mut a, "Esc");
        assert!(matches!(a.0.mode, Mode::Normal));
    }

    #[test]
    fn snapshot_single_pane() {
        let mut a = app("sh", &["-c", "echo hello from pane; sleep 30"]);
        wait_for(&mut a, "hello from pane");
        insta::assert_snapshot!(render(&mut a));
    }

    #[test]
    fn snapshot_two_panes_side_by_side_and_stacked() {
        let mut a = app("sh", &["-c", "echo first; sleep 30"]);
        wait_for(&mut a, "first");
        press(&mut a, "C-b c");
        // Both panes run the same command; wait until the new one has printed too.
        let end = Instant::now() + Duration::from_secs(5);
        while render(&mut a).matches("first").count() < 2 {
            assert!(Instant::now() < end, "second pane never printed");
            std::thread::sleep(Duration::from_millis(10));
        }
        insta::assert_snapshot!("two_panes_side_by_side", render(&mut a));
        press(&mut a, "C-b v");
        insta::assert_snapshot!("two_panes_stacked", render(&mut a));
    }

    #[test]
    fn snapshot_prefix_hint() {
        let mut a = app("cat", &[]);
        press(&mut a, "C-b");
        insta::assert_snapshot!(render(&mut a));
    }

    #[test]
    fn snapshot_session_list() {
        let mut a = app("cat", &[]);
        press(&mut a, "C-b c C-b s");
        insta::assert_snapshot!(render(&mut a));
    }

    #[test]
    fn snapshot_dispatch_prompt() {
        let mut a = app("cat", &[]);
        press(&mut a, "C-b c C-b i @2 Space fix Space the Space build");
        insta::assert_snapshot!(render(&mut a));
    }

    #[test]
    fn snapshot_scroll_mode() {
        let mut a = app("sh", &["-c", "seq 1 100; sleep 30"]);
        wait_for(&mut a, "100");
        press(&mut a, "C-b [ g");
        insta::assert_snapshot!(render(&mut a));
    }

    #[test]
    fn resume_from_history_view_starts_a_session() {
        let mut a = app("sh", &[]);
        a.0.mode = Mode::HistoryView {
            title: "t".into(),
            messages: vec![],
            scroll: 0,
            resume: ("abc".into(), None),
        };
        press(&mut a, "r");
        assert!(matches!(a.0.mode, Mode::Normal));
        assert_eq!(a.0.sessions.len(), 2, "error: {:?}", a.0.error);
    }

    #[test]
    fn snapshot_history_list_empty_and_view() {
        let mut a = app("cat", &[]);
        a.0.mode = Mode::HistoryList {
            entries: vec![],
            query: "nothing".into(),
            selected: 0,
        };
        insta::assert_snapshot!("history_list_empty", render(&mut a));
        a.0.mode = Mode::HistoryView {
            title: "demo project".into(),
            messages: vec![
                Message {
                    role: "user".into(),
                    text: "How do I scroll a pane?".into(),
                },
                Message {
                    role: "assistant".into(),
                    text: "Press Ctrl-b [ to enter scroll mode.".into(),
                },
            ],
            scroll: 0,
            resume: ("abc".into(), None),
        };
        insta::assert_snapshot!("history_view", render(&mut a));
    }
}
