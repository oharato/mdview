//! mdview-rs: a single Rust binary Markdown viewer.
//!
//! Text rendering uses `pulldown-cmark-mdcat` (the library behind mdcat),
//! Mermaid uses `mermaid-rs-renderer`, and images are drawn with Kitty,
//! Sixel, iTerm2, or ANSI half-blocks.

mod blocks;
mod images;
mod iterm;
mod kitty;
mod markdown;
mod mermaid;
mod pager;
mod paths;
mod remote;
mod sections;
mod selftest;
mod sixel;
mod tables;
mod term;
mod text;
mod update;

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};

use blocks::ColorMode;
use images::ImageRendererKind;
use markdown::{ImageBlock, MermaidBlock, Segment};
use mermaid::MermaidTheme;
use pager::BufferSink;
use term::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Renderer {
    Auto,
    Kitty,
    Sixel,
    Iterm,
    Blocks,
}

impl Renderer {
    fn image_kind(self) -> ImageRendererKind {
        match self {
            Self::Kitty => ImageRendererKind::Kitty,
            Self::Sixel => ImageRendererKind::Sixel,
            Self::Iterm => ImageRendererKind::Iterm,
            Self::Blocks | Self::Auto => ImageRendererKind::Blocks,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "mdview",
    version = update::current_version(),
    about = "mdcat + inline images and Mermaid in the terminal (Rust single binary)",
    disable_help_subcommand = true
)]
struct Args {
    /// Markdown files. Reads stdin when omitted or when FILE is `-`.
    #[arg(value_name = "FILE")]
    files: Vec<PathBuf>,

    /// Text width in columns (default: terminal width, max 120).
    #[arg(short = 'w', long)]
    width: Option<u16>,

    /// Image/diagram width in columns (default: terminal width - 2).
    #[arg(short = 'i', long = "image-width")]
    image_width: Option<u32>,

    /// Maximum image height in terminal rows (default: 2/3 of the terminal).
    #[arg(long = "image-max-height", default_value_t = 0)]
    image_max_height: u16,

    /// Rendering mode.
    #[arg(short = 'r', long, value_enum, default_value_t = Renderer::Auto)]
    renderer: Renderer,

    /// Colour count for half-block rendering.
    #[arg(long, value_enum, default_value_t = ColorMode::Truecolor)]
    colours: ColorMode,

    /// Colour count for Sixel.
    #[arg(long = "image-colours", default_value_t = 256)]
    image_colours: usize,

    /// Transparent image background: auto | none | #RRGGBB.
    #[arg(long = "image-bg", default_value = "auto")]
    image_bg: String,

    /// Multiply the cell size when the terminal cannot report it (HiDPI: 2).
    #[arg(long = "pixel-scale", default_value_t = 1.0)]
    pixel_scale: f32,

    /// Upscale small images to the requested width.
    #[arg(long)]
    upscale: bool,

    /// Disable upscaling.
    #[arg(long = "no-upscale")]
    no_upscale: bool,

    /// Treat the terminal background as light.
    #[arg(long)]
    light: bool,

    /// Disable colours and images.
    #[arg(short = 'c', long = "no-colour")]
    no_colour: bool,

    /// Mermaid colour theme.
    #[arg(long = "mermaid-theme", value_enum, default_value_t = MermaidTheme::Auto)]
    mermaid_theme: MermaidTheme,

    /// Mermaid rasterization width in pixels.
    #[arg(long = "mermaid-width", default_value_t = 2048)]
    mermaid_width: u32,

    /// Use the built-in pager.
    #[arg(short = 'p', long)]
    paginate: bool,

    /// List headings and exit.
    #[arg(long)]
    toc: bool,

    /// Show only the section after heading N.
    #[arg(long)]
    section: Option<usize>,

    /// Print terminal and build capabilities and exit.
    #[arg(long)]
    check: bool,

    /// Re-probe the terminal, ignoring the capability cache.
    #[arg(long)]
    refresh: bool,

    /// Check for a newer release.
    #[arg(long = "check-update")]
    check_update: bool,

    /// Update this binary from GitHub Releases (Linux).
    #[arg(long)]
    update: bool,

