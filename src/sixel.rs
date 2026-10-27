//! Sixel encoder: median-cut quantization + RLE.

use std::collections::{BTreeMap, HashMap};

use image::RgbaImage;

#[derive(Debug, Clone)]
struct Bin {
    r: f64,
    g: f64,
    b: f64,
    count: u64,
}

#[derive(Debug, Clone)]
pub struct Quantized {
    /// 3 bytes per colour.
    pub palette: Vec<u8>,
    /// 1 byte per pixel.
    pub indices: Vec<u8>,
    pub colors: usize,
}

fn build_histogram(image: &RgbaImage) -> Vec<Bin> {
    let mut histogram: HashMap<u32, (u64, u64, u64, u64)> = HashMap::new();
    for pixel in image.as_raw().as_chunks::<4>().0 {
        let (r, g, b) = (pixel[0], pixel[1], pixel[2]);
        let key = (((r as u32) >> 3) << 10) | (((g as u32) >> 3) << 5) | ((b as u32) >> 3);
        let entry = histogram.entry(key).or_insert((0, 0, 0, 0));
        entry.0 += r as u64;
        entry.1 += g as u64;
        entry.2 += b as u64;
        entry.3 += 1;
    }

    histogram
        .into_iter()
        .map(|(_, (sr, sg, sb, count))| Bin {
            r: sr as f64 / count as f64,
            g: sg as f64 / count as f64,
            b: sb as f64 / count as f64,
            count,
        })
        .collect()
}

fn channel_range(bins: &[Bin], channel: usize) -> f64 {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for bin in bins {
        let value = match channel {
            0 => bin.r,
            1 => bin.g,
            _ => bin.b,
        };
        min = min.min(value);
        max = max.max(value);
    }
    max - min
}

fn total_count(bins: &[Bin]) -> u64 {
    bins.iter().map(|bin| bin.count).sum()
}

fn channel_value(bin: &Bin, channel: usize) -> f64 {
    match channel {
        0 => bin.r,
        1 => bin.g,
        _ => bin.b,
    }
}

