//! Mermaid -> SVG -> PNG pipeline using the pure-Rust renderer.
//!
//! mdview currently uses Pele (JavaScript). The Rust port swaps that for
//! `mermaid-rs-renderer`. The theme background is forced transparent so the
//! terminal background shows through.

use std::path::PathBuf;

use anyhow::{Context, Result};
use image::RgbaImage;
use mermaid_rs_renderer::{render_with_options, RenderOptions, Theme};

use crate::images;
use crate::paths::{ensure_dir, mermaid_cache_dir, short_hash};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum MermaidTheme {
    /// Follow the terminal background (default).
    Auto,
    Dark,
    Default,
    /// Light neutral theme (mapped to the modern palette).
    Neutral,
    Modern,
}

impl MermaidTheme {
    /// Resolve `auto` from the terminal background.
    pub fn resolve(self, light: bool) -> Self {
        match self {
            Self::Auto => {
                if light {
                    Self::Default
                } else {
                    Self::Dark
                }
            }
            other => other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dark => "dark",
            Self::Default => "default",
            Self::Neutral | Self::Modern => "modern",
        }
    }

    fn theme(self) -> Theme {
        let mut theme = match self.resolve(false) {
            Self::Dark => Theme::dark(),
            Self::Default => Theme::mermaid_default(),
            Self::Neutral | Self::Modern | Self::Auto => Theme::modern(),
        };

        // The built-in Mermaid dark palette contains near-black pie slices
        // (the first one is #0b0000), which disappear on a dark terminal.
        if self.resolve(false) == Self::Dark {
            theme.pie_colors = [
                "#4e79a7", "#f28e2b", "#e15759", "#76b7b2", "#59a14f", "#edc948", "#b07aa1",
                "#ff9da7", "#9c755f", "#bab0ac", "#86bcb6", "#f1ce63",
            ]
            .map(str::to_string);
        }
        theme
    }
}

/// Render Mermaid source to SVG.
pub fn render_svg(code: &str, theme: MermaidTheme) -> Result<String> {
    let mut options = RenderOptions {
        theme: theme.theme(),
        ..RenderOptions::default()
    };
    options.theme.background = "rgba(0,0,0,0)".to_string();
    options.theme.font_family =
        "'Noto Sans CJK JP', 'Noto Sans CJK SC', 'DejaVu Sans', sans-serif".to_string();

    render_with_options(code, options).with_context(|| {
        let first = code.lines().next().unwrap_or("<empty>");
        format!("Mermaid render failed: {first}")
    })
}

/// Render Mermaid source to an RGBA image at `width` pixels.
pub fn render_png(code: &str, theme: MermaidTheme, width: u32) -> Result<RgbaImage> {
    let svg = render_svg(code, theme)?;
    images::rasterize_svg(&svg, width)
}

const CACHE_VERSION: &str = "2";

pub fn cache_path(code: &str, theme: MermaidTheme, width: u32) -> PathBuf {
    let key = short_hash(&[CACHE_VERSION, code, theme.as_str(), &width.to_string()]);
    mermaid_cache_dir().join(format!("{key}.png"))
}

/// Return a cached PNG path, rendering it first if necessary.
pub fn render_cached(code: &str, theme: MermaidTheme, width: u32) -> Result<PathBuf> {
    let path = cache_path(code, theme, width);
    if path.exists() {
        return Ok(path);
    }
    ensure_dir(&mermaid_cache_dir())?;
    let image = render_png(code, theme, width)?;
    images::save_png(&image, &path)?;
    Ok(path)
}

/// Load a cached PNG, or render and cache it.
pub fn load_cached(code: &str, theme: MermaidTheme, width: u32) -> Result<RgbaImage> {
    let path = render_cached(code, theme, width)?;
    images::from_png(&std::fs::read(&path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_flowchart_svg() {
        let svg = render_svg("flowchart LR\n  A --> B\n", MermaidTheme::Dark).expect("svg");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("rgba(0,0,0,0)") || !svg.contains("background:#333"));
    }

    #[test]
    fn sequence_labels_stay_on_one_line_and_lifelines_are_visible() {
        let code = "sequenceDiagram\n  participant U as ユーザー\n  participant M as mdview\n  U->>M: mdview README.md\n";
        let svg = render_svg(code, MermaidTheme::Dark).expect("svg");
        assert!(svg.contains(">mdview README.md</tspan>"));
        assert!(svg.contains("stroke-width=\"1.0\""));
    }

    #[test]
    fn cache_path_is_stable() {
        let a = cache_path("flowchart LR; A-->B", MermaidTheme::Dark, 2048);
        let b = cache_path("flowchart LR; A-->B", MermaidTheme::Dark, 2048);
        let c = cache_path("flowchart LR; A-->B", MermaidTheme::Modern, 2048);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn renders_and_caches_png() {
        let code = "flowchart LR; A-->B";
        let path = render_cached(code, MermaidTheme::Dark, 512).expect("cache");
        assert!(path.exists());
        let image = images::from_png(&std::fs::read(path).expect("png")).expect("decode");
        assert!(image.width() == 512);
    }
}
