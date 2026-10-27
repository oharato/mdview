//! Terminal probing and capability detection.

use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::sync::{LazyLock, OnceLock};
use std::time::{Duration, Instant};

use regex::Regex;
use rustix::fs::{Mode, OFlags};
use rustix::io::Errno;
use rustix::termios::{tcgetattr, tcsetattr, OptionalActions};

use crate::kitty::{parse_kitty_response, KITTY_QUERY};
use crate::paths::{caps_path, ensure_dir};
use crate::selftest::{kitty_self_test_sequence, parse_kitty_self_test};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProbeResult {
    pub sixel: Option<bool>,
    pub kitty: Option<bool>,
    pub kitty_transfer: Option<bool>,
    pub cell: Option<(u32, u32)>,
    pub window: Option<(u32, u32)>,
    pub terminal: Option<(String, String)>,
    /// Terminal background colour from OSC 11, if the terminal answered.
    pub background: Option<[u8; 3]>,
}

static DA1_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[\?([0-9;]+)c").expect("DA1 regex"));
static CELL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[6;(\d+);(\d+)t").expect("cell regex"));
static WINDOW_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[4;(\d+);(\d+)t").expect("window regex"));
static XTVERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1bP>\|([^\x1b]*)\x1b\\").expect("XTVERSION regex"));
static BACKGROUND_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\x1b\]11;rgb:([0-9a-fA-F]{1,4})/([0-9a-fA-F]{1,4})/([0-9a-fA-F]{1,4})(?:\x07|\x1b\\)",
    )
    .expect("OSC 11 regex")
});

fn parse_color_component(value: &str) -> Option<u8> {
    let parsed = u32::from_str_radix(value, 16).ok()?;
    let max = (1u32 << (4 * value.len() as u32)) - 1;
    Some(((parsed * 255) / max) as u8)
}

const TIMEOUT: Duration = Duration::from_millis(300);

fn parse_xtversion(raw: &str) -> Option<(String, String)> {
    let captures = XTVERSION_RE.captures(raw)?;
    let text = captures.get(1)?.as_str().trim();
    if text.is_empty() {
        return None;
    }
    let mut parts = text.split_whitespace();
    let name = parts.next().unwrap_or("").to_string();
    let version = parts.collect::<Vec<_>>().join(" ");
    Some((name, version))
}

/// Parse a terminal probe response.
pub fn parse_probe(raw: &str) -> ProbeResult {
    let mut result = ProbeResult {
        sixel: None,
        kitty: parse_kitty_response(raw),
        kitty_transfer: if raw.contains("_G") {
            Some(parse_kitty_self_test(raw))
        } else {
            None
        },
        cell: None,
        window: None,
        terminal: parse_xtversion(raw),
        background: None,
    };

    if let Some(captures) = DA1_RE.captures(raw) {
        if let Some(attributes) = captures.get(1) {
            result.sixel = Some(attributes.as_str().split(';').any(|value| value == "4"));
        }
    }
    if let Some(captures) = CELL_RE.captures(raw) {
        let height = captures.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
        let width = captures.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
        if let (Some(width), Some(height)) = (width, height) {
            if width > 0 && height > 0 {
                result.cell = Some((width, height));
            }
        }
    }
    if let Some(captures) = BACKGROUND_RE.captures(raw) {
        let component = |index: usize| {
            captures
                .get(index)
                .and_then(|m| parse_color_component(m.as_str()))
        };
        if let (Some(r), Some(g), Some(b)) = (component(1), component(2), component(3)) {
            result.background = Some([r, g, b]);
        }
    }
    if let Some(captures) = WINDOW_RE.captures(raw) {
        let height = captures.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
        let width = captures.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
        if let (Some(width), Some(height)) = (width, height) {
            if width > 0 && height > 0 {
                result.window = Some((width, height));
            }
        }
    }
    result
}

fn is_complete(raw: &str) -> bool {
    // CSI 14t (window pixels) is optional: many terminals never answer it.
    // Requiring it made every probe wait for the full timeout.
    DA1_RE.is_match(raw)
        && CELL_RE.is_match(raw)
        && raw.contains("_G")
        && parse_kitty_self_test(raw)
}