    /// Debug: render the whole document as text and exit.
    #[arg(long, hide = true)]
    dump_text: bool,
}

#[derive(Clone)]
struct Options {
    width: u16,
    image_width: u32,
    image_max_height: u16,
    renderer: Renderer,
    color_mode: ColorMode,
    image_colors: usize,
    image_bg: String,
    pixel_scale: f32,
    upscale: bool,
    light: bool,
    no_colour: bool,
    mermaid_theme: MermaidTheme,
    mermaid_width: u32,
    refresh: bool,
}

impl Options {
    fn resolve(args: &Args) -> Self {
        let (columns, _) = term::terminal_size();
        let width = args.width.unwrap_or_else(|| columns.min(120)).clamp(1, 400);
        let image_width = args
            .image_width
            .unwrap_or_else(|| columns.saturating_sub(2).clamp(10, 200) as u32)
            .clamp(1, 400);
        let upscale = args.upscale || !args.no_upscale;
        Self {
            width,
            image_width,
            image_max_height: args.image_max_height,
            renderer: args.renderer,
            color_mode: args.colours,
            image_colors: args.image_colours.clamp(2, 256),
            image_bg: args.image_bg.clone(),
            pixel_scale: if args.pixel_scale > 0.0 {
                args.pixel_scale
            } else {
                1.0
            },
            upscale,
            light: args.light || detect_light_background(),
            no_colour: args.no_colour,
            mermaid_theme: args.mermaid_theme,
            mermaid_width: args.mermaid_width.max(32),
            refresh: args.refresh,
        }
    }
}

fn detect_light_background() -> bool {
    std::env::var("COLORFGBG")
        .ok()
        .and_then(|value| value.split(';').next_back().map(str::to_string))
        .and_then(|value| value.parse::<u32>().ok())
        .map(|value| value >= 8)
        .unwrap_or(false)
}

type LazyImage = Box<dyn FnOnce() -> Vec<u8>>;

trait Sink {
    fn text(&mut self, body: &str) -> Result<()>;
    fn image(&mut self, rows: u16, payload: Vec<u8>) -> Result<()>;
    fn lazy_image(&mut self, rows: u16, render: LazyImage) -> Result<()>;
}

struct StreamSink;

impl Sink for StreamSink {
    fn text(&mut self, body: &str) -> Result<()> {
        write_out(body.as_bytes())
    }

    fn image(&mut self, _rows: u16, payload: Vec<u8>) -> Result<()> {
        write_out(&payload)
    }

    fn lazy_image(&mut self, _rows: u16, render: LazyImage) -> Result<()> {
        write_out(&render())
    }
}

struct PagerSink(BufferSink);

impl Sink for PagerSink {
    fn text(&mut self, body: &str) -> Result<()> {
        self.0.push_text(body);
        Ok(())
    }

    fn image(&mut self, rows: u16, payload: Vec<u8>) -> Result<()> {
        self.0.push_image(rows, payload);
        Ok(())
    }

