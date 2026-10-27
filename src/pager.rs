//! Built-in pager with heading navigation.

use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::sync::LazyLock;

use anyhow::Result;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, SetAttribute};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, queue};
use regex::Regex;

use crate::sections::Heading;
use crate::tables::display_width;

pub enum ImagePayload {
    Ready(Vec<u8>),
    Lazy(Box<dyn FnOnce() -> Vec<u8>>),
}

pub enum Block {
    Text(Vec<String>),
    Image { rows: u16, payload: ImagePayload },
}

#[derive(Debug, Clone)]
pub struct HeadingMark {
    pub index: usize,
    pub level: usize,
    pub text: String,
    pub block: usize,
    pub line: usize,
}

#[derive(Default)]
pub struct BufferSink {
    pub blocks: Vec<Block>,
    pub marks: Vec<HeadingMark>,
    expected: VecDeque<Heading>,
}

impl BufferSink {
    pub fn expect_headings(&mut self, headings: Vec<Heading>) {
        self.expected.extend(headings);
    }

    /// Store a text chunk as one block. Heading lines are matched against the
    /// headings registered with [`expect_headings`].
    pub fn push_text(&mut self, body: &str) {
        if body.is_empty() {
            return;
        }
        let mut lines: Vec<String> = body.split('\n').map(str::to_string).collect();
        if lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        if lines.is_empty() {
            return;
        }

        let block = self.blocks.len();
        for (line_index, line) in lines.iter().enumerate() {
            if !is_heading_line(line) {
                continue;
            }
            if let Some(heading) = self.expected.pop_front() {
                self.marks.push(HeadingMark {
                    index: heading.index,
                    level: heading.level,
                    text: heading.text,
                    block,
                    line: line_index,
                });
            }
        }
        self.blocks.push(Block::Text(lines));
    }

    pub fn push_image(&mut self, rows: u16, payload: Vec<u8>) {
        self.blocks.push(Block::Image {
            rows,
            payload: ImagePayload::Ready(payload),
        });
    }

    pub fn push_lazy_image(&mut self, rows: u16, render: Box<dyn FnOnce() -> Vec<u8>>) {
        self.blocks.push(Block::Image {
            rows,
            payload: ImagePayload::Lazy(render),
        });
    }
}

static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").expect("ANSI regex"));

pub fn strip_ansi(text: &str) -> String {
    ANSI_RE.replace_all(text, "").into_owned()
}

/// True when this is an mdcat heading line (starts with `┄` after ANSI is removed).
pub fn is_heading_line(line: &str) -> bool {
    strip_ansi(line).starts_with('┄')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageEntry {
    Text { block: usize, line: usize },
    Image { block: usize, rows: u16 },
}

/// Split blocks into pages that fit `body_rows` terminal rows.
pub fn paginate(blocks: &[Block], body_rows: u16) -> Vec<Vec<PageEntry>> {
    let body_rows = body_rows.max(1);
    let mut pages = Vec::new();
    let mut page = Vec::new();
    let mut used = 0u16;

    let flush = |page: &mut Vec<PageEntry>, pages: &mut Vec<Vec<PageEntry>>, used: &mut u16| {
        if !page.is_empty() {
            pages.push(std::mem::take(page));
        }
        *used = 0;
    };

    for (block_index, block) in blocks.iter().enumerate() {
        match block {
            Block::Text(lines) => {
                for (line_index, _) in lines.iter().enumerate() {
                    if used >= body_rows {
                        flush(&mut page, &mut pages, &mut used);
                    }
                    page.push(PageEntry::Text {
                        block: block_index,
                        line: line_index,
                    });
                    used += 1;
                }
            }
            Block::Image { rows, .. } => {
                let rows = (*rows).min(body_rows);
                if used > 0 && used + rows > body_rows {
                    flush(&mut page, &mut pages, &mut used);
                }
                page.push(PageEntry::Image {
                    block: block_index,
                    rows,
                });
                used += rows;
            }
        }
    }
    flush(&mut page, &mut pages, &mut used);
    pages
}

fn truncate_to_width(text: &str, width: u16) -> String {
    let width = width as usize;
    if display_width(text) <= width {
        return text.to_string();
    }
    let mut output = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = display_width(&ch.to_string());
        if used + w > width.saturating_sub(1) {
            break;
        }
        output.push(ch);
        used += w;
    }
    output.push('…');
    output
}

fn pad_to_width(text: &str, width: u16) -> String {
    let truncated = truncate_to_width(text, width);
    let used = display_width(&truncated);
    let mut output = truncated;
    if used < width as usize {
        output.push_str(&" ".repeat(width as usize - used));
    }
    output
}

const HELP: &str = "space:次へ b:前へ ]/[ 見出し t:一覧 g/G:先頭/末尾 q:終了";

