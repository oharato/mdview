//! Image loading, resizing, SVG rasterization, and colour handling.

use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::imageops::FilterType as ResizeFilter;
use image::{ExtendedColorType, ImageEncoder, RgbaImage};

pub type Rgb = [u8; 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageRendererKind {
    Kitty,
    Sixel,
    Iterm,
    Blocks,
}

/// Load a raster image and normalize it to RGBA.
///
/// The fallback to `load_from_memory` handles cached remote images whose URL
/// had no extension and were stored as `.img`.
pub fn load_raster(path: &Path) -> Result<RgbaImage> {
    match image::open(path) {
        Ok(image) => Ok(image.to_rgba8()),
        Err(open_error) => {
            let bytes = std::fs::read(path)
                .with_context(|| format!("failed to read image: {}", path.display()))?;
            let image = image::load_from_memory(&bytes).with_context(|| {
                format!("failed to decode image {}: {open_error}", path.display())
            })?;
            Ok(image.to_rgba8())
        }
    }
}

/// True for `.svg` paths (case-insensitive), or when the file starts with SVG.
pub fn is_svg(path: &Path) -> bool {
    let by_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("svg"))
        .unwrap_or(false);
    if by_extension {
        return true;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut buffer = [0u8; 512];
    let Ok(read) = std::io::Read::read(&mut file, &mut buffer) else {
        return false;
    };
    let text = String::from_utf8_lossy(&buffer[..read]);
    let trimmed = text.trim_start();
    trimmed.starts_with("<svg") || (trimmed.starts_with("<?xml") && trimmed.contains("<svg"))
}

/// Rasterize an SVG file at `target_width` pixels, preserving transparency.
pub fn rasterize_svg_file(path: &Path, target_width: u32) -> Result<RgbaImage> {
    let svg = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read SVG: {}", path.display()))?;
    rasterize_svg(&svg, target_width)
}

/// Rasterize an SVG string at `target_width` pixels, preserving transparency.
pub fn rasterize_svg(svg: &str, target_width: u32) -> Result<RgbaImage> {
    let options = resvg::usvg::Options {
        font_family: "Noto Sans CJK JP".to_string(),
        fontdb: system_fonts(),
        ..resvg::usvg::Options::default()
    };

    let tree = resvg::usvg::Tree::from_str(svg, &options).context("failed to parse SVG")?;
    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        anyhow::bail!("SVG has no size");
    }

    let width = target_width.max(1);
    let scale = width as f32 / size.width();
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).context("failed to allocate SVG pixmap")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let data = pixmap.take_demultiplied();
    RgbaImage::from_raw(width, height, data).context("invalid SVG pixmap buffer")
}

fn system_fonts() -> Arc<resvg::usvg::fontdb::Database> {
    static FONTDB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONTDB
        .get_or_init(|| {
            let mut database = resvg::usvg::fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
}

/// Encode an RGBA image as PNG.
pub fn to_png(image: &RgbaImage, compression: CompressionType) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let encoder = PngEncoder::new_with_quality(&mut output, compression, FilterType::Adaptive);
    encoder
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )
        .context("failed to encode PNG")?;
    Ok(output)
}

/// Resize to exact dimensions using Lanczos3.
pub fn resize_image(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let width = width.max(1);
    let height = height.max(1);
    if image.width() == width && image.height() == height {
        return image.clone();
    }
    image::imageops::resize(image, width, height, ResizeFilter::Lanczos3)
}

/// Composite an RGBA image over an opaque background.
pub fn flatten_image(image: &RgbaImage, background: Rgb) -> RgbaImage {
    let mut output = image.clone();
    for pixel in output.pixels_mut() {
        let alpha = pixel[3] as u32;
        if alpha == 255 {
            continue;
        }
        let inverse = 255 - alpha;
        pixel[0] = ((pixel[0] as u32 * alpha + background[0] as u32 * inverse) / 255) as u8;
        pixel[1] = ((pixel[1] as u32 * alpha + background[1] as u32 * inverse) / 255) as u8;
        pixel[2] = ((pixel[2] as u32 * alpha + background[2] as u32 * inverse) / 255) as u8;
        pixel[3] = 255;
    }
    output
}

/// Decide how transparent pixels should be treated.
pub fn resolve_background(value: &str, light: bool, renderer: ImageRendererKind) -> Option<Rgb> {
    if value == "none" {
        return None;
    }
    if value == "auto" {
        if renderer == ImageRendererKind::Sixel {
            return Some(if light { [255, 255, 255] } else { [12, 12, 12] });
        }
        return None;
    }
    parse_background(value, light)
}

/// Parse `auto|none|#RRGGBB`.
pub fn parse_background(value: &str, light: bool) -> Option<Rgb> {
    if value == "none" {
        return None;
    }
    if value == "auto" {
        return Some(if light { [255, 255, 255] } else { [12, 12, 12] });
    }
    let mut hex = value.trim_start_matches('#').to_string();
    if hex.len() == 3 {
        hex = hex.chars().flat_map(|ch| [ch, ch]).collect();
    }
    if hex.len() != 6 {
        return Some([0, 0, 0]);
    }
    let parsed = u32::from_str_radix(&hex, 16).unwrap_or(0);
    Some([
        ((parsed >> 16) & 0xff) as u8,
        ((parsed >> 8) & 0xff) as u8,
        (parsed & 0xff) as u8,
    ])
}

