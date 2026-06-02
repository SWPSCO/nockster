//! Interactive seed-phrase entry for the `seed` command.
//!
//! When the user runs `seed` without an input source (no `--seedphrase`,
//! `--seed-hex`, or a slot-management flag), and stdin/stdout are both a TTY,
//! we drop into a live word-by-word prompt. Every keystroke filters the 2048
//! BIP-39 English words, autocompletes against them, and refuses anything that
//! isn't on the list — so the phrase that comes out is always well-formed.
//!
//! The block is redrawn in raw mode on each keypress; raw mode is always torn
//! down via the [`RawGuard`] drop, even on error or panic.

use std::io::{self, IsTerminal, Write};

use anstyle::{Ansi256Color, Color, Effects, Reset, Style};
use bip39::{Language, Mnemonic};
use crossterm::cursor::{Hide, MoveToColumn, MoveUp, Show};
use crossterm::event::{read, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use crossterm::{execute, queue};

use crate::ui;

const WL: Language = Language::English;
/// BIP-39 mnemonics are valid only at these word counts.
const VALID_LENGTHS: [usize; 5] = [12, 15, 18, 21, 24];
/// Most suggestion chips drawn under the input at once.
const MAX_SUGGESTIONS: usize = 7;

/// True when an interactive prompt is possible (both ends are a real terminal).
pub fn available() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

// ---- raw-mode guard --------------------------------------------------------

struct RawGuard;

impl RawGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(RawGuard)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), Show);
    }
}

// ---- styling ---------------------------------------------------------------

const ACCENT: u8 = 51;

/// Highlighted "chip" style for the selected suggestion: cyan field, dark text.
fn chip_style() -> Style {
    Style::new()
        .bg_color(Some(Color::Ansi256(Ansi256Color(ACCENT))))
        .fg_color(Some(Color::Ansi256(Ansi256Color(16))))
        .effects(Effects::BOLD)
}

fn chip(s: &str) -> String {
    format!("{} {} {}", chip_style().render(), s, Reset.render())
}

// ---- mnemonic entry --------------------------------------------------------

