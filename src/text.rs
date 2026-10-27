//! Text rendering through `pulldown-cmark-mdcat`, the library mdcat itself uses.
//!
//! This is the key parity check for the Rust port: the PoC should produce the
//! same ANSI text as the pinned mdcat binary.

use std::path::Path;
use std::sync::OnceLock;

use anyhow::Result;
use pulldown_cmark::{Options, Parser};
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
    let parser = Parser::new_ext(markdown, options);

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