    fn lazy_image(&mut self, rows: u16, render: LazyImage) -> Result<()> {
        self.0.push_lazy_image(rows, render);
        Ok(())
    }
}

fn write_out(data: &[u8]) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    match stdout.write_all(data) {
        Ok(()) => {
            stdout.flush()?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {
            std::process::exit(0);
        }
        Err(error) => Err(error.into()),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("mdview: {message}");
    std::process::exit(1);
}

fn read_document(path: Option<&Path>) -> Result<(String, PathBuf, String)> {
    match path {
        None => read_stdin(),
        Some(path) if path == Path::new("-") => read_stdin(),
        Some(path) => {
            let absolute = std::fs::canonicalize(path)
                .with_context(|| format!("ファイルがありません: {}", path.display()))?;
            let base_dir = absolute
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            let markdown = std::fs::read_to_string(&absolute)
                .with_context(|| format!("読み込めません: {}", absolute.display()))?;
            Ok((markdown, base_dir, absolute.display().to_string()))
        }
    }
}

fn read_stdin() -> Result<(String, PathBuf, String)> {
    let mut markdown = String::new();
    std::io::stdin()
        .read_to_string(&mut markdown)
        .context("標準入力を読み込めません")?;
    let cwd = std::env::current_dir().context("カレントディレクトリを取得できません")?;
    Ok((markdown, cwd, "<stdin>".to_string()))
}

static WARNED_KITTY_TRANSFER: AtomicBool = AtomicBool::new(false);
static KITTY_IMAGE_ID: AtomicU32 = AtomicU32::new(1000);

fn next_kitty_image_id() -> u32 {
    KITTY_IMAGE_ID.fetch_add(1, Ordering::Relaxed)
}

fn choose_renderer(options: &Options) -> Renderer {
    if options.renderer != Renderer::Auto {
        return options.renderer;
    }
    if term::kitty_supported() {
        return Renderer::Kitty;
    }

    let probe = term::probe_terminal();
    if !options.no_colour
        && probe.kitty == Some(true)
        && probe.kitty_transfer == Some(false)
        && term::is_wez_term()
        && !WARNED_KITTY_TRANSFER.swap(true, Ordering::Relaxed)
    {
        warn(
            "kitty graphics の画像転送を確認できませんでした（-r blocks で半ブロック表示になります）。\
             接続経路によっては描画されないことがあります: wezterm ssh / -r iterm もお試しください",
        );
    }

    if term::is_wez_term() {
        return if iterm::detect_iterm2() {
            Renderer::Iterm
        } else {
            Renderer::Blocks
        };
    }
    if term::sixel_supported(options.refresh) {
        return Renderer::Sixel;
    }
    if iterm::detect_iterm2() {
        return Renderer::Iterm;
    }
    Renderer::Blocks
}

/// Rasterization width for Mermaid / SVG.
///
/// Rendering a 2048px PNG for half-blocks that are only ~100 cells wide is
/// wasteful: decode + resize then dominates the whole run. Use roughly the
/// final display resolution instead, keeping `--mermaid-width` as an upper
/// bound for users who really want supersampling.
fn mermaid_raster_width(options: &Options) -> u32 {
    let renderer = choose_renderer(options);
    let desired = if renderer == Renderer::Blocks {
        options.image_width.saturating_mul(4)
    } else {
        let cell_width = term::cell_pixels().0.max(1);
        (options.image_width as f32 * cell_width as f32 * options.pixel_scale.max(1.0)) as u32
    };
    desired.max(256).min(options.mermaid_width.max(256))
}

fn print_toc(path: Option<&Path>, show_name: bool) -> Result<()> {
    let (markdown, _, _) = read_document(path)?;
    let headings = sections::list_headings(&markdown);
    if show_name {
        let name = path
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<stdin>".to_string());
        println!("\n{name}");
    }
    if headings.is_empty() {
        println!("(見出しがありません)");
        return Ok(());
    }
    for heading in headings {
        println!(
            "{:>3}  {}{}",
            heading.index,
            "  ".repeat(heading.level.saturating_sub(1)),
            heading.text
        );
    }
    Ok(())
}

fn print_check(options: &Options) {
    let probe = term::probe_terminal();
    let sixel = term::sixel_supported(options.refresh);
    let kitty = term::kitty_supported();
    let (cell_width, cell_height) = term::cell_pixels();
    let window = term::window_pixels();
    let renderer = choose_renderer(options);
    let (columns, _) = term::terminal_size();

    let lines: [(String, String); 19] = [
        ("mdview".into(), update::current_version().into()),
        ("rust".into(), "built-in (no Node/Python runtime)".into()),
        (
            "mdcat".into(),
            "built-in (pulldown-cmark-mdcat 2.9.0)".into(),
        ),
        (
            "mermaid".into(),
            "built-in (mermaid-rs-renderer 0.3.1)".into(),
        ),
        ("resvg".into(), "built-in (0.49)".into()),
        ("TERM".into(), std::env::var("TERM").unwrap_or_default()),
        (
            "terminal".into(),
            probe
                .terminal
                .map(|(name, _)| name)
                .unwrap_or_else(|| "不明 (XTVERSION 未応答)".into()),
        ),
        ("columns".into(), columns.to_string()),
        ("cell pixels".into(), format!("{cell_width}x{cell_height}")),
        (
            "window pixels".into(),
            window
                .map(|(width, height)| format!("{width}x{height}"))
                .unwrap_or_else(|| "不明".into()),
        ),
        (
            "background".into(),
            probe
                .background
                .map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}"))
                .unwrap_or_else(|| "不明".into()),
        ),
        ("interactive".into(), term::is_interactive().to_string()),
        (
            "kitty support".into(),
            match probe.kitty {
                None => "no response".into(),
                Some(true) => "yes".into(),
                Some(false) => "no".into(),
            },
        ),
        (
            "kitty transfer".into(),
            term::kitty_transfer_status().into(),
        ),
        ("kitty usable".into(), yes_no(kitty)),
        ("sixel support".into(), yes_no(sixel)),
        ("iterm2 support".into(), yes_no(iterm::detect_iterm2())),
        ("renderer".into(), renderer_name(renderer).to_string()),
        (
            "herdr".into(),
            if std::env::var_os("HERDR_ENV").is_some() {
                "yes (HERDR_ENV=1)".into()
            } else {
                "no".into()
            },
        ),
    ];
    for (label, value) in &lines {
        println!("{:<14}: {}", label, value);
    }
    println!("{:<14}: {} cols", "image width", options.image_width);

    if std::env::var_os("HERDR_ENV").is_some() && renderer == Renderer::Kitty {
        println!(
            "\nhint: Herdr 経由の Kitty graphics は Herdr が外側端末向けに再エンコードします。\n\
             \x20     画像が出ない場合は Herdr の client / server 両方で kitty_graphics を有効にし、\n\
             \x20     [terminal] kitty_graphics = true（旧: [experimental] kitty_graphics = true）を\n\
             \x20     確認してください。切り分けには mdview -r blocks が使えます。"
        );
    }

    if probe.kitty == Some(true) && probe.kitty_transfer == Some(false) && term::is_wez_term() {
        println!(
            "\nhint: 端末は kitty graphics を話しますが、画像データの転送を確認できませんでした。\n\
             \x20     - wezterm ssh <user>@<host>  で接続し直す（ConPTY を迂回）\n\
             \x20     - mdview -r iterm <file>     OSC 1337 経由を試す\n\
             \x20     - mdview -r blocks <file>    半ブロックで表示（現状のフォールバック）"
        );
    }
}

fn yes_no(value: bool) -> String {
    if value { "yes" } else { "no" }.to_string()
}

fn renderer_name(renderer: Renderer) -> &'static str {
    match renderer {
        Renderer::Auto => "auto",
        Renderer::Kitty => "kitty",
        Renderer::Sixel => "sixel",
        Renderer::Iterm => "iterm",
        Renderer::Blocks => "blocks",
    }
}