fn current_heading(heading_pages: &[(HeadingMark, usize)], page: usize) -> Option<&HeadingMark> {
    let mut found: Option<&HeadingMark> = None;
    for (heading, heading_page) in heading_pages {
        if *heading_page <= page {
            found = Some(heading);
        } else {
            break;
        }
    }
    found
}

pub fn run_pager(
    blocks: &mut [Block],
    marks: &[HeadingMark],
    rows: u16,
    columns: u16,
) -> Result<()> {
    let mut rows = rows.max(5);
    let mut columns = columns.max(20);
    let mut pages = paginate(blocks, rows - 1);
    if pages.is_empty() {
        pages.push(Vec::new());
    }

    let mut page_of: HashMap<(usize, usize), usize> = HashMap::new();
    for (page_index, page) in pages.iter().enumerate() {
        for entry in page {
            if let PageEntry::Text { block, line } = entry {
                page_of.insert((*block, *line), page_index);
            }
        }
    }
    let mut heading_pages: Vec<(HeadingMark, usize)> = marks
        .iter()
        .map(|mark| {
            let page = page_of.get(&(mark.block, mark.line)).copied().unwrap_or(0);
            (mark.clone(), page)
        })
        .collect();

    let mut index = 0usize;
    let mut mode = Mode::Page;
    let mut toc_index = 0usize;

    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    let result = (|| -> Result<()> {
        loop {
            match mode {
                Mode::Page => draw_page(
                    &mut stdout,
                    blocks,
                    &pages,
                    index,
                    rows,
                    columns,
                    current_heading(&heading_pages, index),
                )?,
                Mode::Toc => draw_toc(&mut stdout, &heading_pages, toc_index, rows, columns)?,
            }
            stdout.flush()?;

            let event = event::read()?;
            let Event::Key(key) = event else {
                if let Event::Resize(next_columns, next_rows) = event {
                    if next_rows >= 5 {
                        rows = next_rows;
                    }
                    if next_columns >= 20 {
                        columns = next_columns;
                    }
                    let current_block_line = pages
                        .get(index)
                        .and_then(|page| page.first())
                        .and_then(|entry| match entry {
                            PageEntry::Text { block, line } => Some((*block, *line)),
                            PageEntry::Image { .. } => None,
                        });
                    pages = paginate(blocks, rows - 1);
                    if pages.is_empty() {
                        pages.push(Vec::new());
                    }
                    page_of.clear();
                    for (page_index, page) in pages.iter().enumerate() {
                        for entry in page {
                            if let PageEntry::Text { block, line } = entry {
                                page_of.insert((*block, *line), page_index);
                            }
                        }
                    }
                    heading_pages = marks
                        .iter()
                        .map(|mark| {
                            let page = page_of.get(&(mark.block, mark.line)).copied().unwrap_or(0);
                            (mark.clone(), page)
                        })
                        .collect();
                    index = current_block_line
                        .and_then(|key| page_of.get(&key).copied())
                        .unwrap_or_else(|| index.min(pages.len() - 1));
                }
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }

            if mode == Mode::Toc {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        toc_index = (toc_index + 1).min(heading_pages.len().saturating_sub(1));
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        toc_index = toc_index.saturating_sub(1);
                    }
                    KeyCode::Enter => {
                        if let Some((_, page)) = heading_pages.get(toc_index) {
                            index = *page;
                        }
                        mode = Mode::Page;
                    }
                    KeyCode::Char('t') | KeyCode::Char('q') | KeyCode::Esc => {
                        mode = Mode::Page;
                    }
                    _ => {}
                }
                continue;
            }

            if is_quit(&key) {
                break;
            }
            match key.code {
                KeyCode::Char('t') => {
                    mode = Mode::Toc;
                    toc_index = heading_pages
                        .iter()
                        .position(|(_, page)| *page >= index)
                        .unwrap_or(0);
                }
                KeyCode::Char(' ')
                | KeyCode::Enter
                | KeyCode::Char('j')
                | KeyCode::Char('f')
                | KeyCode::Down
                | KeyCode::PageDown => {
                    if index + 1 < pages.len() {
                        index += 1;
                    }
                }
                KeyCode::Char('b') | KeyCode::Char('k') | KeyCode::Up | KeyCode::PageUp => {
                    index = index.saturating_sub(1);
                }
                KeyCode::Char(']') => {
                    if let Some((_, page)) = heading_pages.iter().find(|(_, page)| *page > index) {
                        index = *page;
                    }
                }
                KeyCode::Char('[') => {
                    if let Some((_, page)) =
                        heading_pages.iter().rev().find(|(_, page)| *page < index)
                    {
                        index = *page;
                    }
                }
                KeyCode::Char('g') => index = 0,
                KeyCode::Char('G') => index = pages.len().saturating_sub(1),
                _ => {}
            }
        }
        Ok(())
    })();

    execute!(stdout, Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    result
}

#[derive(PartialEq, Eq)]
enum Mode {
    Page,
    Toc,
}

