//! Mermaid -> SVG -> PNG pipeline using the pure-Rust renderer.
//!
//! mdview currently uses Pele (JavaScript). The Rust port swaps that for
//! `mermaid-rs-renderer`. The theme background is forced transparent so the
//! terminal background shows through.

use std::path::{Path, PathBuf};

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

const CACHE_VERSION: &str = "4";

pub fn cache_path(code: &str, theme: MermaidTheme, width: u32) -> PathBuf {
    let key = short_hash(&[CACHE_VERSION, code, theme.as_str(), &width.to_string()]);
    mermaid_cache_dir().join(format!("{key}.png"))
}

fn raw_cache_path(code: &str, theme: MermaidTheme, width: u32) -> PathBuf {
    let key = short_hash(&[CACHE_VERSION, code, theme.as_str(), &width.to_string()]);
    mermaid_cache_dir().join(format!("{key}.rgba"))
}

fn write_raw_cache(image: &RgbaImage, path: &Path) -> Result<()> {
    let mut data = Vec::with_capacity(8 + image.as_raw().len());
    data.extend_from_slice(&image.width().to_le_bytes());
    data.extend_from_slice(&image.height().to_le_bytes());
    data.extend_from_slice(image.as_raw());
    std::fs::write(path, data).with_context(|| format!("cannot write {}", path.display()))
}

fn read_raw_cache(path: &Path) -> Option<RgbaImage> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 8 {
        return None;
    }
    let width = u32::from_le_bytes(data[0..4].try_into().ok()?);
    let height = u32::from_le_bytes(data[4..8].try_into().ok()?);
    RgbaImage::from_raw(width, height, data[8..].to_vec())
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
    let _ = write_raw_cache(&image, &raw_cache_path(code, theme, width));
    Ok(path)
}