/// Interactively build a BIP-39 mnemonic. Returns `None` if the user cancels.
pub fn read_mnemonic() -> anyhow::Result<Option<String>> {
    ui::subhead("seed phrase");
    ui::note("type each word — autocompletes against the 2048-word BIP-39 list");
    ui::note("tab/space accept · ↑↓ choose · ⏎ commit · ⌫ delete · esc cancel");

    let _guard = RawGuard::enter()?;
    let mut out = io::stdout();
    queue!(out, Hide)?;
    out.flush()?;

    let mut words: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut sel: usize = 0;
    let mut error: Option<String> = None;
    let mut prev_lines = 0usize;
    let cancelled;

    loop {
        // Recompute suggestions for the current prefix.
        let matches: &[&str] = if buf.is_empty() {
            &[]
        } else {
            WL.words_by_prefix(&buf)
        };
        let shown = &matches[..matches.len().min(MAX_SUGGESTIONS)];
        if sel >= shown.len() {
            sel = shown.len().saturating_sub(1);
        }

        prev_lines = render_block(&mut out, prev_lines, &words, &buf, shown, sel, &error)?;
        error = None;

        let ev = read()?;
        let Event::Key(key) = ev else { continue };
        if key.kind == KeyEventKind::Release {
            continue;
        }

        // Ctrl-C cancels (raw mode swallows the usual SIGINT).
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c'))
        {
            cancelled = true;
            break;
        }

        match key.code {
            KeyCode::Esc => {
                cancelled = true;
                break;
            }
            KeyCode::Char(c) if c.is_ascii_alphabetic() => {
                buf.push(c.to_ascii_lowercase());
                sel = 0;
            }
            KeyCode::Up | KeyCode::Left => {
                sel = sel.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Right => {
                if !shown.is_empty() {
                    sel = (sel + 1).min(shown.len() - 1);
                }
            }
            KeyCode::Tab | KeyCode::Char(' ') => {
                if shown.is_empty() {
                    if !buf.is_empty() {
                        error = Some(format!("no BIP-39 word matches “{buf}”"));
                    }
                } else if let Some(msg) = commit(&mut words, shown[sel]) {
                    error = Some(msg);
                } else {
                    buf.clear();
                    sel = 0;
                }
            }
            KeyCode::Enter => {
                if buf.is_empty() {
                    // Finish — but only at a valid length.
                    let n = words.len();
                    if VALID_LENGTHS.contains(&n) {
                        cancelled = false;
                        break;
                    }
                    error = Some(format!(
                        "{n} words — need 12, 15, 18, 21, or 24 to finish"
                    ));
                } else if let Some(word) = resolve(&buf, shown, sel) {
                    if let Some(msg) = commit(&mut words, &word) {
                        error = Some(msg);
                    } else {
                        buf.clear();
                        sel = 0;
                    }
                } else {
                    error = Some(format!("“{buf}” is not a BIP-39 word"));
                }
            }
            KeyCode::Backspace => {
                if !buf.is_empty() {
                    buf.pop();
                } else if let Some(last) = words.pop() {
                    // Pull the previous word back for editing.
                    buf = last;
                }
                sel = 0;
            }
            _ => {}
        }
    }

    // Leave the cursor on a clean line below the (now final) block.
    queue!(out, MoveToColumn(0))?;
    write!(out, "\r\n")?;
    out.flush()?;
    drop(_guard);

    if cancelled {
        ui::warn("seed entry cancelled");
        return Ok(None);
    }

    let phrase = words.join(" ");
    ui::ok(&format!("captured {} words", words.len()));

    // Length and membership are already guaranteed; this catches checksum typos.
    if Mnemonic::parse_in(Language::English, &phrase).is_err() {
        ui::warn("BIP-39 checksum does not match — likely a mistyped word");
        if !confirm("use this phrase anyway?")? {
            return Ok(None);
        }
    } else {
        ui::note("checksum ok");
    }

    Ok(Some(phrase))
}

/// Pick the word a commit should use: the highlighted suggestion when there is
/// one, otherwise the buffer itself if it is an exact word.
fn resolve(buf: &str, shown: &[&str], sel: usize) -> Option<String> {
    if let Some(w) = shown.get(sel) {
        return Some((*w).to_string());
    }
    WL.find_word(buf).map(|_| buf.to_string())
}

/// Append a word unless we're already at the 24-word ceiling. Returns an error
/// message to surface, or `None` on success.
fn commit(words: &mut Vec<String>, word: &str) -> Option<String> {
    if words.len() >= 24 {
        return Some("24 words is the maximum".to_string());
    }
    words.push(word.to_string());
    None
}

#[allow(clippy::too_many_arguments)]
fn render_block(
    out: &mut io::Stdout,
    prev_lines: usize,
    words: &[String],
    buf: &str,
    shown: &[&str],
    sel: usize,
    error: &Option<String>,
) -> io::Result<usize> {
    if prev_lines > 0 {
        queue!(out, MoveUp(prev_lines as u16))?;
    }
    queue!(out, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;

    let lines = build_lines(words, buf, shown, sel, error);
    for line in &lines {
        queue!(out, MoveToColumn(0))?;
        write!(out, "{}\r\n", line)?;
    }
    out.flush()?;
    Ok(lines.len())
}

fn build_lines(
    words: &[String],
    buf: &str,
    shown: &[&str],
    sel: usize,
    error: &Option<String>,
) -> Vec<String> {
    let width = ui::width();
    let mut lines: Vec<String> = Vec::new();

    // 1. Words collected so far, wrapped to the terminal width.
    if words.is_empty() {
        lines.push(format!("  {}", ui::dim("(no words yet)")));
    } else {
        let indent = "  ";
        let mut line = String::from(indent);
        let mut len = indent.len();
        for (i, word) in words.iter().enumerate() {
            let idx = format!("{:>2} ", i + 1);
            let token_len = idx.len() + word.len() + 2;
            if len + token_len > width && len > indent.len() {
                lines.push(line);
                line = String::from(indent);
                len = indent.len();
            }
            line.push_str(&ui::dim(&idx));
            line.push_str(&ui::strong(word));
            line.push_str("  ");
            len += token_len;
        }
        lines.push(line);
    }

    lines.push(String::new());

    // 2. The live input line with a fake block cursor.
    let body = if buf.is_empty() {
        ui::dim("type a word…")
    } else {
        ui::strong(buf)
    };
    lines.push(format!(
        "  {} {}{}",
        ui::accent("▌"),
        body,
        ui::accent("▏")
    ));

    // 3. Suggestions.
    if buf.is_empty() {
        lines.push(format!("    {}", ui::dim("⌁ 2048 words available")));
    } else if shown.is_empty() {
        lines.push(format!(
            "    {}",
            ui::amber(&format!("✗ no BIP-39 word starts with “{buf}”"))
        ));
    } else {
        let mut line = String::from("    ");
        for (i, word) in shown.iter().enumerate() {
            if i == sel {
                line.push_str(&chip(word));
            } else {
                line.push_str(&ui::dim(word));
            }
            line.push_str("  ");
        }
        lines.push(line);
    }

    // 4. Status / error.
    if let Some(msg) = error {
        lines.push(format!("  {}", ui::amber(&format!("▲ {msg}"))));
    } else {
        let n = words.len();
        let count = if VALID_LENGTHS.contains(&n) {
            ui::good(&format!("✓ {n} words"))
        } else {
            ui::dim(&format!("{n} words"))
        };
        lines.push(format!(
            "  {}  {}",
            count,
            ui::dim("lengths 12/15/18/21/24 · ⏎ on empty to finish")
        ));
    }

    lines
}

// ---- PIN entry -------------------------------------------------------------

/// Prompt for the device PIN. When `confirm` is set (the device has no PIN yet),
/// the PIN must be entered twice and match before it's accepted — so a typo on
/// first setup can't lock the user out. Returns `None` if the user cancels.
pub fn read_pin(confirm: bool) -> anyhow::Result<Option<String>> {
    ui::subhead("PIN");
    if confirm {
        ui::note("no PIN set yet — choose one, then re-enter it to confirm");
    } else {
        ui::note("enter the device PIN to unlock and add this seed");
    }

    loop {
        let Some(pin) = read_secret("PIN", false)? else {
            return Ok(None);
        };
        if !confirm {
            return Ok(Some(pin));
        }
        let Some(again) = read_secret("confirm PIN", false)? else {
            return Ok(None);
        };
        if pin == again {
            ui::ok("PIN confirmed");
            return Ok(Some(pin));
        }
        ui::warn("PINs did not match — try again");
    }
}

// ---- masked secret entry ---------------------------------------------------

fn read_secret(label: &str, allow_empty: bool) -> anyhow::Result<Option<String>> {
    let _guard = RawGuard::enter()?;
    let mut out = io::stdout();
    queue!(out, Hide)?;
    out.flush()?;

    let mut buf = String::new();
    let mut hint = false;
    let result;

    loop {
        queue!(out, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        let masked = ui::accent(&"•".repeat(buf.chars().count()));
        let tail = if hint && buf.is_empty() {
            ui::amber("  (required)")
        } else {
            String::new()
        };
        write!(
            out,
            "  {} {} {}{}",
            ui::accent("▌"),
            ui::dim(label),
            masked,
            tail
        )?;
        out.flush()?;

        let ev = read()?;
        let Event::Key(key) = ev else { continue };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            result = None;
            break;
        }
        match key.code {
            KeyCode::Esc => {
                result = None;
                break;
            }
            KeyCode::Enter => {
                if buf.is_empty() && !allow_empty {
                    hint = true;
                } else {
                    result = Some(buf.clone());
                    break;
                }
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) if !c.is_control() => {
                buf.push(c);
                hint = false;
            }
            _ => {}
        }
    }

    queue!(out, MoveToColumn(0))?;
    write!(out, "\r\n")?;
    out.flush()?;
    Ok(result)
}

/// A simple cooked-mode y/N confirmation (raw mode must already be off).
fn confirm(question: &str) -> anyhow::Result<bool> {
    use std::io::Write as _;
    print!("  {} {} ", ui::amber("?"), question);
    print!("{}", ui::dim("[y/N] "));
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "YES"))
}
