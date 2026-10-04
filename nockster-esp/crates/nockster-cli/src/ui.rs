//! Terminal presentation helpers for nockster-cli.
//!
//! Output is styled when stdout is a TTY and degrades to plain text when the
//! stream is piped or `NO_COLOR` is set — anstream strips escapes automatically,
//! so the same calls stay grep/pipe friendly. Decorative rules adapt to the
//! detected terminal width and are suppressed entirely off a TTY.

use std::io::IsTerminal;
use std::sync::OnceLock;

use anstyle::{Ansi256Color, Color, Effects, Reset, Style};

/// Whether stdout is an interactive terminal. Drives layout decisions (rules,
/// banners) that would only add noise to a pipe.
fn is_tty() -> bool {
    static TTY: OnceLock<bool> = OnceLock::new();
    *TTY.get_or_init(|| std::io::stdout().is_terminal())
}

/// Detected terminal width, clamped to a sane range. Falls back to 100 columns
/// when the width can't be determined (e.g. output is redirected). Memoized —
/// fine for one-shot command output. Interactive redraws that must survive a
/// terminal resize should call [`width_live`] instead.
pub fn width() -> usize {
    static W: OnceLock<usize> = OnceLock::new();
    *W.get_or_init(width_live)
}

/// Like [`width`] but re-queries the terminal every call, so a resize mid-render
/// is picked up. Used by the interactive prompts.
pub fn width_live() -> usize {
    terminal_size::terminal_size()
        .map(|(terminal_size::Width(w), _)| w as usize)
        .unwrap_or(100)
        .clamp(40, 120)
}

#[derive(Clone, Copy)]
enum Tier {
    Small,
    Medium,
    Large,
}

fn tier() -> Tier {
    match width() {
        0..=71 => Tier::Small,
        72..=103 => Tier::Medium,
        _ => Tier::Large,
    }
}

/// Column the key/value separator aligns to, widening with the terminal.
fn label_pad() -> usize {
    match tier() {
        Tier::Small => 10,
        Tier::Medium => 13,
        Tier::Large => 15,
    }
}

// ---- palette ---------------------------------------------------------------

const ACCENT: u8 = 51; // cyan
const DIMC: u8 = 244; // gray
const GOODC: u8 = 42; // green
const WARNC: u8 = 214; // amber
const BADC: u8 = 203; // red
const INFOC: u8 = 44; // teal

/// cyan → blue → violet ramp painted across decorative rules.
const RAMP: [u8; 7] = [51, 45, 39, 33, 63, 99, 135];

fn c(n: u8) -> Style {
    Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(n))))
}

fn paint(style: Style, s: &str) -> String {
    format!("{}{}{}", style.render(), s, Reset.render())
}

// ---- inline styling (return styled strings for embedding in values) --------

pub fn accent(s: &str) -> String {
    paint(c(ACCENT).effects(Effects::BOLD), s)
}
pub fn dim(s: &str) -> String {
    paint(c(DIMC), s)
}
pub fn strong(s: &str) -> String {
    paint(Style::new().effects(Effects::BOLD), s)
}
pub fn good(s: &str) -> String {
    paint(c(GOODC), s)
}
#[allow(dead_code)] // part of the palette; kept for callers that surface errors inline
pub fn bad(s: &str) -> String {
    paint(c(BADC), s)
}
pub fn amber(s: &str) -> String {
    paint(c(WARNC), s)
}

/// Health of a reported value, used to colour status dots.
#[derive(Clone, Copy)]
#[allow(dead_code)] // `Warn` rounds out the health scale for future amber states
pub enum Health {
    Good,
    Warn,
    Bad,
    Neutral,
}

/// A coloured status dot followed by a label, for embedding in a value column.
pub fn dot(state: Health, label: &str) -> String {
    let (col, glyph) = match state {
        Health::Good => (GOODC, "●"),
        Health::Warn => (WARNC, "●"),
        Health::Bad => (BADC, "●"),
        Health::Neutral => (DIMC, "○"),
    };
    format!("{} {}", paint(c(col), glyph), label)
}

/// `yes`/`no` rendered as a coloured dot (green/​dim).
pub fn yesno(value: bool) -> String {
    if value {
        dot(Health::Good, "yes")
    } else {
        dot(Health::Neutral, "no")
    }
}

// ---- decorative rules ------------------------------------------------------

fn gradient(line_char: char, len: usize) -> String {
    if len == 0 {
        return String::new();
    }
    let seg = (len + RAMP.len() - 1) / RAMP.len();
    let mut out = String::new();
    let mut drawn = 0usize;
    for &color in RAMP.iter() {
        if drawn >= len {
            break;
        }
        let count = seg.min(len - drawn);
        let chunk: String = std::iter::repeat(line_char).take(count).collect();
        out.push_str(&paint(c(color), &chunk));
        drawn += count;
    }
    out
}