/// Load a cached image, or render and cache it.
///
/// A raw RGBA cache is used alongside the PNG cache. Decoding a 1000px-wide
/// PNG with the pure-Rust `image` decoder is surprisingly expensive; the raw
/// file turns warm runs into a plain `fs::read` plus `RgbaImage::from_raw`.
pub fn load_cached(code: &str, theme: MermaidTheme, width: u32) -> Result<RgbaImage> {
    let raw_path = raw_cache_path(code, theme, width);
    if let Some(image) = read_raw_cache(&raw_path) {
        return Ok(image);
    }

    let png_path = render_cached(code, theme, width)?;
    let image = images::from_png(&std::fs::read(&png_path)?)?;
    let _ = write_raw_cache(&image, &raw_path);
    Ok(image)
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
    fn class_multiplicity_stays_beside_the_edge() {
        use mermaid_rs_renderer::{compute_layout, parse_mermaid_strict, LayoutConfig};
        for direction in ["TB", "BT", "LR", "RL"] {
            let code = "classDiagram\n  class Invoice {\n    +String number\n    +issue()\n  }\n  Company \"1\" --> \"*\" Invoice\n";
            let code = code.replacen(
                "classDiagram\n",
                &format!("classDiagram\n direction {direction}\n"),
                1,
            );
            let parsed = parse_mermaid_strict(&code).unwrap();
            let theme = MermaidTheme::Dark.theme();
            let layout = compute_layout(&parsed.graph, &theme, &LayoutConfig::default());
            let edge = &layout.edges[0];

            for (label, anchor) in [
                (&edge.start_label, edge.start_label_anchor),
                (&edge.end_label, edge.end_label_anchor),
            ] {
                let label = label.as_ref().unwrap();
                let (x, y) = anchor.unwrap();
                for pair in edge.points.windows(2) {
                    if (pair[0].0 - pair[1].0).abs() < 0.01 {
                        assert!(
                            (x - pair[0].0).abs() > label.width / 2.0,
                            "label overlaps vertical edge"
                        );
                    } else if (pair[0].1 - pair[1].1).abs() < 0.01 {
                        assert!(
                            (y - pair[0].1).abs() > label.height / 2.0,
                            "label overlaps horizontal edge"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn er_crows_feet_point_away_from_nodes() {
        let svg = render_svg(
            "erDiagram\n  COMPANY ||--o{ INVOICE : has\n  INVOICE ||--|{ INVOICE_LINE : contains\n",
            MermaidTheme::Dark,
        )
        .unwrap();
        assert_eq!(svg.matches("M 8 0 L 0 -6 M 8 0 L 0 6").count(), 2);
        assert!(svg.contains("cx=\"16\""));
        // Downward edges need their end markers rotated back into the edge.
        assert_eq!(svg.matches("rotate(270.00)").count(), 2);
    }

    #[test]
    fn er_labels_clear_nodes_other_labels_and_cardinality_markers() {
        use mermaid_rs_renderer::{compute_layout, parse_mermaid_strict, LayoutConfig};
        let overlaps = |a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)| {
            a.0 < b.2 - 0.01 && a.2 > b.0 + 0.01 && a.1 < b.3 - 0.01 && a.3 > b.1 + 0.01
        };
        for code in [
            "erDiagram\n COMPANY ||--o{ INVOICE : has\n INVOICE ||--|{ INVOICE_LINE : contains",
            "erDiagram\n A ||--o{ B : has\n A ||--o{ C : owns",
            "erDiagram\n A ||--o{ B : first\n A ||--o{ B : second",
        ] {
            let parsed = parse_mermaid_strict(code).unwrap();
            let layout = compute_layout(
                &parsed.graph,
                &MermaidTheme::Dark.theme(),
                &LayoutConfig::default(),
            );
            let labels: Vec<_> = layout
                .edges
                .iter()
                .filter_map(|edge| {
                    let label = edge.label.as_ref()?;
                    let (x, y) = edge.label_anchor?;
                    Some((
                        x - label.width / 2.0,
                        y - label.height / 2.0,
                        x + label.width / 2.0,
                        y + label.height / 2.0,
                    ))
                })
                .collect();
            for (i, &rect) in labels.iter().enumerate() {
                for node in layout.nodes.values() {
                    assert!(
                        !overlaps(
                            rect,
                            (node.x, node.y, node.x + node.width, node.y + node.height)
                        ),
                        "ER label overlaps node {}: {code}",
                        node.id
                    );
                }
                for &other in &labels[i + 1..] {
                    assert!(!overlaps(rect, other), "ER labels overlap: {code}");
                }
                for edge in &layout.edges {
                    for (decoration, points) in [
                        (edge.start_decoration, (edge.points[0], edge.points[1])),
                        (
                            edge.end_decoration,
                            (
                                edge.points[edge.points.len() - 1],
                                edge.points[edge.points.len() - 2],
                            ),
                        ),
                    ] {
                        if decoration.is_none() {
                            continue;
                        }
                        let (p, next) = points;
                        let length = (next.0 - p.0).hypot(next.1 - p.1);
                        let (dx, dy) = ((next.0 - p.0) / length, (next.1 - p.1) / length);
                        let center = (p.0 + dx * 10.0, p.1 + dy * 10.0);
                        let label_center = ((rect.0 + rect.2) / 2.0, (rect.1 + rect.3) / 2.0);
                        let half = ((rect.2 - rect.0) / 2.0, (rect.3 - rect.1) / 2.0);
                        let separated = [(1.0, 0.0), (0.0, 1.0), (dx, dy), (-dy, dx)].iter().any(
                            |&(ax, ay)| {
                                let distance = ((label_center.0 - center.0) * ax
                                    + (label_center.1 - center.1) * ay)
                                    .abs();
                                let label_radius = half.0 * ax.abs() + half.1 * ay.abs();
                                let marker_radius = 10.0 * (dx * ax + dy * ay).abs()
                                    + 6.0 * (-dy * ax + dx * ay).abs();
                                distance >= label_radius + marker_radius - 0.01
                            },
                        );
                        assert!(
                            separated,
                            "ER label overlaps cardinality marker on {}->{}: {code}",
                            edge.from, edge.to
                        );
                    }
                }
            }
        }
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
        let code = format!("flowchart LR; A-->B{}", std::process::id());
        let png = cache_path(&code, MermaidTheme::Dark, 512);
        let raw = raw_cache_path(&code, MermaidTheme::Dark, 512);
        let _ = std::fs::remove_file(&png);
        let _ = std::fs::remove_file(&raw);
        let image = load_cached(&code, MermaidTheme::Dark, 512).expect("cache");
        assert!(image.width() == 512);
        assert!(png.exists());
        assert!(raw.exists());
        let _ = std::fs::remove_file(&png);
        let _ = std::fs::remove_file(&raw);
    }
}
