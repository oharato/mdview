import { readFile } from "node:fs/promises";
import { extname } from "node:path";
import { Resvg } from "@resvg/resvg-js";
import sharp from "sharp";
import type { RgbaImage } from "./sixel.js";
import { cellPixels, windowPixels } from "./term.js";
import {
  applyCssVarFallbacks,
  isPeleSvg,
  resolveCssVariables,
  type MermaidTheme,
} from "./theme.js";

export type Rgb = [number, number, number];

/** 画像を読み込み RGBA に正規化する（EXIF の回転も適用）。 */
export async function loadRaster(path: string): Promise<RgbaImage> {
  const { data, info } = await sharp(path, { animated: false, limitInputPixels: 100 * 1024 * 1024 })
    .rotate()
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  return { width: info.width, height: info.height, data };
}

/** PNG などのバッファから RGBA を得る。 */
export async function rasterFromBuffer(buf: Buffer): Promise<RgbaImage> {
  const { data, info } = await sharp(buf, { animated: false })
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  return { width: info.width, height: info.height, data };
}

/** PNG バッファに変換する（Kitty / iTerm2 プロトコル用）。 */
export async function toPng(img: RgbaImage, compressionLevel = 9): Promise<Buffer> {
  return await sharp(img.data, { raw: { width: img.width, height: img.height, channels: 4 } })
    .png({ compressionLevel, adaptiveFiltering: true })
    .toBuffer();
}

/** 指定サイズへ（アスペクト比は呼び出し側で合わせる）。 */
export async function resizeImage(img: RgbaImage, width: number, height: number): Promise<RgbaImage> {
  const w = Math.max(1, Math.round(width));
  const h = Math.max(1, Math.round(height));
  if (img.width === w && img.height === h) return img;
  const { data, info } = await sharp(img.data, {
    raw: { width: img.width, height: img.height, channels: 4 },
  })
    .resize(w, h, { fit: "fill", kernel: "lanczos3" })
    .raw()
    .toBuffer({ resolveWithObject: true });
  return { width: info.width, height: info.height, data };
}

/** 透過画像を背景色に合成する。 */
export async function flattenImage(img: RgbaImage, background: Rgb): Promise<RgbaImage> {
  const { data, info } = await sharp(img.data, {
    raw: { width: img.width, height: img.height, channels: 4 },
  })
    .flatten({ background: { r: background[0], g: background[1], b: background[2] } })
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  return { width: info.width, height: info.height, data };
}

/** SVG を指定幅でラスタライズして PNG バッファを返す。 */
export function rasterizeSvg(svg: string, width: number): Buffer {
  const renderer = new Resvg(svg, {
    fitTo: { mode: "width", value: Math.max(1, Math.round(width)) },
    background: "rgba(0,0,0,0)",
    font: {
      loadSystemFonts: true,
      defaultFontFamily: "sans-serif",
    },
  });
  return Buffer.from(renderer.render().asPng());
}

/** SVG ファイルを PNG（RGBA）に変換する。Pele 由来なら端末テーマに合わせる。 */
export async function loadSvg(
  path: string,
  width: number,
  theme: MermaidTheme = "dark",
): Promise<RgbaImage> {
  let svg = await readFile(path, "utf8");
  if (isPeleSvg(svg)) svg = resolveCssVariables(svg, theme);
  svg = applyCssVarFallbacks(svg);
  return await rasterFromBuffer(rasterizeSvg(svg, width));
}

export function isSvg(path: string): boolean {
  return extname(path).toLowerCase() === ".svg";
}

export interface SixelSizing {
  imageWidthCols: number;
  pixelScale: number;
  upscale: boolean;
  terminalRows: number;
}

/** Sixel で表示する (幅px, 高さpx) を端末の実ピクセルから決める。 */
export async function planDisplaySize(
  width: number,
  height: number,
  sizing: SixelSizing,
): Promise<[number, number]> {
  const [cellWidth, cellHeight] = await cellPixels();
  let targetWidth = Math.max(8, Math.round(sizing.imageWidthCols * cellWidth));
  if (sizing.pixelScale !== 1) targetWidth = Math.max(8, Math.round(targetWidth * sizing.pixelScale));

  const window = await windowPixels();
  if (window) targetWidth = Math.min(targetWidth, window[0]);

  if (width <= 0 || height <= 0) return [targetWidth, 1];

  const maxHeight = Math.max(6, (sizing.terminalRows - 3) * cellHeight);
  let scale = Math.min(targetWidth / width, maxHeight / height);
  if (scale > 1) scale = sizing.upscale ? Math.min(scale, 3) : 1;

  return [Math.max(1, Math.round(width * scale)), Math.max(1, Math.round(height * scale))];
}

/** `auto|none|#RRGGBB` を解釈する。auto は明暗どちらの端末かを考慮する。 */
export function parseBackground(value: string, light: boolean): Rgb | null {
  if (value === "none") return null;
  if (value === "auto") return light ? [255, 255, 255] : [12, 12, 12];
  let hex = value.replace(/^#/, "");
  if (hex.length === 3) hex = [...hex].map((c) => c + c).join("");
  const num = Number.parseInt(hex, 16);
  if (Number.isNaN(num) || hex.length !== 6) return [0, 0, 0];
  return [(num >> 16) & 0xff, (num >> 8) & 0xff, num & 0xff];
}

