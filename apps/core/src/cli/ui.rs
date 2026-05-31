//! # Terminal UI Helpers
//!
//! Beautiful terminal output with colors, Unicode symbols, and box-drawing.
//! Detects terminal capabilities and falls back gracefully to plain text.

use std::io::{self, Write};

/// ANSI color codes for terminal output.
/// All output goes to stderr to keep stdout clean (in case of piping).
pub struct Colors;

impl Colors {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const RED: &str = "\x1b[91m";
    pub const GREEN: &str = "\x1b[92m";
    pub const YELLOW: &str = "\x1b[93m";
    pub const CYAN: &str = "\x1b[96m";
    pub const WHITE: &str = "\x1b[97m";
    pub const ORANGE: &str = "\x1b[38;5;208m";
}

/// Whether the terminal supports ANSI colors.
pub fn supports_color() -> bool {
    // Check NO_COLOR standard: https://no-color.org
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    // Check if stderr is a TTY
    atty_stderr()
}

/// Check if stderr is a terminal (not piped).
fn atty_stderr() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::isatty(2) != 0 }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let handle = io::stderr().as_raw_handle();
        // If we can get console mode, it's a real console
        unsafe {
            let mut mode: u32 = 0;
            windows::Win32::System::Console::GetConsoleMode(
                windows::Win32::Foundation::HANDLE(handle as _),
                &mut mode,
            )
            .is_ok()
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Color wrapper — returns colored text if supported, plain text otherwise.
fn c(color: &str, text: &str) -> String {
    if supports_color() {
        format!("{}{}{}", color, text, Colors::RESET)
    } else {
        text.to_string()
    }
}

// ─── Print Helpers ───────────────────────────────────────────────────────────

pub fn print_ok(msg: &str) {
    eprintln!("  {}  {}", c(Colors::GREEN, "✓"), msg);
}

pub fn print_warn(msg: &str) {
    eprintln!("  {}  {}", c(Colors::YELLOW, "⚠"), c(Colors::YELLOW, msg));
}

pub fn print_fail(msg: &str) {
    eprintln!("  {}  {}", c(Colors::RED, "✗"), c(Colors::RED, msg));
}

pub fn print_skip(msg: &str) {
    eprintln!("  {}  {}", c(Colors::DIM, "○"), c(Colors::DIM, msg));
}

pub fn print_info(msg: &str) {
    eprintln!("  {}  {}", c(Colors::CYAN, "→"), msg);
}

pub fn print_step(msg: &str) {
    eprintln!();
    eprintln!("  {}", c(Colors::BOLD, msg));
}

pub fn print_line() {
    eprintln!(
        "  {}",
        c(Colors::DIM, "──────────────────────────────────────────────────")
    );
}

// ─── Box Drawing ─────────────────────────────────────────────────────────────

/// Print a box banner with title and optional subtitle.
pub fn print_banner(icon: &str, title: &str, subtitle: Option<&str>) {
    let version = env!("CARGO_PKG_VERSION");
    let title_line = format!("{} {} v{}", icon, title, version);
    let width = 44;

    eprintln!();
    eprintln!("  {}", c(Colors::ORANGE, &format!("╭{}╮", "─".repeat(width))));
    eprintln!(
        "  {}  {}{}",
        c(Colors::ORANGE, "│"),
        c(&format!("{}{}", Colors::BOLD, Colors::WHITE), &title_line),
        " ".repeat(width.saturating_sub(title_line.chars().count() + 2))
            .chars()
            .collect::<String>()
            + &c(Colors::ORANGE, "│")
    );
    if let Some(sub) = subtitle {
        eprintln!(
            "  {}  {}{}",
            c(Colors::ORANGE, "│"),
            c(Colors::DIM, sub),
            " ".repeat(width.saturating_sub(sub.chars().count() + 2))
                .chars()
                .collect::<String>()
                + &c(Colors::ORANGE, "│")
        );
    }
    eprintln!("  {}", c(Colors::ORANGE, &format!("╰{}╯", "─".repeat(width))));
    eprintln!();
}

/// Print a result summary box.
pub fn print_result_box(color: &str, message: &str) {
    let width = 44;
    eprintln!();
    eprintln!("  {}", c(color, &format!("╭{}╮", "─".repeat(width))));
    eprintln!(
        "  {}  {}{}",
        c(color, "│"),
        message,
        " ".repeat(width.saturating_sub(strip_ansi_len(message) + 2))
            .chars()
            .collect::<String>()
            + &c(color, "│")
    );
    eprintln!("  {}", c(color, &format!("╰{}╯", "─".repeat(width))));
    eprintln!();
}

/// Approximate length of a string excluding ANSI escape codes.
fn strip_ansi_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
        } else {
            len += 1;
        }
    }
    len
}

/// Prompt the user for yes/no confirmation. Returns true if 'y' or 'Y'.
pub fn confirm(prompt: &str) -> bool {
    eprint!("  {} [y/N] ", prompt);
    let _ = io::stderr().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}
