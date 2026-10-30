//! `Term`: the terminal for programs that draw a screen (a TUI). Raw mode, the size, the
//! modes (alternate screen, mouse...), writing a frame and reading the next key, mouse
//! movement, paste or resize. The terminal host provides it; nothing is offered when the
//! output is not a terminal, except the size and the colour depth.
//!
//! An event reaches the program as an array of strings (`["key", "Enter", "ctrl+shift"]`),
//! which the prelude turns into an `Event`.

use crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton,
    MouseEventKind,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, execute};
use qlang_core::host::{HostError, HostValue};
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;

pub const TERM_EXTERNS: &[&str] = &[
    "term_is_tty",
    "term_size",
    "term_colors",
    "term_raw",
    "term_mode",
    "term_write",
    "term_event",
];

static RAW: AtomicBool = AtomicBool::new(false);
/// The modes that were switched on (bits of `MODE_*`), to switch them off at the end.
static MODES: AtomicU8 = AtomicU8::new(0);

const MODE_ALT: u8 = 1;
const MODE_MOUSE: u8 = 2;
const MODE_PASTE: u8 = 4;
const MODE_FOCUS: u8 = 8;
const MODE_HIDDEN: u8 = 16;

fn fail(message: impl Into<String>) -> HostError {
    HostError::Other(message.into())
}

fn strings(items: &[&str]) -> HostValue {
    HostValue::Array(
        items
            .iter()
            .map(|s| HostValue::Str((*s).to_string()))
            .collect(),
    )
}

/// Puts the terminal back as it was: out of raw mode, the modes that were switched on
/// switched off, the cursor visible, the colours reset. Safe to call at any time, as often
/// as needed (the program ends, fails or panics in the middle of a screen).
pub fn restore() {
    let modes = MODES.swap(0, Ordering::SeqCst);
    let mut out = std::io::stdout();
    if modes & MODE_MOUSE != 0 {
        let _ = execute!(out, DisableMouseCapture);
    }
    if modes & MODE_PASTE != 0 {
        let _ = execute!(out, DisableBracketedPaste);
    }
    if modes & MODE_FOCUS != 0 {
        let _ = execute!(out, DisableFocusChange);
    }
    if modes & MODE_HIDDEN != 0 {
        let _ = execute!(out, cursor::Show);
    }
    if modes & MODE_ALT != 0 {
        let _ = execute!(out, LeaveAlternateScreen);
    }
    if RAW.swap(false, Ordering::SeqCst) {
        let _ = disable_raw_mode();
    }
    if modes != 0 {
        let _ = out.write_all(b"\x1b[0m");
        let _ = out.flush();
    }
}

fn from_env(name: &str) -> Option<u16> {
    std::env::var(name)
        .ok()?
        .trim()
        .parse()
        .ok()
        .filter(|n| *n > 0)
}

/// Columns and rows: the terminal's, else `COLUMNS` and `LINES`, else 80 by 24.
fn size() -> (u16, u16) {
    if std::io::stdout().is_terminal()
        && let Ok((c, r)) = crossterm::terminal::size()
        && c > 0
        && r > 0
    {
        return (c, r);
    }
    (
        from_env("COLUMNS").unwrap_or(80),
        from_env("LINES").unwrap_or(24),
    )
}

/// How many colours the terminal shows: 0 (none: not a terminal, `NO_COLOR`, `TERM=dumb`),
/// 16, 256 or 16777216 (true colour).
fn colors() -> i64 {
    let set = |name: &str| std::env::var(name).map(|v| !v.is_empty()).unwrap_or(false);
    if set("NO_COLOR") || !std::io::stdout().is_terminal() {
        return 0;
    }
    let term = std::env::var("TERM").unwrap_or_default();
    if term == "dumb" {
        return 0;
    }
    let truecolor = std::env::var("COLORTERM").unwrap_or_default();
    if truecolor == "truecolor" || truecolor == "24bit" || set("WT_SESSION") {
        return 16_777_216;
    }
    if term.contains("256color") {
        return 256;
    }
    if term.is_empty() && !cfg!(windows) {
        return 0;
    }
    16
}

fn modifiers(m: KeyModifiers) -> String {
    let mut parts = Vec::new();
    if m.contains(KeyModifiers::CONTROL) {
        parts.push("ctrl");
    }
    if m.contains(KeyModifiers::ALT) {
        parts.push("alt");
    }
    if m.contains(KeyModifiers::SHIFT) {
        parts.push("shift");
    }
    parts.join("+")
}

fn key_name(code: KeyCode, mods: &mut KeyModifiers) -> String {
    match code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::BackTab => {
            *mods |= KeyModifiers::SHIFT;
            "Tab".into()
        }
        KeyCode::Esc => "Esc".into(),
        KeyCode::Up => "Up".into(),
        KeyCode::Down => "Down".into(),
        KeyCode::Left => "Left".into(),
        KeyCode::Right => "Right".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PageUp".into(),
        KeyCode::PageDown => "PageDown".into(),
        KeyCode::Delete => "Delete".into(),
        KeyCode::Insert => "Insert".into(),
        KeyCode::F(n) => format!("F{n}"),
        _ => "Unknown".into(),
    }
}