#[allow(clippy::too_many_arguments)]
/// Choose the display size in pixels for Kitty / Sixel / iTerm2.
pub fn plan_display_size(
    width: u32,
    height: u32,
    image_width_cols: u32,
    cell: (u32, u32),
    window: Option<(u32, u32)>,
    pixel_scale: f32,
    upscale: bool,
    terminal_rows: u16,
) -> (u32, u32) {
    let mut target_width = ((image_width_cols * cell.0) as f32).round().max(8.0);
    if (pixel_scale - 1.0).abs() > f32::EPSILON {
        target_width = (target_width * pixel_scale).max(8.0);
    }
    if let Some((window_width, _)) = window {
        target_width = target_width.min(window_width as f32);
    }

    if width == 0 || height == 0 {
        return (target_width.max(1.0) as u32, 1);
    }

    let max_height = (terminal_rows.saturating_sub(3) as f32 * cell.1 as f32).max(6.0);
    let mut scale = (target_width / width as f32).min(max_height / height as f32);
    if scale > 1.0 {
        scale = if upscale { scale.min(3.0) } else { 1.0 };
    }
    (
        (width as f32 * scale).round().max(1.0) as u32,
        (height as f32 * scale).round().max(1.0) as u32,
    )
}

/// Save an RGBA image as PNG.
pub fn save_png(image: &RgbaImage, path: &Path) -> Result<()> {
    let png = to_png(image, CompressionType::Best)?;
    std::fs::write(path, png).with_context(|| format!("failed to write PNG: {}", path.display()))
}

/// Decode PNG bytes to RGBA (used by the cache and tests).
pub fn from_png(data: &[u8]) -> Result<RgbaImage> {
    let image = image::load_from_memory_with_format(data, image::ImageFormat::Png)
        .context("failed to decode PNG")?;
    Ok(image.to_rgba8())
}

/// Encode to a `Cursor` (convenience for tests).
#[allow(dead_code)]
pub fn to_png_cursor(image: &RgbaImage) -> Result<Cursor<Vec<u8>>> {
    Ok(Cursor::new(to_png(image, CompressionType::Best)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn auto_background_preserves_alpha_except_sixel() {
        assert_eq!(
            resolve_background("auto", false, ImageRendererKind::Kitty),
            None
        );
        assert_eq!(
            resolve_background("auto", false, ImageRendererKind::Iterm),
            None
        );
        assert_eq!(
            resolve_background("auto", false, ImageRendererKind::Blocks),
            None
        );
        assert_eq!(
            resolve_background("auto", false, ImageRendererKind::Sixel),
            Some([12, 12, 12])
        );
        assert_eq!(
            resolve_background("auto", true, ImageRendererKind::Sixel),
            Some([255, 255, 255])
        );
    }

    #[test]
    fn explicit_backgrounds_are_parsed() {
        assert_eq!(
            resolve_background("#ff0000", false, ImageRendererKind::Kitty),
            Some([255, 0, 0])
        );
        assert_eq!(
            resolve_background("#0f0", false, ImageRendererKind::Blocks),
            Some([0, 255, 0])
        );
        assert_eq!(
            resolve_background("none", false, ImageRendererKind::Sixel),
            None
        );
        assert_eq!(parse_background("auto", false), Some([12, 12, 12]));
        assert_eq!(parse_background("auto", true), Some([255, 255, 255]));
    }

    #[test]
    fn flattens_alpha_onto_background() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 128]));
        let flattened = flatten_image(&image, [0, 0, 255]);
        let pixel = flattened.get_pixel(0, 0);
        assert_eq!(pixel[3], 255);
        assert!(pixel[0] > 120);
        assert!(pixel[2] > 120);
    }

    #[test]
    fn display_size_respects_width_and_upscale() {
        let (width, height) = plan_display_size(100, 100, 40, (9, 18), None, 1.0, true, 40);
        assert_eq!(width, 300);
        assert_eq!(height, 300);

        let (width, height) = plan_display_size(100, 100, 40, (9, 18), None, 1.0, false, 40);
        assert_eq!((width, height), (100, 100));
    }

    #[test]
    fn detects_svg_by_content() {
        let path = std::env::temp_dir().join(format!("mdview-svg-{}.img", std::process::id()));
        std::fs::write(
            &path,
            "<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        )
        .expect("write");
        assert!(is_svg(&path));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn loads_png_without_extension() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([9, 8, 7, 255]));
        let png = to_png(&image, CompressionType::Best).expect("encode");
        let path = std::env::temp_dir().join(format!("mdview-img-{}.img", std::process::id()));
        std::fs::write(&path, png).expect("write");
        let loaded = load_raster(&path).expect("load");
        assert_eq!(loaded.get_pixel(0, 0).0, [9, 8, 7, 255]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn png_round_trip() {
        let image = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
        let png = to_png(&image, CompressionType::Best).expect("encode");
        let decoded = from_png(&png).expect("decode");
        assert_eq!(decoded.dimensions(), (2, 2));
        assert_eq!(decoded.get_pixel(0, 0).0, [1, 2, 3, 255]);
    }
}
