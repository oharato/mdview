//! Markdown segmentation: text, Mermaid fences, standalone images, tables.
//!
//! This mirrors the TypeScript implementation: assets are replaced with
//! sentinel lines, then the document is split so mdcat only sees text.

use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidBlock {
    pub code: String,
    pub info: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBlock {
    pub src: String,
    pub alt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Asset(usize),
    Table(usize),
}

static FENCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([ \t]*)(`{3,}|~{3,})[ \t]*mermaid\b(.*)$").expect("valid fence regex")
});
static IMAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^!\[([^\]]*)\]\(\s*<?([^)>\s]+)>?(?:\s+(?:"[^"]*"|'[^']*'))?\s*\)$"#)
        .expect("valid image regex")
});
static ASSET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^@@mdview-asset-(\d+)-@@[ \t]*$").expect("valid asset regex"));
static TABLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^@@mdview-table-(\d+)-@@[ \t]*$").expect("valid table regex"));

fn asset_sentinel(index: usize) -> String {
    format!("@@mdview-asset-{index}-@@")
}

fn is_fence_close(line: &str, fence_char: char, fence_len: usize) -> bool {
    let trimmed = line.trim();
    let marker = fence_char.to_string().repeat(fence_len);
    trimmed.starts_with(&marker)
        && trimmed
            .chars()
            .all(|ch| ch == fence_char || ch == ' ' || ch == '\t')
}

/// Replace ```mermaid fences with asset placeholders.
pub fn extract_mermaid(text: &str) -> (String, Vec<MermaidBlock>) {
    let mut output = Vec::new();
    let mut blocks = Vec::new();
    let mut lines = text.split('\n').peekable();

    while let Some(line) = lines.next() {
        let Some(captures) = FENCE_RE.captures(line) else {
            output.push(line.to_string());
            continue;
        };
        let fence = captures.get(2).map(|m| m.as_str()).unwrap_or("");
        let fence_char = fence.chars().next().unwrap_or('`');
        let fence_len = fence.len();

        let mut body = Vec::new();
        for inner in lines.by_ref() {
            if is_fence_close(inner, fence_char, fence_len) {
                break;
            }
            body.push(inner);
        }

        output.push(String::new());
        output.push(asset_sentinel(blocks.len()));
        output.push(String::new());
        blocks.push(MermaidBlock {
            code: format!("{}\n", body.join("\n")),
            info: captures
                .get(3)
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim()
                .to_string(),
        });
    }

    (output.join("\n"), blocks)
}

/// Replace standalone image lines with asset placeholders.
///
/// Inline images stay in the text, matching the current mdview behaviour.
pub fn extract_images(text: &str, offset: usize) -> (String, Vec<ImageBlock>) {
    let mut output = Vec::new();
    let mut blocks = Vec::new();

    for line in text.split('\n') {
        let trimmed = line.trim();
        if let Some(captures) = IMAGE_RE.captures(trimmed) {
            if captures.get(0).map(|m| m.as_str()) == Some(trimmed) {
                output.push(String::new());
                output.push(asset_sentinel(offset + blocks.len()));
                output.push(String::new());
                blocks.push(ImageBlock {
                    src: captures
                        .get(2)
                        .map(|m| m.as_str())
                        .unwrap_or("")
                        .to_string(),
                    alt: captures
                        .get(1)
                        .map(|m| m.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
                continue;
            }
        }
        output.push(line.to_string());
    }

    (output.join("\n"), blocks)
}

/// Split a document on asset and table placeholders.
pub fn split_segments(text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut buffer = Vec::new();

    let flush = |buffer: &mut Vec<String>, segments: &mut Vec<Segment>| {
        let body = buffer.join("\n");
        let body = body.trim_matches('\n');
        if !body.trim().is_empty() {
            segments.push(Segment::Text(body.to_string()));
        }
        buffer.clear();
    };

    for line in text.split('\n') {
        if let Some(captures) = ASSET_RE.captures(line) {
            flush(&mut buffer, &mut segments);
            if let Some(index) = captures
                .get(1)
                .and_then(|m| m.as_str().parse::<usize>().ok())
            {
                segments.push(Segment::Asset(index));
                continue;
            }
        }
        if let Some(captures) = TABLE_RE.captures(line) {
            flush(&mut buffer, &mut segments);
            if let Some(index) = captures
                .get(1)
                .and_then(|m| m.as_str().parse::<usize>().ok())
            {
                segments.push(Segment::Table(index));
                continue;
            }
        }
        buffer.push(line.to_string());
    }
    flush(&mut buffer, &mut segments);
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_mermaid_and_leaves_other_code_blocks() {
        let markdown =
            "# A\n\n```mermaid\nflowchart LR\n  A-->B\n```\n\n```rust\nfn main() {}\n```\n";
        let (text, blocks) = extract_mermaid(markdown);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "flowchart LR\n  A-->B\n");
        assert!(text.contains("@@mdview-asset-0-@@"));
        assert!(text.contains("fn main()"));
    }

    #[test]
    fn extracts_standalone_images_only() {
        let markdown = "前に ![inline](a.png) です\n\n![alt](b.png)\n";
        let (text, blocks) = extract_images(markdown, 0);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].src, "b.png");
        assert_eq!(blocks[0].alt, "alt");
        assert!(text.contains("![inline](a.png)"));
    }

    #[test]
    fn splits_assets_and_tables() {
        let markdown = "intro\n\n@@mdview-asset-0-@@\n\nmiddle\n\n@@mdview-table-2-@@\n\nend";
        let segments = split_segments(markdown);
        assert_eq!(
            segments,
            [
                Segment::Text("intro".to_string()),
                Segment::Asset(0),
                Segment::Text("middle".to_string()),
                Segment::Table(2),
                Segment::Text("end".to_string()),
            ]
        );
    }
}
