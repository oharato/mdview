import type { RgbaImage } from "./sixel.js";

export type ColorMode = "truecolor" | "256";

export interface HalfBlockPlan {
  cols: number;
  rows: number;
}

/** 半ブロック描画（1 セル = 縦 2 ピクセル）の出力サイズを決める。 */
export function planHalfBlocks(
  width: number,
  height: number,
  maxCols: number,
  maxRows = 0,
): HalfBlockPlan {
  if (width <= 0 || height <= 0) return { cols: 1, rows: 1 };
  let cols = Math.max(1, Math.min(maxCols, width));
  let rows = Math.max(1, Math.round((cols * height) / (2 * width)));
  if (maxRows > 0 && rows > maxRows) {
    cols = Math.max(1, Math.floor((cols * maxRows) / rows));
    rows = Math.max(1, Math.round((cols * height) / (2 * width)));
  }
  return { cols, rows };
}

function escapeTruecolor(r: number, g: number, b: number, bg: boolean): string {
  return `\x1b[${bg ? 48 : 38};2;${r};${g};${b}m`;
}

function to256(r: number, g: number, b: number): number {
  const q = (v: number): number => (v < 48 ? 0 : v < 115 ? 1 : Math.floor((v - 35) / 40));
  let idx = 16 + 36 * q(r) + 6 * q(g) + q(b);
  if (Math.abs(r - g) < 10 && Math.abs(g - b) < 10) {
    const avg = Math.floor((r + g + b) / 3);
    idx = avg < 8 ? 16 : avg > 238 ? 231 : 232 + Math.min(23, Math.max(0, Math.floor((avg - 8) / 10)));
  }
  return idx;
}

function escape256(r: number, g: number, b: number, bg: boolean): string {
  return `\x1b[${bg ? 48 : 38};5;${to256(r, g, b)}m`;
}

/**
 * 縦 2 ピクセルを `▀` (U+2580) の前景色／背景色で表現する。
 * img は `cols x rows*2` にリサイズ済みであること。
 */
export function renderHalfBlocks(img: RgbaImage, mode: ColorMode = "truecolor"): string {
  const esc = mode === "truecolor" ? escapeTruecolor : escape256;
  const { width, height, data } = img;
  const rows = Math.floor(height / 2);
  const lines: string[] = [];

  for (let y = 0; y < rows; y += 1) {
    let currentFg = "";
    let currentBg = "";
    const parts: string[] = [];
    const topRow = y * 2 * width * 4;
    const bottomRow = (y * 2 + 1) * width * 4;

    for (let x = 0; x < width; x += 1) {
      const ti = topRow + x * 4;
      const bi = bottomRow + x * 4;
      const ta = data[ti + 3] ?? 255;
      const ba = data[bi + 3] ?? 255;
      const topOpaque = ta >= 128;
      const bottomOpaque = ba >= 128;

      if (topOpaque) {
        const key = `${data[ti]},${data[ti + 1]},${data[ti + 2]}`;
        if (key !== currentFg) {
          parts.push(esc(data[ti] ?? 0, data[ti + 1] ?? 0, data[ti + 2] ?? 0, false));
          currentFg = key;
        }
        if (bottomOpaque) {
          const bgKey = `${data[bi]},${data[bi + 1]},${data[bi + 2]}`;
          if (bgKey !== currentBg) {
            parts.push(esc(data[bi] ?? 0, data[bi + 1] ?? 0, data[bi + 2] ?? 0, true));
            currentBg = bgKey;
          }
        } else if (currentBg !== "") {
          parts.push("\x1b[49m");
          currentBg = "";
        }
        parts.push("\u2580");
      } else if (bottomOpaque) {
        const key = `${data[bi]},${data[bi + 1]},${data[bi + 2]}`;
        if (key !== currentFg) {
          parts.push(esc(data[bi] ?? 0, data[bi + 1] ?? 0, data[bi + 2] ?? 0, false));
          currentFg = key;
        }
        if (currentBg !== "") {
          parts.push("\x1b[49m");
          currentBg = "";
        }
        parts.push("\u2584");
      } else {
        if (currentBg !== "") {
          parts.push("\x1b[49m");
          currentBg = "";
        }
        parts.push(" ");
      }
    }
    parts.push("\x1b[0m\n");
    lines.push(parts.join(""));
  }

  return lines.join("");
}
