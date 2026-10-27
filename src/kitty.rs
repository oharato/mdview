//! Kitty graphics protocol encoder.

use base64::Engine as _;

const CHUNK_BYTES: usize = 4096;

pub const KITTY_QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";

#[derive(Debug, Clone, Copy, Default)]
pub struct KittyPlacement {
    pub cols: Option<u32>,
    pub rows: Option<u32>,
    pub image_id: Option<u32>,
    pub hold_cursor: bool,
}

fn build_control(placement: KittyPlacement, more: u8, first: bool) -> String {
    let mut keys = Vec::new();
    if first {
        keys.push("a=T".to_string());
        keys.push("f=100".to_string());
        if let Some(image_id) = placement.image_id {
            keys.push(format!("i={image_id}"));
        }
        if placement.hold_cursor {
            keys.push("C=1".to_string());
        }
        if let Some(cols) = placement.cols {
            keys.push(format!("c={}", cols.max(1)));
        }
        if let Some(rows) = placement.rows {
            keys.push(format!("r={}", rows.max(1)));
        }
        keys.push("q=2".to_string());
    }
    keys.push(format!("m={more}"));
    keys.join(",")
}

/// Encode a PNG using the Kitty graphics protocol, splitting base64 into 4096-byte chunks.
pub fn encode_kitty(png: &[u8], placement: KittyPlacement) -> Vec<u8> {
    if png.is_empty() {
        return Vec::new();
    }

    let payload = base64::engine::general_purpose::STANDARD.encode(png);
    let mut output = Vec::new();
    let chunks: Vec<&str> = payload
        .as_bytes()
        .chunks(CHUNK_BYTES)
        .map(|chunk| std::str::from_utf8(chunk).expect("base64 is ASCII"))
        .collect();

    for (index, chunk) in chunks.iter().enumerate() {
        let more = if index + 1 == chunks.len() { 0 } else { 1 };
        let control = build_control(placement, more, index == 0);
        output.extend_from_slice(format!("\x1b_G{control};{chunk}\x1b\\").as_bytes());
    }
    output
}

/// Parse a Kitty query response. `None` means no Kitty response was seen.
pub fn parse_kitty_response(raw: &str) -> Option<bool> {
    if !raw.contains("\x1b_G") {
        return None;
    }
    Some(raw.contains(";OK"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    fn split_commands(data: &[u8]) -> Vec<(String, String)> {
        let text = String::from_utf8_lossy(data);
        let regex = Regex::new(r"\x1b_G([^;]*);([^\x1b]*)\x1b\\").expect("regex");
        regex
            .captures_iter(&text)
            .map(|captures| {
                (
                    captures.get(1).unwrap().as_str().to_string(),
                    captures.get(2).unwrap().as_str().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn sends_control_keys_and_chunks() {
        let png = vec![7u8; 12_000];
        let commands = split_commands(&encode_kitty(
            &png,
            KittyPlacement {
                cols: Some(40),
                rows: Some(10),
                hold_cursor: true,
                ..KittyPlacement::default()
            },
        ));
        assert_eq!(commands[0].0, "a=T,f=100,C=1,c=40,r=10,q=2,m=1");
        assert!(commands.len() > 1);
        for command in &commands[..commands.len() - 1] {
            assert_eq!(command.1.len(), 4096);
        }
        assert_eq!(commands.last().unwrap().0, "m=0");
    }

    #[test]
    fn encodes_image_id() {
        let commands = split_commands(&encode_kitty(
            &[1, 2, 3],
            KittyPlacement {
                image_id: Some(42),
                cols: Some(10),
                rows: Some(5),
                ..KittyPlacement::default()
            },
        ));
        assert_eq!(commands[0].0, "a=T,f=100,i=42,c=10,r=5,q=2,m=0");
    }

    #[test]
    fn empty_png_produces_nothing() {
        assert!(encode_kitty(&[], KittyPlacement::default()).is_empty());
    }

    #[test]
    fn parses_ok_and_error_responses() {
        assert_eq!(parse_kitty_response("\x1b_Gi=31;OK\x1b\\"), Some(true));
        assert_eq!(
            parse_kitty_response("\x1b_Gi=31;EINVAL:bad data\x1b\\"),
            Some(false)
        );
        assert_eq!(parse_kitty_response("\x1b[?62;c"), None);
    }
}
