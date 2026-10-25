import { describe, expect, it } from "vitest";
import { encodeSixel, rleEncode, type RgbaImage } from "../src/sixel.js";

interface Decoded {
  width: number;
  height: number;
  pixels: string[];
}

/** テスト用の最小 Sixel デコーダ（パレット + RLE）。 */
function decodeSixel(data: Buffer): Decoded {
  const text = data.toString("latin1");
  const header = /^\x1bP0;1;0q"1;1;(\d+);(\d+)/.exec(text);
  if (!header) throw new Error("invalid sixel header");
  const width = Number(header[1]);
  const height = Number(header[2]);
  const body = text.slice(header[0].length, text.length - 2);

  const pixels = new Array<string>(width * height).fill("#000000");
  const palette = new Map<number, string>();
  let color = "#000000";
  let x = 0;
  let band = 0;
  let i = 0;

  const paint = (char: number): void => {
    const value = char - 63;
    for (let bit = 0; bit < 6; bit += 1) {
      if ((value >> bit) & 1) {
        const y = band + bit;
        if (y < height && x < width) pixels[y * width + x] = color;
      }
    }
    x += 1;
  };

  while (i < body.length) {
    const ch = body[i]!;
    if (ch === "#") {
      const m = /^#(\d+)(?:;2;(\d+);(\d+);(\d+))?/.exec(body.slice(i));
      if (!m) throw new Error("invalid color");
      if (m[2] !== undefined) {
        color = `#${[m[2], m[3], m[4]].map((v) => Number(v).toString(16).padStart(2, "0")).join("")}`;
        palette.set(Number(m[1]), color);
      } else {
        color = palette.get(Number(m[1])) ?? "#000000";
      }
      i += m[0].length;
      continue;
    }
    if (ch === "$") {
      x = 0;
      i += 1;
      continue;
    }
    if (ch === "-") {
      x = 0;
      band += 6;
      i += 1;
      continue;
    }
    if (ch === "!") {
      const m = /^!(\d+)(.)/s.exec(body.slice(i));
      if (!m) throw new Error("invalid rle");
      const count = Number(m[1]);
      const char = m[2]!.charCodeAt(0);
      for (let n = 0; n < count; n += 1) paint(char);
      i += m[0].length;
      continue;
    }
    paint(ch.charCodeAt(0));
    i += 1;
  }

  return { width, height, pixels };
}

function solidImage(width: number, height: number, color: [number, number, number]): RgbaImage {
  const data = Buffer.alloc(width * height * 4);
  for (let p = 0; p < width * height; p += 1) {
    data[p * 4] = color[0];
    data[p * 4 + 1] = color[1];
    data[p * 4 + 2] = color[2];
    data[p * 4 + 3] = 255;
  }
  return { width, height, data };
}

describe("rleEncode", () => {
  it("4 文字以上の連続だけを圧縮する", () => {
    expect(rleEncode(Buffer.from("aaabbbbbc", "latin1")).toString("latin1")).toBe("aaa!5bc");
  });
});

describe("encodeSixel", () => {
  it("ヘッダとラスタ属性を出力する", () => {
    const out = encodeSixel(solidImage(4, 6, [10, 20, 30]), 8);
    expect(out.toString("latin1").startsWith('\x1bP0;1;0q"1;1;4;6')).toBe(true);
    expect(out.toString("latin1").endsWith("\x1b\\")).toBe(true);
  });

  it("単色画像をその色で復元できる", () => {
    const out = encodeSixel(solidImage(10, 7, [200, 100, 50]), 4);
    const decoded = decodeSixel(out);
    expect(decoded.width).toBe(10);
    expect(decoded.height).toBe(7);
    expect(new Set(decoded.pixels)).toEqual(new Set(["#c86432"]));
  });

  it("4 色画像の配置を保つ", () => {
    const width = 8;
    const height = 8;
    const img = solidImage(width, height, [0, 0, 0]);
    const colors: [number, number, number][] = [
      [255, 0, 0],
      [0, 255, 0],
      [0, 0, 255],
      [255, 255, 255],
    ];
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        const color = colors[Math.floor(y / 4) * 2 + Math.floor(x / 4)]!;
        const offset = (y * width + x) * 4;
        img.data[offset] = color[0];
        img.data[offset + 1] = color[1];
        img.data[offset + 2] = color[2];
      }
    }
    const decoded = decodeSixel(encodeSixel(img, 16));
    const at = (x: number, y: number): string => decoded.pixels[y * width + x]!;
    expect(at(0, 0)).toBe("#ff0000");
    expect(at(7, 0)).toBe("#00ff00");
    expect(at(0, 7)).toBe("#0000ff");
    expect(at(7, 7)).toBe("#ffffff");
  });

  it("色数を指定してもそれ以上には増えない", () => {
    // グラデーション画像
    const width = 32;
    const height = 32;
    const img = solidImage(width, height, [0, 0, 0]);
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        const offset = (y * width + x) * 4;
        img.data[offset] = x * 8;
        img.data[offset + 1] = y * 8;
        img.data[offset + 2] = 128;
      }
    }
    const decoded = decodeSixel(encodeSixel(img, 8));
    expect(new Set(decoded.pixels).size).toBeLessThanOrEqual(8);
    expect(new Set(decoded.pixels).size).toBeGreaterThan(1);
  });
});
