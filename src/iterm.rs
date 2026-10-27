//! iTerm2 inline image protocol.

use base64::Engine as _;

#[derive(Debug, Clone, Copy, Default)]
pub struct Iterm2Options<'a> {
    pub name: Option<&'a str>,
    pub width_px: Option<u32>,
    pub height_px: Option<u32>,
}

/// Encode a PNG using the iTerm2 OSC 1337 inline-image protocol.
pub fn encode_iterm2(png: &[u8], options: Iterm2Options<'_>) -> Vec<u8> {
    if png.is_empty() {
        return Vec::new();
    }

    let mut params = vec!["inline=1".to_string(), format!("size={}", png.len())];
    if let Some(name) = options.name {
        params.push(format!(
            "name={}",
            base64::engine::general_purpose::STANDARD.encode(name.as_bytes())
        ));
    }
    if let Some(width) = options.width_px {
        params.push(format!("width={}px", width.max(1)));
    }
    if let Some(height) = options.height_px {
        params.push(format!("height={}px", height.max(1)));
    }
    params.push("preserveAspectRatio=1".to_string());

    let payload = base64::engine::general_purpose::STANDARD.encode(png);
    format!("\x1b]1337;File={}:{}\x07", params.join(";"), payload).into_bytes()
}

fn detect_from(term_program: &str, lc_terminal: &str) -> bool {
    let program = term_program.to_lowercase();
    let lc_terminal = lc_terminal.to_lowercase();
    program == "iterm.app" || lc_terminal.contains("iterm") || program == "vscode"
}

/// Detect iTerm2 inline-image support from environment variables.
pub fn detect_iterm2() -> bool {
    detect_from(
        &std::env::var("TERM_PROGRAM").unwrap_or_default(),
        &std::env::var("LC_TERMINAL").unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_size_and_payload() {
        let png = [0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4];
        let output = String::from_utf8(encode_iterm2(
            &png,
            Iterm2Options {
                name: Some("図.png"),
                ..Iterm2Options::default()
            },
        ))
        .expect("ASCII");
        assert!(output.starts_with("\x1b]1337;File=inline=1;size=8;"));
        assert!(output.contains("preserveAspectRatio=1"));
        assert!(output.ends_with('\x07'));
        let payload = &output[output.find(':').unwrap() + 1..output.len() - 1];
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .expect("base64");
        assert_eq!(decoded, png);
    }

    #[test]
    fn detects_supported_terminals() {
        assert!(detect_from("iTerm.app", ""));
        assert!(detect_from("vscode", ""));
        assert!(detect_from("", "iTerm2"));
        assert!(!detect_from("Apple_Terminal", ""));
    }
}
