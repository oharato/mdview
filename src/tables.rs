//! CJK-aware GFM table extraction and rendering.
//!
//! `pulldown-cmark-mdcat` computes column widths in bytes and pads by
//! character count, which breaks tables containing CJK or emoji. mdview has
//! always rendered tables itself; this is the Rust port of that logic.

use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableBlock {
    pub alignments: Vec<Alignment>,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

static FENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*(```+|~~~+)").expect("valid fence regex"));
static DELIMITER_CELL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^:?-+:?$").expect("valid delimiter regex"));
static IMAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[([^\]]*)\]\([^)]*\)").expect("valid image regex"));
static LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").expect("valid link regex"));
static CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([^`]*)`").expect("valid code regex"));
static BOLD_STAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*\*([^*]+)\*\*").expect("valid bold regex"));
static BOLD_UNDERSCORE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"__([^_]+)__").expect("valid bold regex"));
static ITALIC_STAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*([^*]+)\*").expect("valid italic regex"));
static ITALIC_UNDERSCORE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_([^_]+)_").expect("valid italic regex"));
static STRIKE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"~~([^~]+)~~").expect("valid strike regex"));

const WIDE_RANGES: &[(u32, u32)] = &[
    (0x1100, 0x115f),
    (0x2e80, 0x303e),
    (0x3041, 0x33ff),
    (0x3400, 0x4dbf),
    (0x4e00, 0x9fff),
    (0xa000, 0xa4cf),
    (0xa960, 0xa97f),
    (0xac00, 0xd7a3),
    (0xf900, 0xfaff),
    (0xfe10, 0xfe19),
    (0xfe30, 0xfe6f),
    (0xff00, 0xff60),
    (0xffe0, 0xffe6),
    (0x16fe0, 0x16fe4),
    (0x17000, 0x18cff),
    (0x1b000, 0x1b2ff),
    (0x1f004, 0x1f004),
    (0x1f0cf, 0x1f0cf),
    (0x1f18e, 0x1f18e),
    (0x1f191, 0x1f19a),
    (0x1f200, 0x1f320),
    (0x1f32d, 0x1f335),
    (0x1f337, 0x1f37c),
    (0x1f37e, 0x1f393),
    (0x1f3a0, 0x1f3ca),
    (0x1f3cf, 0x1f3d3),
    (0x1f3e0, 0x1f3f0),
    (0x1f3f4, 0x1f3f4),
    (0x1f3f8, 0x1f43e),
    (0x1f440, 0x1f440),
    (0x1f442, 0x1f4fc),
    (0x1f4ff, 0x1f53d),
    (0x1f54b, 0x1f54e),
    (0x1f550, 0x1f567),
    (0x1f57a, 0x1f57a),
    (0x1f595, 0x1f596),
    (0x1f5a4, 0x1f5a4),
    (0x1f5fb, 0x1f64f),
    (0x1f680, 0x1f6c5),
    (0x1f6cc, 0x1f6cc),
    (0x1f6d0, 0x1f6d2),
    (0x1f6d5, 0x1f6d7),
    (0x1f6eb, 0x1f6ec),
    (0x1f6f4, 0x1f6fc),
    (0x1f7e0, 0x1f7eb),
    (0x1f90c, 0x1f93a),
    (0x1f93c, 0x1f945),
    (0x1f947, 0x1f9ff),
    (0x1fa70, 0x1faff),
    (0x20000, 0x3fffd),
];

fn is_zero_width(code: u32) -> bool {
    (0x0300..=0x036f).contains(&code)
        || (0x1ab0..=0x1aff).contains(&code)
        || (0x1dc0..=0x1dff).contains(&code)
        || (0x20d0..=0x20f0).contains(&code)
        || (0xfe00..=0xfe0f).contains(&code)
        || (0xfe20..=0xfe2f).contains(&code)
        || (0x200b..=0x200f).contains(&code)
        || code == 0xfeff
}

fn is_wide(code: u32) -> bool {
    WIDE_RANGES
        .iter()
        .any(|(start, end)| code >= *start && code <= *end)
}

/// Terminal display width, counting East Asian wide characters as 2 cells.
pub fn display_width(text: &str) -> usize {
    text.chars()
        .map(|ch| {
            let code = ch as u32;
            if code == 0 || is_zero_width(code) {
                0
            } else if is_wide(code) {
                2
            } else {
                1
            }
        })
        .sum()
}

fn split_row(line: &str) -> Vec<String> {
    let mut body = line.trim();
    if let Some(rest) = body.strip_prefix('|') {
        body = rest;
    }
    if let Some(rest) = body.strip_suffix('|') {
        if !body.ends_with("\\|") {
            body = rest;
        }
    }

    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            current.push('|');
            chars.next();
        } else if ch == '|' {
            cells.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    cells.push(current.trim().to_string());
    cells
}

fn parse_alignment(cell: &str) -> Alignment {
    let left = cell.starts_with(':');
    let right = cell.ends_with(':');
    if left && right {
        Alignment::Center
    } else if right {
        Alignment::Right
    } else {
        Alignment::Left
    }
}

fn is_delimiter_row(line: &str) -> bool {
    if !line.contains('-') {
        return false;
    }
    let cells = split_row(line);
    !cells.is_empty()
        && cells
            .iter()
            .all(|cell| DELIMITER_CELL_RE.is_match(cell.trim()))
}

/// Extract GFM tables and replace them with `@@mdview-table-N-@@` lines.
pub fn extract_tables(text: &str) -> (String, Vec<TableBlock>) {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut output = Vec::new();
    let mut tables = Vec::new();
    let mut in_fence = false;
    let mut fence_char = '\0';
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if let Some(captures) = FENCE_RE.captures(line) {
            let marker = captures[1].chars().next().unwrap_or('`');
            if !in_fence {
                in_fence = true;
                fence_char = marker;
            } else if marker == fence_char {
                in_fence = false;
            }
            output.push(line.to_string());
            i += 1;
            continue;
        }
        if in_fence {
            output.push(line.to_string());
            i += 1;
            continue;
        }

        if line.contains('|') {
            if let Some(next) = lines.get(i + 1) {
                if is_delimiter_row(next) {
                    let header = split_row(line);
                    let alignments = split_row(next)
                        .iter()
                        .map(|cell| parse_alignment(cell))
                        .collect();
                    let mut rows = Vec::new();
                    let mut j = i + 2;
                    while j < lines.len() {
                        let candidate = lines[j];
                        if !candidate.contains('|') || candidate.trim().is_empty() {
                            break;
                        }
                        let cells = split_row(candidate);
                        if cells.len() != header.len() {
                            break;
                        }
                        rows.push(cells);
                        j += 1;
                    }

                    output.push(String::new());
                    output.push(format!("@@mdview-table-{}-@@", tables.len()));
                    output.push(String::new());
                    tables.push(TableBlock {
                        alignments,
                        header,
                        rows,
                    });
                    i = j;
                    continue;
                }
            }
        }

        output.push(line.to_string());
        i += 1;
    }

    (output.join("\n"), tables)
}

