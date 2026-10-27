//! ANSI half-block image rendering (two vertical pixels per cell).

use image::imageops::FilterType;
use image::RgbaImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorMode {
    Truecolor,
    #[value(name = "256")]
    Ansi256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HalfBlockPlan {
    pub cols: u32,
    pub rows: u32,
}

pub fn plan_half_blocks(width: u32, height: u32, max_cols: u32, max_rows: u32) -> HalfBlockPlan {
    if width == 0 || height == 0 {
        return HalfBlockPlan { cols: 1, rows: 1 };
    }
    let mut cols = max_cols.max(1).min(width);
    let mut rows = ((cols as f64 * height as f64) / (2.0 * width as f64))
        .round()
        .max(1.0) as u32;
    if max_rows > 0 && rows > max_rows {
        cols = ((cols as f64 * max_rows as f64) / rows as f64)
            .floor()
            .max(1.0) as u32;
        rows = ((cols as f64 * height as f64) / (2.0 * width as f64))
            .round()
            .max(1.0) as u32;
    }
    HalfBlockPlan { cols, rows }
}

fn escape_truecolor(r: u8, g: u8, b: u8, background: bool) -> String {
    format!("\x1b[{};2;{r};{g};{b}m", if background { 48 } else { 38 })
}

fn to_256(r: u8, g: u8, b: u8) -> u8 {
    let q = |value: u8| -> u8 {
        if value < 48 {
            0
        } else if value < 115 {
            1
        } else {
            ((value as u16 - 35) / 40) as u8
        }
    };
    let mut index = 16 + 36 * q(r) + 6 * q(g) + q(b);
    if r.abs_diff(g) < 10 && g.abs_diff(b) < 10 {
        let average = ((r as u16 + g as u16 + b as u16) / 3) as u8;
        index = if average < 8 {
            16
        } else if average > 238 {
            231
        } else {
            232 + ((average as u16 - 8) / 10).min(23) as u8
        };
    }
    index
}

fn blend_pixel(data: &[u8], index: usize, background: Option<[u8; 3]>) -> (u8, u8, u8, u8) {
    let alpha = data[index + 3];
    if alpha == 0 {
        return (0, 0, 0, 0);
    }
    if alpha == 255 {
        return (data[index], data[index + 1], data[index + 2], 255);
    }
    let Some(background) = background else {
        return (data[index], data[index + 1], data[index + 2], alpha);
    };
    let alpha = alpha as u32;
    let inverse = 255 - alpha;
    (
        ((data[index] as u32 * alpha + background[0] as u32 * inverse) / 255) as u8,
        ((data[index + 1] as u32 * alpha + background[1] as u32 * inverse) / 255) as u8,
        ((data[index + 2] as u32 * alpha + background[2] as u32 * inverse) / 255) as u8,
        255,
    )
}

fn escape_256(r: u8, g: u8, b: u8, background: bool) -> String {
    format!(
        "\x1b[{};5;{}m",
        if background { 48 } else { 38 },
        to_256(r, g, b)
    )
}

/// Render an image as `▀` half-blocks.
///
/// When `background` is set, partially transparent pixels are alpha-blended
/// against it (fully transparent pixels still leave the terminal background
/// visible).
pub fn render_half_blocks(
    image: &RgbaImage,
    max_cols: u32,
    max_rows: u32,
    mode: ColorMode,
    background: Option<[u8; 3]>,
) -> String {
    let plan = plan_half_blocks(image.width(), image.height(), max_cols, max_rows);
    let resized = image::imageops::resize(image, plan.cols, plan.rows * 2, FilterType::Lanczos3);
    let width = resized.width() as usize;
    let data = resized.as_raw();
    let escape = match mode {
        ColorMode::Truecolor => escape_truecolor,
        ColorMode::Ansi256 => escape_256,
    };
    let mut output = String::new();

    for y in 0..plan.rows as usize {
        let mut current_fg: Option<(u8, u8, u8)> = None;
        let mut current_bg: Option<(u8, u8, u8)> = None;
        let top_row = y * 2 * width * 4;
        let bottom_row = (y * 2 + 1) * width * 4;

        for x in 0..width {
            let ti = top_row + x * 4;
            let bi = bottom_row + x * 4;
            let top = blend_pixel(data, ti, background);
            let bottom = blend_pixel(data, bi, background);
            let top_opaque = top.3 >= 128;
            let bottom_opaque = bottom.3 >= 128;
            let top = (top.0, top.1, top.2);
            let bottom = (bottom.0, bottom.1, bottom.2);

            if top_opaque {
                if current_fg != Some(top) {
                    output.push_str(&escape(top.0, top.1, top.2, false));
                    current_fg = Some(top);
                }
                if bottom_opaque {
                    if current_bg != Some(bottom) {
                        output.push_str(&escape(bottom.0, bottom.1, bottom.2, true));
                        current_bg = Some(bottom);
                    }
                } else if current_bg.is_some() {
                    output.push_str("\x1b[49m");
                    current_bg = None;
                }
                output.push('\u{2580}');
            } else if bottom_opaque {
                if current_fg != Some(bottom) {
                    output.push_str(&escape(bottom.0, bottom.1, bottom.2, false));
                    current_fg = Some(bottom);
                }
                if current_bg.is_some() {
                    output.push_str("\x1b[49m");
                    current_bg = None;
                }
                output.push('\u{2584}');
            } else {
                if current_bg.is_some() {
                    output.push_str("\x1b[49m");
                    current_bg = None;
                }
                output.push(' ');
            }
        }
        output.push_str("\x1b[0m\n");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn solid(width: u32, height: u32, color: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba(color))
    }

    #[test]
    fn plans_aspect_ratio_and_caps_rows() {
        let plan = plan_half_blocks(100, 100, 40, 100);
        assert_eq!(plan.cols, 40);
        assert_eq!(plan.rows, 20);

        let capped = plan_half_blocks(100, 100, 40, 10);
        assert!(capped.rows <= 10);
        assert!(capped.cols <= 40);
    }

    #[test]
    fn renders_truecolor_half_blocks() {
        let image = solid(2, 2, [255, 0, 0, 255]);
        let output = render_half_blocks(&image, 2, 1, ColorMode::Truecolor, None);
        assert!(output.contains("\x1b[38;2;255;0;0m"));
        assert!(output.contains('\u{2580}'));
        assert!(output.ends_with('\n'));
    }

    #[test]
    fn transparent_pixels_leave_terminal_background() {
        let image = solid(2, 2, [0, 0, 0, 0]);
        let output = render_half_blocks(&image, 2, 1, ColorMode::Truecolor, None);
        assert!(!output.contains("38;2;"));
        assert!(output.contains(' '));
    }

    #[test]
    fn blends_partial_alpha_with_background() {
        let mut image = solid(1, 2, [255, 0, 0, 128]);
        image.put_pixel(0, 0, Rgba([255, 0, 0, 128]));
        image.put_pixel(0, 1, Rgba([255, 0, 0, 128]));
        let output = render_half_blocks(&image, 1, 1, ColorMode::Truecolor, Some([0, 0, 255]));
        assert!(output.contains("\x1b[38;2;128;0;127m"));
    }

    #[test]
    fn supports_ansi_256_colors() {
        let image = solid(2, 2, [255, 0, 0, 255]);
        let output = render_half_blocks(&image, 2, 1, ColorMode::Ansi256, None);
        assert!(output.contains("\x1b[38;5;"));
    }
}
