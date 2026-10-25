/** RGBA ピクセルバッファ。 */
export interface RgbaImage {
  width: number;
  height: number;
  data: Buffer;
}

interface Bin {
  key: number;
  r: number;
  g: number;
  b: number;
  count: number;
}

export interface Quantized {
  /** 3 バイト × 色数。 */
  palette: Uint8Array;
  /** 1 バイト × 画素数。 */
  indices: Uint8Array;
  colors: number;
}

function buildHistogram(img: RgbaImage): Bin[] {
  const hist = new Map<number, { sr: number; sg: number; sb: number; count: number }>();
  const d = img.data;
  for (let i = 0; i < d.length; i += 4) {
    const r = d[i]!;
    const g = d[i + 1]!;
    const b = d[i + 2]!;
    const key = ((r >> 3) << 10) | ((g >> 3) << 5) | (b >> 3);
    const entry = hist.get(key);
    if (entry) {
      entry.sr += r;
      entry.sg += g;
      entry.sb += b;
      entry.count += 1;
    } else {
      hist.set(key, { sr: r, sg: g, sb: b, count: 1 });
    }
  }

  const bins: Bin[] = [];
  for (const [key, e] of hist) {
    bins.push({
      key,
      r: e.sr / e.count,
      g: e.sg / e.count,
      b: e.sb / e.count,
      count: e.count,
    });
  }
  return bins;
}

function channelRange(bins: Bin[], channel: "r" | "g" | "b"): number {
  let min = Infinity;
  let max = -Infinity;
  for (const bin of bins) {
    const v = bin[channel];
    if (v < min) min = v;
    if (v > max) max = v;
  }
  return max - min;
}

function totalCount(bins: Bin[]): number {
  let n = 0;
  for (const bin of bins) n += bin.count;
  return n;
}

/**
 * メディアンカット法で減色する。ディザは行わない（端末の文字を滲ませないため）。
 */
export function quantize(img: RgbaImage, maxColors = 256): Quantized {
  const bins = buildHistogram(img);
  if (bins.length === 0) {
    return { palette: new Uint8Array([0, 0, 0]), indices: new Uint8Array(img.width * img.height), colors: 1 };
  }
  if (bins.length <= maxColors) {
    return mapToPalette(img, bins.map((bin) => [bin.r, bin.g, bin.b] as const));
  }

  let boxes: Bin[][] = [bins];
  while (boxes.length < maxColors) {
    let bestIndex = -1;
    let bestScore = -1;
    let bestChannel: "r" | "g" | "b" = "r";

    for (let i = 0; i < boxes.length; i += 1) {
      const box = boxes[i]!;
      if (box.length < 2) continue;
      const rRange = channelRange(box, "r");
      const gRange = channelRange(box, "g");
      const bRange = channelRange(box, "b");
      const range = Math.max(rRange, gRange, bRange);
      if (range <= 0) continue;
      const score = range * totalCount(box);
      if (score > bestScore) {
        bestScore = score;
        bestIndex = i;
        bestChannel = range === rRange ? "r" : range === gRange ? "g" : "b";
      }
    }
    if (bestIndex < 0) break;

    const box = boxes[bestIndex]!;
    box.sort((a, b) => a[bestChannel] - b[bestChannel]);
    const half = totalCount(box) / 2;
    let acc = 0;
    let splitAt = 1;
    for (let i = 0; i < box.length - 1; i += 1) {
      acc += box[i]!.count;
      if (acc >= half) {
        splitAt = i + 1;
        break;
      }
    }
    splitAt = Math.min(Math.max(1, splitAt), box.length - 1);
    const left = box.slice(0, splitAt);
    const right = box.slice(splitAt);
    boxes = [...boxes.slice(0, bestIndex), left, right, ...boxes.slice(bestIndex + 1)];
  }

  const palette = boxes.map((box) => {
    let sr = 0;
    let sg = 0;
    let sb = 0;
    let count = 0;
    for (const bin of box) {
      sr += bin.r * bin.count;
      sg += bin.g * bin.count;
      sb += bin.b * bin.count;
      count += bin.count;
    }
    return [sr / count, sg / count, sb / count] as const;
  });

  return mapToPalette(img, palette);
}