/// Median-cut quantization without dithering.
pub fn quantize(image: &RgbaImage, max_colors: usize) -> Quantized {
    let max_colors = max_colors.clamp(2, 256);
    let bins = build_histogram(image);

    if bins.is_empty() {
        return Quantized {
            palette: vec![0, 0, 0],
            indices: vec![0; image.width() as usize * image.height() as usize],
            colors: 1,
        };
    }

    if bins.len() <= max_colors {
        let palette: Vec<(f64, f64, f64)> = bins.iter().map(|bin| (bin.r, bin.g, bin.b)).collect();
        return map_to_palette(image, &palette);
    }

    let mut boxes: Vec<Vec<Bin>> = vec![bins];
    while boxes.len() < max_colors {
        let mut best_index: Option<usize> = None;
        let mut best_score = -1.0;
        let mut best_channel = 0;

        for (index, bucket) in boxes.iter().enumerate() {
            if bucket.len() < 2 {
                continue;
            }
            let r_range = channel_range(bucket, 0);
            let g_range = channel_range(bucket, 1);
            let b_range = channel_range(bucket, 2);
            let range = r_range.max(g_range).max(b_range);
            if range <= 0.0 {
                continue;
            }
            let score = range * total_count(bucket) as f64;
            if score > best_score {
                best_score = score;
                best_index = Some(index);
                best_channel = if range == r_range {
                    0
                } else if range == g_range {
                    1
                } else {
                    2
                };
            }
        }

        let Some(index) = best_index else {
            break;
        };

        let bucket = &mut boxes[index];
        bucket.sort_by(|a, b| {
            channel_value(a, best_channel)
                .partial_cmp(&channel_value(b, best_channel))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let half = total_count(bucket) as f64 / 2.0;
        let mut accumulated = 0.0;
        let mut split_at = 1;
        for (position, bin) in bucket.iter().enumerate().take(bucket.len() - 1) {
            accumulated += bin.count as f64;
            if accumulated >= half {
                split_at = position + 1;
                break;
            }
        }
        let split_at = split_at.clamp(1, bucket.len() - 1);
        let right = bucket.split_off(split_at);
        boxes.insert(index + 1, right);
    }

    let palette: Vec<(f64, f64, f64)> = boxes
        .iter()
        .map(|bucket| {
            let mut sr = 0.0;
            let mut sg = 0.0;
            let mut sb = 0.0;
            let mut count = 0u64;
            for bin in bucket {
                sr += bin.r * bin.count as f64;
                sg += bin.g * bin.count as f64;
                sb += bin.b * bin.count as f64;
                count += bin.count;
            }
            let count = count.max(1) as f64;
            (sr / count, sg / count, sb / count)
        })
        .collect();

    map_to_palette(image, &palette)
}

fn map_to_palette(image: &RgbaImage, palette: &[(f64, f64, f64)]) -> Quantized {
    let colors = palette.len();
    let mut pal = Vec::with_capacity(colors * 3);
    for (r, g, b) in palette {
        pal.push(r.round().clamp(0.0, 255.0) as u8);
        pal.push(g.round().clamp(0.0, 255.0) as u8);
        pal.push(b.round().clamp(0.0, 255.0) as u8);
    }

    let mut cache: HashMap<u32, u8> = HashMap::new();
    let mut indices = Vec::with_capacity(image.width() as usize * image.height() as usize);
    for pixel in image.as_raw().as_chunks::<4>().0 {
        let (r, g, b) = (pixel[0], pixel[1], pixel[2]);
        let key = (((r as u32) >> 3) << 10) | (((g as u32) >> 3) << 5) | ((b as u32) >> 3);
        let index = *cache.entry(key).or_insert_with(|| {
            let mut best = 0u8;
            let mut best_distance = f64::INFINITY;
            for color in 0..colors {
                let dr = r as f64 - pal[color * 3] as f64;
                let dg = g as f64 - pal[color * 3 + 1] as f64;
                let db = b as f64 - pal[color * 3 + 2] as f64;
                let distance = dr * dr + dg * dg + db * db;
                if distance < best_distance {
                    best_distance = distance;
                    best = color as u8;
                }
            }
            best
        });
        indices.push(index);
    }

    Quantized {
        palette: pal,
        indices,
        colors,
    }
}

/// Compress runs of four or more identical bytes as `!<count><char>`.
pub fn rle_encode(chars: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let value = chars[i];
        let mut j = i + 1;
        while j < chars.len() && chars[j] == value {
            j += 1;
        }
        let run = j - i;
        if run >= 4 {
            output.extend_from_slice(format!("!{run}").as_bytes());
            output.push(value);
        } else {
            output.extend_from_slice(&chars[i..j]);
        }
        i = j;
    }
    output
}

/// Encode an RGBA image (already composited on a background) as Sixel.
pub fn encode_sixel(image: &RgbaImage, max_colors: usize) -> Vec<u8> {
    let width = image.width() as usize;
    let height = image.height() as usize;
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let Quantized {
        palette,
        indices,
        colors,
    } = quantize(image, max_colors);
    let mut output = Vec::new();
    output.extend_from_slice(format!("\x1bP0;1;0q\"1;1;{width};{height}").as_bytes());

    for band in (0..height).step_by(6) {
        let mut bitmaps: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
        for dy in 0..6 {
            let y = band + dy;
            if y >= height {
                break;
            }
            let row_offset = y * width;
            let bit = 1u8 << dy;
            for x in 0..width {
                let color = indices[row_offset + x];
                let bits = bitmaps.entry(color).or_insert_with(|| vec![0u8; width]);
                bits[x] |= bit;
            }
        }

        for (color, bits) in bitmaps {
            let chars: Vec<u8> = bits.iter().map(|value| 63 + value).collect();
            let color_index = color as usize * 3;
            let r = palette.get(color_index).copied().unwrap_or(0);
            let g = palette.get(color_index + 1).copied().unwrap_or(0);
            let b = palette.get(color_index + 2).copied().unwrap_or(0);
            output.extend_from_slice(format!("#{color};2;{r};{g};{b}").as_bytes());
            output.extend_from_slice(&rle_encode(&chars));
            output.push(b'$');
        }
        output.push(b'-');
    }

    output.extend_from_slice(b"\x1b\\");
    let _ = colors;
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn solid(width: u32, height: u32, color: [u8; 3]) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba([color[0], color[1], color[2], 255]))
    }

    fn decode(data: &[u8]) -> (usize, usize, Vec<String>) {
        let text = std::str::from_utf8(data).expect("ASCII sixel");
        let header_re = regex::Regex::new(r#"q"1;1;(\d+);(\d+)"#).expect("header regex");
        let captures = header_re.captures(text).expect("header");
        let width = captures[1].parse::<usize>().expect("width");
        let height = captures[2].parse::<usize>().expect("height");
        let header_end = captures.get(0).expect("match").end();
        let body = &text[header_end..text.len() - 2];

        let mut pixels = vec!["#000000".to_string(); width * height];
        let mut palette: HashMap<u8, String> = HashMap::new();
        let mut color = "#000000".to_string();
        let mut x = 0usize;
        let mut band = 0usize;
        let bytes = body.as_bytes();
        let mut i = 0;

        fn paint(
            pixels: &mut [String],
            color: &str,
            width: usize,
            height: usize,
            x: &mut usize,
            band: usize,
            char: u8,
        ) {
            let value = char - 63;
            for bit in 0..6 {
                if (value >> bit) & 1 == 1 {
                    let y = band + bit;
                    if y < height && *x < width {
                        pixels[y * width + *x] = color.to_string();
                    }
                }
            }
            *x += 1;
        }

        while i < bytes.len() {
            match bytes[i] {
                b'#' => {
                    let rest = &body[i..];
                    let end = rest
                        .find(|ch: char| ch != '#' && !ch.is_ascii_digit() && ch != ';')
                        .unwrap_or(rest.len());
                    let spec = &rest[..end];
                    let parts: Vec<&str> = spec.split(';').collect();
                    let index = parts[0]
                        .trim_start_matches('#')
                        .parse::<u8>()
                        .expect("index");
                    if parts.len() >= 5 {
                        let hex = format!(
                            "#{:02x}{:02x}{:02x}",
                            parts[2].parse::<u8>().unwrap(),
                            parts[3].parse::<u8>().unwrap(),
                            parts[4].parse::<u8>().unwrap()
                        );
                        palette.insert(index, hex.clone());
                        color = hex;
                    } else {
                        color = palette
                            .get(&index)
                            .cloned()
                            .unwrap_or_else(|| "#000000".into());
                    }
                    i += end;
                }
                b'$' => {
                    x = 0;
                    i += 1;
                }
                b'-' => {
                    x = 0;
                    band += 6;
                    i += 1;
                }
                b'!' => {
                    let rest = &body[i + 1..];
                    let digit_end = rest
                        .find(|ch: char| !ch.is_ascii_digit())
                        .unwrap_or(rest.len());
                    let count = rest[..digit_end].parse::<usize>().expect("count");
                    let char = rest.as_bytes()[digit_end];
                    for _ in 0..count {
                        paint(&mut pixels, &color, width, height, &mut x, band, char);
                    }
                    i += 1 + digit_end + 1;
                }
                char => {
                    paint(&mut pixels, &color, width, height, &mut x, band, char);
                    i += 1;
                }
            }
        }

        (width, height, pixels)
    }

    #[test]
    fn rle_compresses_only_long_runs() {
        assert_eq!(rle_encode(b"aaabbbbbc"), b"aaa!5bc");
    }

    #[test]
    fn emits_header_and_terminator() {
        let output = encode_sixel(&solid(4, 6, [10, 20, 30]), 8);
        let text = String::from_utf8_lossy(&output);
        assert!(text.starts_with("\x1bP0;1;0q\"1;1;4;6"));
        assert!(text.ends_with("\x1b\\"));
    }

    #[test]
    fn round_trips_solid_color() {
        let output = encode_sixel(&solid(10, 7, [200, 100, 50]), 4);
        let (width, height, pixels) = decode(&output);
        assert_eq!((width, height), (10, 7));
        assert!(pixels.iter().all(|pixel| pixel == "#c86432"));
    }

    #[test]
    fn preserves_four_color_quadrants() {
        let mut image = solid(8, 8, [0, 0, 0]);
        let colors = [[255u8, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]];
        for y in 0..8 {
            for x in 0..8 {
                let color = colors[(y / 4) * 2 + x / 4];
                image.put_pixel(
                    x as u32,
                    y as u32,
                    Rgba([color[0], color[1], color[2], 255]),
                );
            }
        }
        let (_, _, pixels) = decode(&encode_sixel(&image, 16));
        let at = |x: usize, y: usize| pixels[y * 8 + x].as_str();
        assert_eq!(at(0, 0), "#ff0000");
        assert_eq!(at(7, 0), "#00ff00");
        assert_eq!(at(0, 7), "#0000ff");
        assert_eq!(at(7, 7), "#ffffff");
    }

    #[test]
    fn respects_max_colors() {
        let mut image = solid(32, 32, [0, 0, 0]);
        for y in 0..32 {
            for x in 0..32 {
                image.put_pixel(
                    x as u32,
                    y as u32,
                    Rgba([(x * 8) as u8, (y * 8) as u8, 128, 255]),
                );
            }
        }
        let (_, _, pixels) = decode(&encode_sixel(&image, 8));
        let unique: std::collections::HashSet<&String> = pixels.iter().collect();
        assert!(unique.len() <= 8);
        assert!(unique.len() > 1);
    }
}