fn button(b: MouseButton) -> &'static str {
    match b {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

/// The event as the array the prelude reads, or `None` for one that is of no interest (the
/// release of a key).
fn describe(ev: Event) -> Option<HostValue> {
    Some(match ev {
        Event::Key(k) => {
            if k.kind == KeyEventKind::Release {
                return None;
            }
            let mut mods = k.modifiers;
            let name = key_name(k.code, &mut mods);
            strings(&["key", &name, &modifiers(mods)])
        }
        Event::Mouse(m) => {
            let (action, who) = match m.kind {
                MouseEventKind::Down(b) => ("down", button(b)),
                MouseEventKind::Up(b) => ("up", button(b)),
                MouseEventKind::Drag(b) => ("drag", button(b)),
                MouseEventKind::Moved => ("move", "none"),
                MouseEventKind::ScrollUp => ("scroll_up", "none"),
                MouseEventKind::ScrollDown => ("scroll_down", "none"),
                MouseEventKind::ScrollLeft => ("scroll_left", "none"),
                MouseEventKind::ScrollRight => ("scroll_right", "none"),
            };
            strings(&[
                "mouse",
                action,
                who,
                &m.column.to_string(),
                &m.row.to_string(),
                &modifiers(m.modifiers),
            ])
        }
        Event::Resize(c, r) => strings(&["resize", &c.to_string(), &r.to_string()]),
        Event::Paste(text) => strings(&["paste", &text]),
        Event::FocusGained => strings(&["focus", "in"]),
        Event::FocusLost => strings(&["focus", "out"]),
    })
}

fn need_terminal() -> Result<(), HostError> {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        Ok(())
    } else {
        Err(fail(
            "this program needs a terminal (its input or output is redirected)",
        ))
    }
}

fn set_raw(on: bool) -> Result<(), HostError> {
    if on {
        need_terminal()?;
        enable_raw_mode()
            .map_err(|e| fail(format!("cannot switch the terminal to raw mode: {e}")))?;
        RAW.store(true, Ordering::SeqCst);
    } else if RAW.swap(false, Ordering::SeqCst) {
        disable_raw_mode().map_err(|e| fail(format!("cannot leave raw mode: {e}")))?;
    }
    Ok(())
}

fn set_mode(name: &str, on: bool) -> Result<(), HostError> {
    let bit = match name {
        "alt" => MODE_ALT,
        "mouse" => MODE_MOUSE,
        "paste" => MODE_PASTE,
        "focus" => MODE_FOCUS,
        "hide_cursor" => MODE_HIDDEN,
        other => return Err(fail(format!("unknown terminal mode `{other}`"))),
    };
    need_terminal()?;
    let mut out = std::io::stdout();
    let done = match (bit, on) {
        (MODE_ALT, true) => execute!(out, EnterAlternateScreen),
        (MODE_ALT, false) => execute!(out, LeaveAlternateScreen),
        (MODE_MOUSE, true) => execute!(out, EnableMouseCapture),
        (MODE_MOUSE, false) => execute!(out, DisableMouseCapture),
        (MODE_PASTE, true) => execute!(out, EnableBracketedPaste),
        (MODE_PASTE, false) => execute!(out, DisableBracketedPaste),
        (MODE_FOCUS, true) => execute!(out, EnableFocusChange),
        (MODE_FOCUS, false) => execute!(out, DisableFocusChange),
        (_, true) => execute!(out, cursor::Hide),
        (_, false) => execute!(out, cursor::Show),
    };
    done.map_err(|e| fail(e.to_string()))?;
    if on {
        MODES.fetch_or(bit, Ordering::SeqCst);
    } else {
        MODES.fetch_and(!bit, Ordering::SeqCst);
    }
    Ok(())
}

fn next_event(timeout_ms: i64) -> Result<HostValue, HostError> {
    need_terminal()?;
    let started = std::time::Instant::now();
    loop {
        let wait = if timeout_ms < 0 {
            Duration::from_millis(500)
        } else {
            Duration::from_millis(timeout_ms as u64)
                .saturating_sub(started.elapsed())
                .min(Duration::from_millis(500))
        };
        let ready = event::poll(wait).map_err(|e| fail(e.to_string()))?;
        if ready {
            let ev = event::read().map_err(|e| fail(e.to_string()))?;
            if let Some(found) = describe(ev) {
                return Ok(found);
            }
            continue;
        }
        if timeout_ms >= 0 && started.elapsed() >= Duration::from_millis(timeout_ms as u64) {
            return Ok(strings(&["timeout"]));
        }
    }
}

/// Runs one of the `TERM_EXTERNS`.
pub fn call(name: &str, args: &[HostValue]) -> Result<HostValue, HostError> {
    let wrong = || fail(format!("`{name}` was called with the wrong arguments"));
    match (name, args) {
        ("term_is_tty", [HostValue::Int(which)]) => Ok(HostValue::Bool(match which {
            0 => std::io::stdin().is_terminal(),
            _ => std::io::stdout().is_terminal(),
        })),
        ("term_size", []) => {
            let (c, r) = size();
            Ok(HostValue::Array(vec![
                HostValue::Int(c as i64),
                HostValue::Int(r as i64),
            ]))
        }
        ("term_colors", []) => Ok(HostValue::Int(colors())),
        ("term_raw", [HostValue::Bool(on)]) => set_raw(*on).map(|()| HostValue::Bool(true)),
        ("term_mode", [HostValue::Str(mode), HostValue::Bool(on)]) => {
            set_mode(mode, *on).map(|()| HostValue::Bool(true))
        }
        ("term_write", [HostValue::Str(text)]) => {
            let mut out = std::io::stdout().lock();
            out.write_all(text.as_bytes())
                .and_then(|()| out.flush())
                .map(|()| HostValue::Bool(true))
                .map_err(|e| fail(e.to_string()))
        }
        ("term_event", [HostValue::Int(timeout)]) => next_event(*timeout),
        _ => Err(wrong()),
    }
}
