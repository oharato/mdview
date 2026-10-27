//! Kitty graphics transfer self-test.
//!
//! Some terminals answer Kitty queries but cannot actually receive image data
//! (for example WezTerm behind ConPTY). Send a multi-chunk PNG and require an
//! OK response other than the initial query response.

use base64::Engine as _;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use regex::Regex;
use std::sync::LazyLock;

pub const KITTY_TEST_ID: u32 = 987_654;

static OK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_G([^;]*);OK").expect("valid kitty response regex"));

fn test_png() -> Vec<u8> {
    // 32x32 pseudo-random RGBA compresses poorly, so base64 is > 4096 bytes
    // (i.e. the self-test has to use multiple Kitty chunks).
    let mut data = vec![0u8; 32 * 32 * 4];
    let mut state = 0x1234_5678u32;
    for byte in &mut data {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *byte = (state >> 24) as u8;
    }
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&data, 32, 32, ExtendedColorType::Rgba8)
        .expect("PNG encode");
    png
}

/// Build the Kitty self-test sequence.
pub fn kitty_self_test_sequence() -> String {
    let payload = base64::engine::general_purpose::STANDARD.encode(test_png());
    let chunks: Vec<&str> = payload
        .as_bytes()
        .chunks(4096)
        .map(|chunk| std::str::from_utf8(chunk).expect("ASCII"))
        .collect();

    let mut output = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let more = if index + 1 == chunks.len() { 0 } else { 1 };
        let control = if index == 0 {
            format!("a=t,f=100,I={KITTY_TEST_ID},q=0,m={more}")
        } else {
            format!("m={more}")
        };
        output.push_str(&format!("\x1b_G{control};{chunk}\x1b\\"));
    }
    output.push_str(&format!("\x1b_Ga=d,d=n,I={KITTY_TEST_ID},q=2\x1b\\"));
    output
}

/// Return true when an OK response came from the image transfer, not the query.
pub fn parse_kitty_self_test(raw: &str) -> bool {
    OK_RE.captures_iter(raw).any(|captures| {
        let control = captures.get(1).map(|m| m.as_str()).unwrap_or("");
        !control.split(',').any(|part| part == "i=31")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_uses_multiple_chunks_and_cleans_up() {
        let sequence = kitty_self_test_sequence();
        assert!(sequence.matches("\x1b_G").count() > 2);
        assert!(sequence.contains("I=987654"));
        assert!(sequence.contains("a=d,d=n,I=987654"));
    }

    #[test]
    fn accepts_transfer_response_but_rejects_query_only() {
        assert!(!parse_kitty_self_test("\x1b_Gi=31;OK\x1b\\"));
        assert!(parse_kitty_self_test("\x1b_GI=12;OK\x1b\\"));
        assert!(parse_kitty_self_test("\x1b_G;OK\x1b\\"));
    }
}