// ---- blocks ----------------------------------------------------------------

/// Command banner: a left accent bar, the app name, and the command title over
/// a gradient rule. Off a TTY this collapses to a single plain line.
pub fn header(title: &str) {
    anstream::println!();
    if is_tty() {
        anstream::println!(
            "{} {}  {}  {}",
            paint(c(ACCENT).effects(Effects::BOLD), "▌"),
            accent("nockster"),
            dim("·"),
            strong(title),
        );
        anstream::println!("{}", gradient('─', width()));
    } else {
        anstream::println!("nockster — {title}");
    }
}

/// Full-width gradient divider. No-op off a TTY.
pub fn rule() {
    if is_tty() {
        anstream::println!("{}", gradient('─', width()));
    }
}

/// A dim, italic section label with a leading blank line.
pub fn subhead(text: &str) {
    anstream::println!();
    anstream::println!("  {}", paint(c(DIMC).effects(Effects::ITALIC), text));
}

/// An aligned `key   value` row. The value may carry its own styling.
pub fn kv(key: &str, value: impl AsRef<str>) {
    let pad = label_pad();
    let label = if key.len() >= pad {
        format!("{key} ")
    } else {
        format!("{key}{}", " ".repeat(pad - key.len()))
    };
    anstream::println!("  {}{}", dim(&label), value.as_ref());
}

/// An indented bullet item.
pub fn item(text: impl AsRef<str>) {
    anstream::println!("  {} {}", paint(c(ACCENT), "·"), text.as_ref());
}

/// A blank spacer line.
#[allow(dead_code)] // spacing helper kept available to commands
pub fn blank() {
    anstream::println!();
}

/// A plain, dim, indented note.
pub fn note(msg: &str) {
    anstream::println!("  {}", dim(msg));
}

// ---- status lines ----------------------------------------------------------

/// Success line (stdout).
pub fn ok(msg: &str) {
    anstream::println!("{} {}", good("✔"), msg);
}

/// Informational line (stdout).
pub fn info(msg: &str) {
    anstream::println!("{} {}", paint(c(INFOC), "›"), msg);
}

/// Warning line (stderr), matching the previous `eprintln!("warning: …")` sink.
pub fn warn(msg: &str) {
    anstream::eprintln!("{} {}", amber("▲"), msg);
}

// ---- progress --------------------------------------------------------------

fn human_bytes(n: u64) -> String {
    const K: f64 = 1024.0;
    let f = n as f64;
    if f >= K * K {
        format!("{:.1} MB", f / (K * K))
    } else if f >= K {
        format!("{:.0} KB", f / K)
    } else {
        format!("{n} B")
    }
}

/// An in-place, gradient-filled progress bar for streaming work (e.g. flashing
/// firmware). On a TTY it redraws on a single line; off a TTY it stays silent so
/// pipes capture only the final result lines.
pub struct Progress {
    label: String,
    total: u64,
    tty: bool,
    bar_w: usize,
}

impl Progress {
    pub fn new(label: &str, total: u64) -> Self {
        let bar_w = width().saturating_sub(34).clamp(10, 48);
        Self {
            label: label.to_string(),
            total,
            tty: is_tty(),
            bar_w,
        }
    }

    /// Redraw the bar at `current` bytes transferred.
    pub fn set(&self, current: u64) {
        if !self.tty {
            return;
        }
        let frac = if self.total == 0 {
            1.0
        } else {
            (current as f64 / self.total as f64).clamp(0.0, 1.0)
        };
        let filled = (frac * self.bar_w as f64).round() as usize;
        let mut bar = String::new();
        for i in 0..self.bar_w {
            if i < filled {
                let color = RAMP[(i * RAMP.len()) / self.bar_w.max(1)];
                bar.push_str(&paint(c(color), "█"));
            } else {
                bar.push_str(&paint(c(DIMC), "░"));
            }
        }
        let pct = (frac * 100.0).round() as u32;
        anstream::print!(
            "\r  {} {}  {}  {}",
            paint(c(ACCENT).effects(Effects::BOLD), "▌"),
            accent(&self.label),
            bar,
            dim(&format!(
                "{pct:3}%  {}/{}",
                human_bytes(current),
                human_bytes(self.total)
            )),
        );
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
    }

    /// Erase the bar so a following status line starts on a clean row.
    pub fn done(&self) {
        if self.tty {
            anstream::print!("\r\x1b[K");
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
        }
    }
}