function mapToPalette(img: RgbaImage, palette: readonly (readonly [number, number, number])[]): Quantized {
  const colors = palette.length;
  const pal = new Uint8Array(colors * 3);
  palette.forEach(([r, g, b], i) => {
    pal[i * 3] = Math.round(r);
    pal[i * 3 + 1] = Math.round(g);
    pal[i * 3 + 2] = Math.round(b);
  });

  const cache = new Map<number, number>();
  const d = img.data;
  const out = new Uint8Array(img.width * img.height);

  for (let p = 0, i = 0; i < d.length; i += 4, p += 1) {
    const r = d[i]!;
    const g = d[i + 1]!;
    const b = d[i + 2]!;
    const key = ((r >> 3) << 10) | ((g >> 3) << 5) | (b >> 3);
    let idx = cache.get(key);
    if (idx === undefined) {
      let best = 0;
      let bestDist = Infinity;
      for (let c = 0; c < colors; c += 1) {
        const dr = r - pal[c * 3]!;
        const dg = g - pal[c * 3 + 1]!;
        const db = b - pal[c * 3 + 2]!;
        const dist = dr * dr + dg * dg + db * db;
        if (dist < bestDist) {
          bestDist = dist;
          best = c;
        }
      }
      idx = best;
      cache.set(key, idx);
    }
    out[p] = idx;
  }

  return { palette: pal, indices: out, colors };
}

/** 同じ文字の連続を `!<count><char>` に圧縮する（Sixel の RLE）。 */
export function rleEncode(chars: Buffer): Buffer {
  const parts: Buffer[] = [];
  let i = 0;
  while (i < chars.length) {
    const c = chars[i]!;
    let j = i + 1;
    while (j < chars.length && chars[j] === c) j += 1;
    const run = j - i;
    if (run >= 4) {
      parts.push(Buffer.from(`!${run}${String.fromCharCode(c)}`, "latin1"));
    } else if (run === 1) {
      parts.push(Buffer.from([c]));
    } else {
      parts.push(Buffer.alloc(run, c));
    }
    i = j;
  }
  return Buffer.concat(parts);
}

/**
 * RGBA 画像を Sixel 文字列に変換する。
 * 透過は事前に合成済みであることを前提とする。
 */
export function encodeSixel(img: RgbaImage, maxColors = 256): Buffer {
  const { width: W, height: H } = img;
  if (W <= 0 || H <= 0) return Buffer.alloc(0);

  const { palette, indices, colors } = quantize(img, Math.max(2, Math.min(256, maxColors)));

  const parts: Buffer[] = [Buffer.from(`\x1bP0;1;0q"1;1;${W};${H}`, "latin1")];

  for (let band = 0; band < H; band += 6) {
    const bitmaps = new Map<number, Uint8Array>();
    for (let dy = 0; dy < 6; dy += 1) {
      const y = band + dy;
      if (y >= H) break;
      const rowOffset = y * W;
      const bit = 1 << dy;
      for (let x = 0; x < W; x += 1) {
        const color = indices[rowOffset + x]!;
        let bits = bitmaps.get(color);
        if (!bits) {
          bits = new Uint8Array(W);
          bitmaps.set(color, bits);
        }
        bits[x] = (bits[x] ?? 0) | bit;
      }
    }

    for (const color of [...bitmaps.keys()].sort((a, b) => a - b)) {
      const bits = bitmaps.get(color)!;
      const chars = Buffer.allocUnsafe(W);
      for (let x = 0; x < W; x += 1) chars[x] = 63 + (bits[x] ?? 0);
      const r = palette[color * 3] ?? 0;
      const g = palette[color * 3 + 1] ?? 0;
      const b = palette[color * 3 + 2] ?? 0;
      parts.push(Buffer.from(`#${color};2;${r};${g};${b}`, "latin1"));
      parts.push(rleEncode(chars));
      parts.push(Buffer.from("$", "latin1"));
    }
    parts.push(Buffer.from("-", "latin1"));
  }

  parts.push(Buffer.from("\x1b\\", "latin1"));
  void colors;
  return Buffer.concat(parts);
}