fn is_quit(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('q')
        || key.code == KeyCode::Char('Q')
        || key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

fn draw_page(
    stdout: &mut impl Write,
    blocks: &mut [Block],
    pages: &[Vec<PageEntry>],
    index: usize,
    rows: u16,
    columns: u16,
    heading: Option<&HeadingMark>,
) -> Result<()> {
    queue!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
    let mut row = 0u16;
    if let Some(page) = pages.get(index) {
        for entry in page {
            match entry {
                PageEntry::Text { block, line } => {
                    if let Some(Block::Text(lines)) = blocks.get(*block) {
                        if let Some(text) = lines.get(*line) {
                            queue!(stdout, MoveTo(0, row))?;
                            stdout.write_all(text.as_bytes())?;
                        }
                    }
                    row = row.saturating_add(1);
                }
                PageEntry::Image { block, rows } => {
                    if let Some(Block::Image { payload, .. }) = blocks.get_mut(*block) {
                        if matches!(payload, ImagePayload::Lazy(_)) {
                            let old = std::mem::replace(payload, ImagePayload::Ready(Vec::new()));
                            if let ImagePayload::Lazy(render) = old {
                                *payload = ImagePayload::Ready(render());
                            }
                        }
                        if let ImagePayload::Ready(bytes) = payload {
                            queue!(stdout, MoveTo(0, row))?;
                            stdout.write_all(bytes)?;
                        }
                    }
                    row = row.saturating_add(*rows);
                }
            }
            if row >= rows.saturating_sub(1) {
                break;
            }
        }
    }

    let label = heading
        .map(|heading| format!("§ {}", strip_ansi(&heading.text)))
        .unwrap_or_default();
    let status = format!(" {}/{}  {}  {}", index + 1, pages.len(), label, HELP);
    queue!(
        stdout,
        MoveTo(0, rows.saturating_sub(1)),
        SetAttribute(Attribute::Reverse)
    )?;
    stdout.write_all(pad_to_width(&status, columns).as_bytes())?;
    queue!(
        stdout,
        SetAttribute(Attribute::NoReverse),
        crossterm::style::ResetColor
    )?;
    Ok(())
}

fn draw_toc(
    stdout: &mut impl Write,
    headings: &[(HeadingMark, usize)],
    toc_index: usize,
    rows: u16,
    columns: u16,
) -> Result<()> {
    queue!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
    stdout.write_all("見出し一覧  (j/k: 移動, Enter: ジャンプ, t/Esc: 戻る)\r\n".as_bytes())?;
    let visible = rows.saturating_sub(2) as usize;
    let offset = toc_index.saturating_sub(visible / 2);
    for i in 0..visible {
        let position = offset + i;
        let Some((heading, page)) = headings.get(position) else {
            break;
        };
        let indent = "  ".repeat(heading.level.saturating_sub(1));
        let page_label = format!("p.{}", page + 1);
        let text = format!(
            " {:>3} {}{}",
            heading.index,
            indent,
            strip_ansi(&heading.text)
        );
        let line = format!(
            "{}{} ",
            pad_to_width(&text, columns.saturating_sub(page_label.len() as u16 + 1)),
            page_label
        );
        queue!(stdout, MoveTo(0, i as u16 + 1))?;
        if position == toc_index {
            queue!(stdout, SetAttribute(Attribute::Reverse))?;
            stdout.write_all(pad_to_width(&line, columns).as_bytes())?;
            queue!(stdout, SetAttribute(Attribute::NoReverse))?;
        } else {
            stdout.write_all(line.as_bytes())?;
        }
    }
    queue!(stdout, MoveTo(0, rows.saturating_sub(1)))?;
    stdout.write_all(pad_to_width("", columns).as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_heading_lines() {
        assert!(is_heading_line("┄ Title"));
        assert!(is_heading_line("\x1b[1m┄\x1b[0m Title"));
        assert!(!is_heading_line("normal"));
    }

    #[test]
    fn paginates_text_and_images() {
        let blocks = vec![
            Block::Text(vec!["a".into(), "b".into(), "c".into()]),
            Block::Image {
                rows: 3,
                payload: ImagePayload::Ready(vec![]),
            },
            Block::Text(vec!["d".into()]),
        ];
        let pages = paginate(&blocks, 2);
        assert_eq!(pages.len(), 4);
        assert_eq!(pages[0].len(), 2);
        assert!(matches!(pages[1][0], PageEntry::Text { .. }));
        assert!(matches!(pages[2][0], PageEntry::Image { rows: 2, .. }));
        assert!(matches!(pages[3][0], PageEntry::Text { .. }));
    }

    #[test]
    fn truncates_and_pads_to_terminal_width() {
        assert_eq!(display_width(&pad_to_width("abc", 5)), 5);
        assert!(display_width(&truncate_to_width("abcdef", 4)) <= 4);
    }
}
