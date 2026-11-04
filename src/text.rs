//! Text rendering through `pulldown-cmark-mdcat`, the library mdcat itself uses.
//!
//! Headings use Markdown's `#` prefixes in bold; other text keeps mdcat styling.

use std::path::Path;
use std::sync::OnceLock;

use anyhow::Result;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use pulldown_cmark_mdcat::resources::NoopResourceHandler;
use pulldown_cmark_mdcat::{push_tty, Environment, Settings, TerminalProgram, TerminalSize, Theme};
use syntect::parsing::SyntaxSet;

/// Render a Markdown fragment as terminal text.
pub fn render(
    markdown: &str,
    columns: u16,
    rows: u16,
    ansi: bool,
    base_dir: &Path,
) -> Result<String> {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    let syntax_set = SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines);
    let terminal = if ansi {
        TerminalProgram::Ansi
    } else {
        TerminalProgram::Dumb
    };
    let settings = Settings {
        terminal_capabilities: terminal.capabilities(),
        terminal_size: TerminalSize {
            columns,
            rows,
            pixels: None,
            cell: None,
        },
        syntax_set,
        theme: Theme::default(),
    };

    // mdcat 2.7.1 uses these three extensions.  Keep the same set so the
    // output stays comparable to the currently pinned binary.
    let options =
        Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(markdown, options).flat_map(|event| match event {
        Event::Start(Tag::Heading { level, .. }) => vec![
            Event::Start(Tag::Paragraph),
            Event::Start(Tag::Strong),
            Event::Text(format!("{} ", "#".repeat(level as usize)).into()),
        ],
        Event::End(TagEnd::Heading(_)) => {
            vec![Event::End(TagEnd::Strong), Event::End(TagEnd::Paragraph)]
        }
        event => vec![event],
    });

    let environment = Environment::for_local_directory(&base_dir)?;
    let mut output = Vec::new();
    push_tty(
        &settings,
        &environment,
        &NoopResourceHandler,
        &mut output,
        parser,
    )?;
    Ok(String::from_utf8_lossy(&output).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_show_their_level_in_bold_without_changing_code() {
        let markdown = "# Title\n\n## Section\n\n### Detail\n\n#### Four\n\n##### Five\n\n###### Six\n\n```text\n# code\n┄ literal\n```\n";
        for ansi in [false, true] {
            let output = render(
                markdown,
                80,
                24,
                ansi,
                Path::new(env!("CARGO_MANIFEST_DIR")),
            )
            .unwrap();
            let plain = crate::pager::strip_ansi(&output);
            for heading in [
                "# Title",
                "## Section",
                "### Detail",
                "#### Four",
                "##### Five",
                "###### Six",
            ] {
                assert!(plain.lines().any(|line| line == heading), "{plain}");
                if ansi {
                    let joined = output.replace("\x1b[0m\x1b[1m", "");
                    assert!(joined.contains(&format!("\x1b[1m{heading}\x1b[0m")));
                }
            }
            assert!(plain.contains("# code"));
            assert!(plain.contains("┄ literal"));
            if !ansi {
                assert!(!output.contains('\x1b'));
            }
        }
    }
}