/// Strip inline Markdown for width calculation.
pub fn strip_inline(text: &str) -> String {
    let mut output = IMAGE_RE.replace_all(text, "$1").into_owned();
    output = LINK_RE.replace_all(&output, "$1").into_owned();
    output = CODE_RE.replace_all(&output, "$1").into_owned();
    output = BOLD_STAR_RE.replace_all(&output, "$1").into_owned();
    output = BOLD_UNDERSCORE_RE.replace_all(&output, "$1").into_owned();
    output = ITALIC_STAR_RE.replace_all(&output, "$1").into_owned();
    output = ITALIC_UNDERSCORE_RE.replace_all(&output, "$1").into_owned();
    STRIKE_RE.replace_all(&output, "$1").into_owned()
}

/// Convert inline Markdown to ANSI, or plain text when `colour` is false.
pub fn format_inline(text: &str, colour: bool) -> String {
    if !colour {
        return strip_inline(text);
    }
    let mut output = LINK_RE
        .replace_all(text, |caps: &regex::Captures<'_>| {
            let label = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let url = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            if url.is_empty() {
                label.to_string()
            } else {
                format!("\x1b]8;;{url}\x1b\\{label}\x1b]8;;\x1b\\")
            }
        })
        .into_owned();
    output = CODE_RE
        .replace_all(&output, "\x1b[33m$1\x1b[0m")
        .into_owned();
    output = BOLD_STAR_RE
        .replace_all(&output, "\x1b[1m$1\x1b[0m")
        .into_owned();
    output = BOLD_UNDERSCORE_RE
        .replace_all(&output, "\x1b[1m$1\x1b[0m")
        .into_owned();
    output = ITALIC_STAR_RE
        .replace_all(&output, "\x1b[3m$1\x1b[0m")
        .into_owned();
    output = ITALIC_UNDERSCORE_RE
        .replace_all(&output, "\x1b[3m$1\x1b[0m")
        .into_owned();
    STRIKE_RE
        .replace_all(&output, "\x1b[9m$1\x1b[0m")
        .into_owned()
}

fn truncate(text: &str, width: usize) -> String {
    let plain = strip_inline(text);
    if width == 0 {
        return String::new();
    }
    if display_width(&plain) <= width {
        return plain;
    }
    let mut output = String::new();
    let mut used = 0;
    for ch in plain.chars() {
        let char_width = display_width(&ch.to_string());
        if used + char_width > width - 1 {
            break;
        }
        output.push(ch);
        used += char_width;
    }
    output.push('…');
    output
}

