//! Heading extraction and section slicing.

use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1-based sequential number (`--section N`).
    pub index: usize,
    /// Number of `#` characters.
    pub level: usize,
    pub text: String,
    /// 0-based source line number.
    pub line: usize,
}

static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.+?)\s*#*\s*$").expect("valid heading regex"));
static FENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*(```+|~~~+)").expect("valid fence regex"));

/// List headings in document order. Headings inside code fences are ignored.
pub fn list_headings(markdown: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut in_fence = false;
    let mut fence_char = '\0';

    for (line_number, line) in markdown.lines().enumerate() {
        if let Some(captures) = FENCE_RE.captures(line) {
            let marker = captures[1].chars().next().unwrap_or('`');
            if !in_fence {
                in_fence = true;
                fence_char = marker;
            } else if marker == fence_char {
                in_fence = false;
            }
            continue;
        }
        if in_fence {
            continue;
        }

        if let Some(captures) = HEADING_RE.captures(line) {
            let hashes = captures.get(1).map(|m| m.as_str()).unwrap_or("");
            let text = captures.get(2).map(|m| m.as_str()).unwrap_or("").trim();
            if !hashes.is_empty() && !text.is_empty() {
                headings.push(Heading {
                    index: headings.len() + 1,
                    level: hashes.len(),
                    text: text.to_string(),
                    line: line_number,
                });
            }
        }
    }

    headings
}

/// Return the body of heading `index`, up to the next heading of the same or
/// higher level. Returns `None` when the heading does not exist.
pub fn extract_section(markdown: &str, index: usize) -> Option<String> {
    let headings = list_headings(markdown);
    let target = headings.iter().find(|heading| heading.index == index)?;
    let next = headings
        .iter()
        .find(|heading| heading.index > target.index && heading.level <= target.level);

    let lines: Vec<&str> = markdown.lines().collect();
    let end = next.map(|heading| heading.line).unwrap_or(lines.len());
    if target.line >= end {
        return Some(String::new());
    }
    let body = lines[target.line..end].join("\n");
    Some(format!("{}\n", body.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# タイトル\n\nはじめに\n\n## 準備\n\n```bash\n# コード内\n```\n\n本文\n\n## 使い方\n\n### 応用\n\n応用の本文\n\n## まとめ\n\nおわり";

    #[test]
    fn lists_headings_and_ignores_fences() {
        let headings = list_headings(DOC);
        let summary: Vec<String> = headings
            .iter()
            .map(|heading| format!("{}:{}:{}", heading.index, heading.level, heading.text))
            .collect();
        assert_eq!(
            summary,
            [
                "1:1:タイトル",
                "2:2:準備",
                "3:2:使い方",
                "4:3:応用",
                "5:2:まとめ"
            ]
        );
        assert!(!headings
            .iter()
            .any(|heading| heading.text.contains("コード内")));
    }

    #[test]
    fn extracts_section_body() {
        let section = extract_section(DOC, 3).expect("section");
        assert!(section.contains("## 使い方"));
        assert!(section.contains("### 応用"));
        assert!(section.contains("応用の本文"));
        assert!(!section.contains("## まとめ"));
    }

    #[test]
    fn missing_section_returns_none() {
        assert!(extract_section(DOC, 0).is_none());
        assert!(extract_section(DOC, 99).is_none());
    }
}
