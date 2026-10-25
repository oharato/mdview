/**
 * GFM テーブルの抽出と描画。
 *
 * mdcat（pulldown-cmark-mdcat）の表は列幅を「バイト数」で計算し、
 * パディングを「文字数」で行うため、CJK や絵文字を含む表が大きくずれる。
 * （pulldown-cmark-mdcat/src/render/write.rs の calculate_column_widths /
 *   format_table_cell）
 * ここでは表示幅（East Asian Width）を考慮して描画する。
 */

export type Alignment = "left" | "center" | "right";

export interface TableBlock {
  alignments: Alignment[];
  header: string[];
  rows: string[][];
}

const SENTINEL = (index: number): string => `@@mdview-table-${index}-@@`;
const DELIMITER_CELL = /^:?-{1,}:?$/;
const FENCE_RE = /^[ \t]*(```+|~~~+)/;

/** 表示幅 2 の範囲（East Asian Wide / Fullwidth、絵文字）。 */
const WIDE_RANGES: [number, number][] = [
  [0x1100, 0x115f],
  [0x2e80, 0x303e],
  [0x3041, 0x33ff],
  [0x3400, 0x4dbf],
  [0x4e00, 0x9fff],
  [0xa000, 0xa4cf],
  [0xa960, 0xa97f],
  [0xac00, 0xd7a3],
  [0xf900, 0xfaff],
  [0xfe10, 0xfe19],
  [0xfe30, 0xfe6f],
  [0xff00, 0xff60],
  [0xffe0, 0xffe6],
  [0x16fe0, 0x16fe4],
  [0x17000, 0x18cff],
  [0x1b000, 0x1b2ff],
  [0x1f004, 0x1f004],
  [0x1f0cf, 0x1f0cf],
  [0x1f18e, 0x1f18e],
  [0x1f191, 0x1f19a],
  [0x1f200, 0x1f320],
  [0x1f32d, 0x1f335],
  [0x1f337, 0x1f37c],
  [0x1f37e, 0x1f393],
  [0x1f3a0, 0x1f3ca],
  [0x1f3cf, 0x1f3d3],
  [0x1f3e0, 0x1f3f0],
  [0x1f3f4, 0x1f3f4],
  [0x1f3f8, 0x1f43e],
  [0x1f440, 0x1f440],
  [0x1f442, 0x1f4fc],
  [0x1f4ff, 0x1f53d],
  [0x1f54b, 0x1f54e],
  [0x1f550, 0x1f567],
  [0x1f57a, 0x1f57a],
  [0x1f595, 0x1f596],
  [0x1f5a4, 0x1f5a4],
  [0x1f5fb, 0x1f64f],
  [0x1f680, 0x1f6c5],
  [0x1f6cc, 0x1f6cc],
  [0x1f6d0, 0x1f6d2],
  [0x1f6d5, 0x1f6d7],
  [0x1f6eb, 0x1f6ec],
  [0x1f6f4, 0x1f6fc],
  [0x1f7e0, 0x1f7eb],
  [0x1f90c, 0x1f93a],
  [0x1f93c, 0x1f945],
  [0x1f947, 0x1f9ff],
  [0x1fa70, 0x1faff],
  [0x20000, 0x3fffd],
];

/** 表示幅 0 の文字（結合文字・書式制御・異体字セレクタ）。 */
function isZeroWidth(code: number): boolean {
  return (
    (code >= 0x0300 && code <= 0x036f) ||
    (code >= 0x1ab0 && code <= 0x1aff) ||
    (code >= 0x1dc0 && code <= 0x1dff) ||
    (code >= 0x20d0 && code <= 0x20f0) ||
    (code >= 0xfe00 && code <= 0xfe0f) ||
    (code >= 0xfe20 && code <= 0xfe2f) ||
    (code >= 0x200b && code <= 0x200f) ||
    code === 0xfeff
  );
}

/** 文字列の端末表示幅。 */
export function displayWidth(text: string): number {
  let width = 0;
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0;
    if (code === 0 || isZeroWidth(code)) continue;
    width += WIDE_RANGES.some(([start, end]) => code >= start && code <= end) ? 2 : 1;
  }
  return width;
}

function splitRow(line: string): string[] {
  let body = line.trim();
  if (body.startsWith("|")) body = body.slice(1);
  if (body.endsWith("|") && !body.endsWith("\\|")) body = body.slice(0, -1);

  const cells: string[] = [];
  let current = "";
  for (let i = 0; i < body.length; i += 1) {
    const char = body[i]!;
    if (char === "\\" && body[i + 1] === "|") {
      current += "|";
      i += 1;
    } else if (char === "|") {
      cells.push(current.trim());
      current = "";
    } else {
      current += char;
    }
  }
  cells.push(current.trim());
  return cells;
}

function parseAlignment(cell: string): Alignment {
  const left = cell.startsWith(":");
  const right = cell.endsWith(":");
  if (left && right) return "center";
  if (right) return "right";
  return "left";
}

function isDelimiterRow(line: string): boolean {
  if (!line.includes("-")) return false;
  const cells = splitRow(line);
  return cells.length > 0 && cells.every((cell) => DELIMITER_CELL.test(cell));
}

/**
 * GFM テーブルを抽出してプレースホルダ行に置き換える。
 * コードフェンス内は対象外。
 */
export function extractTables(text: string): { text: string; tables: TableBlock[] } {
  const lines = text.split("\n");
  const out: string[] = [];
  const tables: TableBlock[] = [];
  let inFence = false;
  let fenceMarker = "";

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!;

    const fence = FENCE_RE.exec(line);
    if (fence?.[1]) {
      const marker = fence[1][0]!;
      if (!inFence) {
        inFence = true;
        fenceMarker = marker;
      } else if (marker === fenceMarker) {
        inFence = false;
      }
      out.push(line);
      continue;
    }
    if (inFence) {
      out.push(line);
      continue;
    }

    const next = lines[i + 1];
    if (line.includes("|") && next !== undefined && isDelimiterRow(next)) {
      const header = splitRow(line);
      const alignments = splitRow(next).map(parseAlignment);
      const rows: string[][] = [];
      let j = i + 2;
      while (j < lines.length) {
        const candidate = lines[j]!;
        if (!candidate.includes("|") || candidate.trim() === "") break;
        const cells = splitRow(candidate);
        // 列数が合わない行は表の終わりとみなす
        if (cells.length !== header.length) break;
        rows.push(cells);
        j += 1;
      }

      out.push("", SENTINEL(tables.length), "");
      tables.push({ alignments, header, rows });
      i = j - 1;
      continue;
    }

    out.push(line);
  }

  return { text: out.join("\n"), tables };
}

export interface RenderOptions {
  maxWidth: number;
  colour: boolean;
}

const ANSI = {
  reset: "\x1b[0m",
  bold: "\x1b[1m",
  italic: "\x1b[3m",
  strike: "\x1b[9m",
  code: "\x1b[33m",
  rule: "\x1b[32m",
};

function stripInline(text: string): string {
  return text
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/`([^`]*)`/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/__([^_]+)__/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/_([^_]+)_/g, "$1")
    .replace(/~~([^~]+)~~/g, "$1");
}

/** セル内のインライン記法を ANSI に変換する（色が無効なら素のテキスト）。 */
export function formatInline(text: string, colour: boolean): string {
  if (!colour) return stripInline(text);
  let out = text;
  out = out.replace(/\[([^\]]*)\]\(([^)]*)\)/g, (_all, label: string, url: string) =>
    url ? `\x1b]8;;${url}\x1b\\${label}\x1b]8;;\x1b\\` : label,
  );
  out = out.replace(/`([^`]*)`/g, `${ANSI.code}$1${ANSI.reset}`);
  out = out.replace(/\*\*([^*]+)\*\*/g, `${ANSI.bold}$1${ANSI.reset}`);
  out = out.replace(/__([^_]+)__/g, `${ANSI.bold}$1${ANSI.reset}`);
  out = out.replace(/\*([^*]+)\*/g, `${ANSI.italic}$1${ANSI.reset}`);
  out = out.replace(/_([^_]+)_/g, `${ANSI.italic}$1${ANSI.reset}`);
  out = out.replace(/~~([^~]+)~~/g, `${ANSI.strike}$1${ANSI.reset}`);
  return out;
}

function truncate(text: string, width: number): string {
  const plain = stripInline(text);
  if (width <= 0) return "";
  if (displayWidth(plain) <= width) return plain;
  let out = "";
  let used = 0;
  for (const char of plain) {
    const w = displayWidth(char);
    if (used + w > width - 1) break;
    out += char;
    used += w;
  }
  return out + "…";
}

/** 表を描画する。 */
export function renderTable(table: TableBlock, options: RenderOptions): string {
  const columns = table.header.length;
  const header = [...table.header];
  while (header.length < columns) header.push("");
  const rows = table.rows.map((row) => {
    const padded = [...row];
    while (padded.length < columns) padded.push("");
    return padded.slice(0, columns);
  });

  const widths = new Array<number>(columns).fill(1);
  for (let c = 0; c < columns; c += 1) {
    widths[c] = Math.max(displayWidth(stripInline(header[c] ?? "")), 1);
    for (const row of rows) {
      widths[c] = Math.max(widths[c]!, displayWidth(stripInline(row[c] ?? "")));
    }
  }

  // 各セルは " text "、列間は 2 スペース
  const overhead = 4 * columns - 2;
  const totalWidth = (): number => widths.reduce((acc, w) => acc + w, 0) + overhead;
  while (totalWidth() > options.maxWidth) {
    let widest = -1;
    let width = 3;
    for (let c = 0; c < columns; c += 1) {
      if (widths[c]! > width) {
        width = widths[c]!;
        widest = c;
      }
    }
    if (widest < 0) break;
    widths[widest] = widths[widest]! - 1;
  }

  const ruleText = "─".repeat(Math.max(totalWidth(), 1));
  const rule = options.colour ? `${ANSI.rule}${ruleText}${ANSI.reset}` : ruleText;

  const renderRow = (cells: string[], bold: boolean): string =>
    cells
      .map((cell, c) => {
        const width = widths[c]!;
        const plain = stripInline(cell);
        const shown = displayWidth(plain) > width ? truncate(cell, width) : cell;
        const formatted = formatInline(shown, options.colour);
        const text = bold && options.colour ? `${ANSI.bold}${formatted}${ANSI.reset}` : formatted;

        const pad = Math.max(0, width - displayWidth(stripInline(shown)));
        const align = table.alignments[c] ?? "left";
        const left = align === "right" ? pad : align === "center" ? Math.floor(pad / 2) : 0;
        const right = pad - left;
        return ` ${" ".repeat(left)}${text}${" ".repeat(right)} `;
      })
      .join("  ");

  const lines: string[] = [rule, renderRow(header, true), rule];
  for (const row of rows) lines.push(renderRow(row, false));
  lines.push(rule);
  return lines.join("\n") + "\n";
}