/// Render a table with display-width-aware padding.
pub fn render_table(table: &TableBlock, max_width: usize, colour: bool) -> String {
    let columns = table.header.len();
    if columns == 0 {
        return String::new();
    }

    let mut header = table.header.clone();
    header.resize(columns, String::new());
    let rows: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| {
            let mut cells = row.clone();
            cells.resize(columns, String::new());
            cells.truncate(columns);
            cells
        })
        .collect();

    let mut widths = vec![1usize; columns];
    for (index, cell) in header.iter().enumerate() {
        widths[index] = display_width(&strip_inline(cell)).max(1);
    }
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(display_width(&strip_inline(cell)));
        }
    }

    let overhead = 4 * columns - 2;
    let total_width = |widths: &[usize]| widths.iter().sum::<usize>() + overhead;
    while total_width(&widths) > max_width {
        let mut widest: Option<usize> = None;
        let mut widest_width = 3;
        for (index, width) in widths.iter().enumerate() {
            if *width > widest_width {
                widest_width = *width;
                widest = Some(index);
            }
        }
        let Some(index) = widest else {
            break;
        };
        widths[index] -= 1;
    }

    let rule_text = "─".repeat(total_width(&widths).max(1));
    let rule = if colour {
        format!("\x1b[32m{rule_text}\x1b[0m")
    } else {
        rule_text
    };

    let render_row = |cells: &[String], bold: bool| -> String {
        cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let width = widths[index];
                let plain = strip_inline(cell);
                let shown = if display_width(&plain) > width {
                    truncate(cell, width)
                } else {
                    cell.clone()
                };
                let mut formatted = format_inline(&shown, colour);
                if bold && colour {
                    formatted = format!("\x1b[1m{formatted}\x1b[0m");
                }
                let pad = width.saturating_sub(display_width(&strip_inline(&shown)));
                let (left, right) = match table
                    .alignments
                    .get(index)
                    .copied()
                    .unwrap_or(Alignment::Left)
                {
                    Alignment::Right => (pad, 0),
                    Alignment::Center => (pad / 2, pad - pad / 2),
                    Alignment::Left => (0, pad),
                };
                format!(" {}{}{} ", " ".repeat(left), formatted, " ".repeat(right))
            })
            .collect::<Vec<_>>()
            .join("  ")
    };

    let mut lines = vec![rule.clone(), render_row(&header, true), rule.clone()];
    for row in &rows {
        lines.push(render_row(row, false));
    }
    lines.push(rule);
    format!("{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_width_counts_cjk_and_emoji() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width("あいう"), 6);
        assert_eq!(display_width("半ブロック"), 10);
        assert_eq!(display_width("ＡＢ"), 4);
        assert_eq!(display_width("🎨"), 2);
        assert_eq!(display_width("e\u{0301}"), 1);
    }

    #[test]
    fn extracts_table_structure() {
        let markdown = "| 名前 | 値 |\n|:---|---:|\n| あ | 1 |\n| b | 22 |\n\nafter";
        let (text, tables) = extract_tables(markdown);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].header, ["名前", "値"]);
        assert_eq!(tables[0].alignments, [Alignment::Left, Alignment::Right]);
        assert_eq!(tables[0].rows, [["あ", "1"], ["b", "22"]]);
        assert!(text.contains("@@mdview-table-0-@@"));
        assert!(text.contains("after"));
    }

    #[test]
    fn ignores_tables_inside_fences() {
        let code = "```markdown\n| a | b |\n|---|---|\n| 1 | 2 |\n```";
        assert!(extract_tables(code).1.is_empty());
    }

    #[test]
    fn handles_escaped_pipes() {
        let (_, tables) = extract_tables("| a | b |\n|---|---|\n| x \\| y | z |");
        assert_eq!(tables[0].rows[0], ["x | y", "z"]);
    }

    #[test]
    fn rendered_table_has_uniform_width() {
        let table = TableBlock {
            alignments: vec![Alignment::Left, Alignment::Right],
            header: vec!["プロトコル".into(), "解像度".into()],
            rows: vec![
                vec!["Kitty graphics".into(), "フル".into()],
                vec!["半ブロック".into(), "低".into()],
            ],
        };
        let output = render_table(&table, 80, false);
        let widths: std::collections::HashSet<usize> =
            output.trim_end().lines().map(display_width).collect();
        assert_eq!(widths.len(), 1);
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn truncates_to_max_width() {
        let table = TableBlock {
            alignments: vec![Alignment::Left, Alignment::Right],
            header: vec!["プロトコル".into(), "解像度".into()],
            rows: vec![vec!["Kitty graphics とても長い説明".into(), "フル".into()]],
        };
        let output = render_table(&table, 24, false);
        for line in output.trim_end().lines() {
            assert!(display_width(line) <= 24);
        }
    }

    #[test]
    fn formats_inline_markup() {
        assert!(format_inline("**太字**", true).contains("\x1b[1m"));
        assert!(format_inline("`code`", true).contains("\x1b[33m"));
        assert_eq!(format_inline("**太字**", false), "太字");
    }
}