/// True when stdout is connected to a terminal.
pub fn is_interactive() -> bool {
    std::io::stdout().is_terminal()
}

fn write_all(fd: &impl std::os::fd::AsFd, mut data: &[u8]) -> std::io::Result<()> {
    while !data.is_empty() {
        match rustix::io::write(fd, data) {
            Ok(0) => return Err(std::io::Error::other("short write")),
            Ok(written) => data = &data[written..],
            Err(Errno::AGAIN) => std::thread::sleep(Duration::from_millis(1)),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn probe_tty() -> Option<ProbeResult> {
    let fd = rustix::fs::open("/dev/tty", OFlags::RDWR | OFlags::NONBLOCK, Mode::empty()).ok()?;

    let saved = tcgetattr(&fd).ok();
    if let Some(mut raw) = saved.clone() {
        raw.make_raw();
        let _ = tcsetattr(&fd, OptionalActions::Now, &raw);
    }

    let query = format!(
        "\x1b]11;?\x07\x1b[c\x1b[16t\x1b[14t\x1b[>0q{}{}",
        KITTY_QUERY,
        kitty_self_test_sequence()
    );
    let mut raw = String::new();
    let started = Instant::now();
    let mut connected = false;

    if write_all(&fd, query.as_bytes()).is_ok() {
        connected = true;
        let mut buffer = [0u8; 4096];
        while started.elapsed() < TIMEOUT {
            match rustix::io::read(&fd, &mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    raw.push_str(&String::from_utf8_lossy(&buffer[..read]));
                    if is_complete(&raw) {
                        break;
                    }
                }
                Err(Errno::AGAIN) => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    }

    if let Some(saved) = saved {
        let _ = tcsetattr(&fd, OptionalActions::Now, &saved);
    }

    if connected {
        Some(parse_probe(&raw))
    } else {
        None
    }
}

#[cfg(not(unix))]
fn probe_tty() -> Option<ProbeResult> {
    None
}

/// Probe the terminal once per process.
pub fn probe_terminal() -> ProbeResult {
    static PROBE: OnceLock<ProbeResult> = OnceLock::new();
    PROBE
        .get_or_init(|| {
            if !is_interactive() {
                return ProbeResult::default();
            }
            probe_tty().unwrap_or_default()
        })
        .clone()
}

fn caps_key() -> String {
    [
        "TERM",
        "TERM_PROGRAM",
        "TERM_PROGRAM_VERSION",
        "SSH_CONNECTION",
    ]
    .iter()
    .map(|name| std::env::var(name).unwrap_or_default())
    .collect::<Vec<_>>()
    .join("|")
}

fn read_caps() -> HashMap<String, bool> {
    std::fs::read_to_string(caps_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_caps(caps: &HashMap<String, bool>) {
    if let Some(parent) = caps_path().parent() {
        let _ = ensure_dir(parent);
    }
    if let Ok(raw) = serde_json::to_string(caps) {
        let _ = std::fs::write(caps_path(), raw);
    }
}

fn env_override(name: &str) -> Option<bool> {
    let value = std::env::var(name).ok()?;
    if value.is_empty() {
        return None;
    }
    Some(!matches!(
        value.to_ascii_lowercase().as_str(),
        "0" | "false" | "no"
    ))
}

/// Sixel support (environment override, cache, then terminal probe).
pub fn sixel_supported(refresh: bool) -> bool {
    if let Some(override_value) = env_override("MDVIEW_SIXEL") {
        return override_value;
    }

    let key = caps_key();
    let mut cache = read_caps();
    if !refresh {
        if let Some(value) = cache.get(&key) {
            return *value;
        }
    }

    let probe = probe_terminal();
    let supported = probe.sixel.unwrap_or(false);
    cache.insert(key, supported);
    write_caps(&cache);
    supported
}

/// WezTerm detection from `TERM_PROGRAM` or XTVERSION.
pub fn is_wez_term() -> bool {
    if std::env::var("TERM_PROGRAM")
        .map(|value| value.eq_ignore_ascii_case("wezterm"))
        .unwrap_or(false)
    {
        return true;
    }
    terminal_name() == "wezterm"
}

/// Terminal name reported by XTVERSION, lowercased.
pub fn terminal_name() -> String {
    probe_terminal()
        .terminal
        .map(|(name, _)| name.to_lowercase())
        .unwrap_or_default()
}

/// Kitty graphics support.
pub fn kitty_supported() -> bool {
    if let Some(override_value) = env_override("MDVIEW_KITTY") {
        return override_value;
    }
    let probe = probe_terminal();
    if probe.kitty == Some(true) {
        if is_wez_term() {
            return probe.kitty_transfer == Some(true);
        }
        return true;
    }

    if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return true;
    }
    if std::env::var("TERM")
        .map(|value| value.contains("kitty"))
        .unwrap_or(false)
    {
        return true;
    }
    matches!(
        std::env::var("TERM_PROGRAM")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "wezterm" | "ghostty" | "mintty"
    )
}

/// Cell pixel size (`MDVIEW_CELL=18x36` overrides).
pub fn cell_pixels() -> (u32, u32) {
    if let Ok(value) = std::env::var("MDVIEW_CELL") {
        let parts: Vec<&str> = value.trim().split(['x', 'X', ',']).map(str::trim).collect();
        if parts.len() == 2 {
            if let (Ok(width), Ok(height)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                if width > 0 && height > 0 {
                    return (width, height);
                }
            }
        }
    }

    if let Some(cell) = probe_terminal().cell {
        return cell;
    }
    // Last resort; HiDPI users can correct this with --pixel-scale.
    (9, 18)
}

pub fn window_pixels() -> Option<(u32, u32)> {
    probe_terminal().window
}

/// Terminal background colour reported by OSC 11.
pub fn terminal_background() -> Option<[u8; 3]> {
    probe_terminal().background
}

/// Terminal size in cells. Piped output uses the same 100x40 default as the
/// Node implementation; interactive output uses the real terminal size.
pub fn terminal_size() -> (u16, u16) {
    if !is_interactive() {
        return (100, 40);
    }
    match crossterm::terminal::size() {
        Ok((columns, rows)) if columns >= 20 && rows >= 5 => (columns, rows),
        _ => (100, 40),
    }
}

/// Kitty transfer test status for `--check`.
pub fn kitty_transfer_status() -> &'static str {
    let probe = probe_terminal();
    match (probe.kitty, probe.kitty_transfer) {
        (Some(false), _) | (None, _) => "n/a (kitty 未対応)",
        (_, Some(true)) => "ok",
        (_, Some(false)) => "失敗 (画像データが届かない)",
        (_, None) => "未検証",
    }
}

/// Print a warning that cannot fail the program.
pub fn warn(message: &str) {
    let _ = writeln!(std::io::stderr(), "\x1b[2m[mdview] {message}\x1b[0m");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_da1_cell_window_and_xtversion() {
        let raw = "\x1b[?62;4;22c\x1b[6;36;18t\x1b[4;1200;2400t\x1bP>|WezTerm 20240203\x1b\\\x1b]11;rgb:1e1e/1e1e/2020\x07";
        let probe = parse_probe(raw);
        assert_eq!(probe.sixel, Some(true));
        assert_eq!(probe.cell, Some((18, 36)));
        assert_eq!(probe.window, Some((2400, 1200)));
        assert_eq!(
            probe.terminal,
            Some(("WezTerm".to_string(), "20240203".to_string()))
        );
        assert_eq!(probe.background, Some([0x1e, 0x1e, 0x20]));
    }

    #[test]
    fn reports_no_sixel_without_da1_attribute() {
        let probe = parse_probe("\x1b[?62;22c");
        assert_eq!(probe.sixel, Some(false));
    }

    #[test]
    fn parses_kitty_query_and_transfer() {
        let query = "\x1b_Gi=31;OK\x1b\\";
        let transfer = "\x1b_GI=12;OK\x1b\\";
        let probe = parse_probe(&format!("{query}{transfer}"));
        assert_eq!(probe.kitty, Some(true));
        assert_eq!(probe.kitty_transfer, Some(true));
    }
}