fn process_document(
    text: &str,
    base_dir: &Path,
    options: &Options,
    sink: &mut dyn Sink,
    page_mode: bool,
) -> Result<()> {
    let (text, mermaid_blocks) = markdown::extract_mermaid(text);
    let (text, image_blocks) = markdown::extract_images(&text, mermaid_blocks.len());
    let (text, table_blocks) = tables::extract_tables(&text);

    enum Asset {
        Mermaid(MermaidBlock),
        Image(ImageBlock),
    }
    let mut assets = Vec::with_capacity(mermaid_blocks.len() + image_blocks.len());
    assets.extend(mermaid_blocks.into_iter().map(Asset::Mermaid));
    assets.extend(image_blocks.into_iter().map(Asset::Image));

    for segment in markdown::split_segments(&text) {
        match segment {
            Segment::Text(body) => {
                let started = Instant::now();
                let rendered = text::render(
                    &body,
                    options.width,
                    term::terminal_size().1,
                    !options.no_colour,
                    base_dir,
                )?;
                sink.text(&rendered)?;
                if std::env::var_os("MDVIEW_DEBUG").is_some() {
                    eprintln!(
                        "[text] {} chars -> {:.1} ms",
                        body.len(),
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                }
            }
            Segment::Asset(index) => match assets.get(index) {
                Some(Asset::Mermaid(block)) => {
                    render_mermaid(block, options, sink, base_dir, page_mode)?
                }
                Some(Asset::Image(block)) => {
                    render_image(block, options, sink, base_dir, page_mode)?
                }
                None => {}
            },
            Segment::Table(index) => {
                if let Some(table) = table_blocks.get(index) {
                    sink.text(&tables::render_table(
                        table,
                        options.width as usize,
                        !options.no_colour,
                    ))?;
                    sink.text("\n")?;
                }
            }
        }
    }
    Ok(())
}

fn render_mermaid(
    block: &MermaidBlock,
    options: &Options,
    sink: &mut dyn Sink,
    base_dir: &Path,
    page_mode: bool,
) -> Result<()> {
    if options.no_colour {
        sink.text("[画像: mermaid]\n")?;
        return Ok(());
    }

    let started = Instant::now();
    let theme = options.mermaid_theme.resolve(options.light);
    let cache_width = mermaid_raster_width(options);
    match mermaid::load_cached(&block.code, theme, cache_width) {
        Ok(image) => {
            if std::env::var_os("MDVIEW_DEBUG").is_some() {
                eprintln!(
                    "[mermaid] {:.1} ms",
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
            if page_mode {
                if let Some((rows, render)) = defer_image(&image, "mermaid", options) {
                    return sink.lazy_image(rows, render);
                }
            }
            draw_image(&image, "mermaid", options, sink, page_mode)
        }
        Err(error) => {
            warn(&format!("Mermaid を描画できません: {error}"));
            let fallback = format!("```mermaid\n{}\n```\n", block.code.trim_end());
            let rendered = text::render(
                &fallback,
                options.width,
                term::terminal_size().1,
                !options.no_colour,
                base_dir,
            )?;
            sink.text(&rendered)
        }
    }
}

fn render_image(
    block: &ImageBlock,
    options: &Options,
    sink: &mut dyn Sink,
    base_dir: &Path,
    page_mode: bool,
) -> Result<()> {
    if options.no_colour {
        sink.text(&format!(
            "[画像: {}]\n",
            if block.alt.is_empty() {
                &block.src
            } else {
                &block.alt
            }
        ))?;
        return Ok(());
    }

    let Some(path) = remote::resolve_image_source(&block.src, base_dir) else {
        sink.text(&format!(
            "\x1b[2m[画像: {}]\x1b[0m\n",
            if block.alt.is_empty() {
                &block.src
            } else {
                &block.alt
            }
        ))?;
        return Ok(());
    };

    let started = Instant::now();
    let image = if images::is_svg(&path) {
        images::rasterize_svg_file(&path, mermaid_raster_width(options))?
    } else {
        images::load_raster(&path)?
    };
    if std::env::var_os("MDVIEW_DEBUG").is_some() {
        eprintln!(
            "[image] {} {}x{} -> {:.1} ms",
            path.display(),
            image.width(),
            image.height(),
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    if page_mode {
        if let Some((rows, render)) = defer_image(&image, &block.alt, options) {
            return sink.lazy_image(rows, render);
        }
    }
    draw_image(&image, &block.alt, options, sink, page_mode)
}

#[derive(Debug, Clone, Copy)]
struct ProtocolPlan {
    kind: ImageRendererKind,
    width: u32,
    height: u32,
    rows: u16,
    cell: (u32, u32),
}

fn plan_protocol_image(
    image: &image::RgbaImage,
    options: &Options,
    renderer: Renderer,
) -> Option<ProtocolPlan> {
    let kind = renderer.image_kind();
    if !matches!(
        kind,
        ImageRendererKind::Kitty | ImageRendererKind::Sixel | ImageRendererKind::Iterm
    ) {
        return None;
    }

    let (_, rows) = term::terminal_size();
    let cell = term::cell_pixels();
    let window = term::window_pixels();
    let max_rows = images::max_image_rows(rows, options.image_max_height);
    let (width, height) = images::plan_display_size(
        image.width(),
        image.height(),
        options.image_width,
        cell,
        window,
        options.pixel_scale,
        options.upscale,
        max_rows,
    );
    let rows_cells = ((height as f64) / cell.1.max(1) as f64).ceil().max(1.0) as u16;
    Some(ProtocolPlan {
        kind,
        width,
        height,
        rows: rows_cells,
        cell,
    })
}

fn encode_protocol_payload(
    resized: &image::RgbaImage,
    options: &Options,
    plan: ProtocolPlan,
    alt: &str,
    page_mode: bool,
) -> Vec<u8> {
    match plan.kind {
        ImageRendererKind::Sixel => {
            let mut payload = sixel::encode_sixel(resized, options.image_colors);
            if !page_mode {
                payload.push(b'\n');
            }
            payload
        }
        ImageRendererKind::Kitty => {
            let png = images::to_png(resized, image::codecs::png::CompressionType::Fast)
                .expect("PNG encoding cannot fail in memory");
            let cols_cells = ((resized.width() as f64) / plan.cell.0.max(1) as f64)
                .round()
                .max(1.0) as u16;
            let encoded = kitty::encode_kitty(
                &png,
                kitty::KittyPlacement {
                    cols: Some(cols_cells as u32),
                    rows: Some(plan.rows as u32),
                    image_id: Some(next_kitty_image_id()),
                    hold_cursor: true,
                },
            );
            if page_mode {
                encoded
            } else {
                let mut payload = Vec::new();
                payload.extend_from_slice(b"\r");
                payload.extend_from_slice("\n".repeat(plan.rows as usize).as_bytes());
                payload.extend_from_slice(format!("\x1b[{}A", plan.rows).as_bytes());
                payload.extend_from_slice(&encoded);
                payload.extend_from_slice(format!("\x1b[{}B", plan.rows).as_bytes());
                payload
            }
        }
        ImageRendererKind::Iterm => iterm::encode_iterm2(
            &images::to_png(resized, image::codecs::png::CompressionType::Fast)
                .expect("PNG encoding cannot fail in memory"),
            iterm::Iterm2Options {
                name: Some(alt),
                width_px: Some(resized.width()),
                height_px: Some(resized.height()),
            },
        ),
        ImageRendererKind::Blocks => unreachable!(),
    }
}

/// Prepare a pager image without encoding its payload yet. The payload is
/// produced only when the page containing the image is first drawn.
fn defer_image(image: &image::RgbaImage, alt: &str, options: &Options) -> Option<(u16, LazyImage)> {
    let renderer = choose_renderer(options);
    let plan = plan_protocol_image(image, options, renderer)?;
    let detected_background = term::terminal_background();
    let image = match images::resolve_background(
        options.image_bg.as_str(),
        options.light,
        plan.kind,
        detected_background,
    ) {
        Some(background) => images::flatten_image(image, background),
        None => image.clone(),
    };
    let options = options.clone();
    let alt = alt.to_string();
    let width = plan.width;
    let height = plan.height;
    Some((
        plan.rows,
        Box::new(move || {
            let resized = images::resize_image(&image, width, height);
            encode_protocol_payload(&resized, &options, plan, &alt, true)
        }),
    ))
}

fn draw_image(
    image: &image::RgbaImage,
    alt: &str,
    options: &Options,
    sink: &mut dyn Sink,
    page_mode: bool,
) -> Result<()> {
    let renderer = choose_renderer(options);
    let detected_background = term::terminal_background();
    let image = match images::resolve_background(
        options.image_bg.as_str(),
        options.light,
        renderer.image_kind(),
        detected_background,
    ) {
        Some(background) => images::flatten_image(image, background),
        None => image.clone(),
    };

    if let Some(plan) = plan_protocol_image(&image, options, renderer) {
        let resized = images::resize_image(&image, plan.width, plan.height);
        let payload = encode_protocol_payload(&resized, options, plan, alt, page_mode);
        sink.image(plan.rows, payload)?;
    } else {
        let (_, rows) = term::terminal_size();
        let max_rows = images::max_image_rows(rows, options.image_max_height);
        let plan =
            blocks::plan_half_blocks(image.width(), image.height(), options.image_width, max_rows);
        let resized = images::resize_image(&image, plan.cols, plan.rows * 2);
        let block_background = images::blend_background(
            options.image_bg.as_str(),
            options.light,
            detected_background,
        );
        let rendered = blocks::render_half_blocks(
            &resized,
            options.image_width,
            max_rows,
            options.color_mode,
            block_background,
        );
        sink.text(&rendered)?;
    }
    Ok(())
}

#[cfg(unix)]
fn reset_sigpipe() {
    // Rust ignores SIGPIPE by default, which turns `mdview ... | head` into a
    // panic. Restore the Unix default so the process terminates quietly.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

fn main() -> Result<()> {
    #[cfg(unix)]
    reset_sigpipe();

    let args = Args::parse();

    if args.check_update {
        let check = update::check_for_update();
        println!("mdview        : {}", check.current);
        println!(
            "install       : {}",
            update::describe_install(&check.install)
        );
        match &check.latest {
            Some(latest) => {
                println!("latest        : {}", latest.version);
                println!(
                    "status        : {}",
                    if check.has_update {
                        "更新があります"
                    } else {
                        "最新です"
                    }
                );
                if check.has_update {
                    println!("\n  mdview --update  で更新できます ({})", latest.url);
                }
            }
            None => println!(
                "latest        : 取得できませんでした（リリース未作成、またはネットワーク）"
            ),
        }
        return Ok(());
    }

    if args.update {
        let message = update::run_self_update()?;
        println!("{message}");
        return Ok(());
    }

    let options = Options::resolve(&args);

    if args.dump_text {
        let file = args.files.first().map(PathBuf::as_path);
        let (markdown, base_dir, _) = read_document(file)?;
        let rendered = text::render(
            &markdown,
            options.width,
            term::terminal_size().1,
            !options.no_colour,
            &base_dir,
        )?;
        print!("{rendered}");
        return Ok(());
    }

    if args.check {
        print_check(&options);
        return Ok(());
    }

    if args.toc {
        let show_name = args.files.len() > 1;
        for file in &args.files {
            print_toc(Some(file), show_name)?;
        }
        if args.files.is_empty() {
            print_toc(None, false)?;
        }
        return Ok(());
    }

    if args.section == Some(0) {
        fail("--section には 1 以上の番号を指定してください（--toc で一覧を表示できます）");
    }

    if term::is_interactive() && !options.no_colour {
        let _ = choose_renderer(&options);
    }

    let use_pager = args.paginate && term::is_interactive() && std::io::stdin().is_terminal();

    let mut sink = if use_pager {
        SinkKind::Pager(PagerSink(BufferSink::default()))
    } else {
        SinkKind::Stream(StreamSink)
    };

    let files: Vec<Option<PathBuf>> = if args.files.is_empty() {
        vec![None]
    } else {
        args.files.iter().cloned().map(Some).collect()
    };

    for file in files {
        let (markdown, base_dir, _) = read_document(file.as_deref())?;
        let markdown = if let Some(section) = args.section {
            sections::extract_section(&markdown, section).unwrap_or_else(|| {
                fail(&format!(
                    "セクション {section} が見つかりません（--toc で一覧を表示できます）"
                ))
            })
        } else {
            markdown
        };

        if let SinkKind::Pager(pager_sink) = &mut sink {
            // The pager needs headings before the document is rendered so that
            // mdcat heading lines can be matched in order.
            pager_sink
                .0
                .expect_headings(sections::list_headings(&markdown));
        }
        process_document(&markdown, &base_dir, &options, &mut sink, use_pager)?;
    }

    if let SinkKind::Pager(mut pager_sink) = sink {
        let (columns, rows) = term::terminal_size();
        pager::run_pager(&mut pager_sink.0.blocks, &pager_sink.0.marks, rows, columns)?;
    }

    Ok(())
}

enum SinkKind {
    Stream(StreamSink),
    Pager(PagerSink),
}

impl Sink for SinkKind {
    fn text(&mut self, body: &str) -> Result<()> {
        match self {
            Self::Stream(sink) => sink.text(body),
            Self::Pager(sink) => sink.text(body),
        }
    }

    fn image(&mut self, rows: u16, payload: Vec<u8>) -> Result<()> {
        match self {
            Self::Stream(sink) => sink.image(rows, payload),
            Self::Pager(sink) => sink.image(rows, payload),
        }
    }

    fn lazy_image(&mut self, rows: u16, render: LazyImage) -> Result<()> {
        match self {
            Self::Stream(sink) => sink.lazy_image(rows, render),
            Self::Pager(sink) => sink.lazy_image(rows, render),
        }
    }
}
