import { writeSync } from "node:fs";
import type { Heading } from "./sections.js";

/**
 * 出力先。
 * - StreamSink: そのまま端末へ流す（既定）
 * - BufferSink: ブロックとして溜めて、ページャで前後に移動できるようにする
 */

export type Block =
  | { kind: "text"; lines: string[] }
  | { kind: "image"; rows: number; emit: () => void };

export interface Sink {
  text(body: string): Promise<void>;
  image(rows: number, emit: () => void): Promise<void>;
}

export function writeBytes(text: string | Buffer): void {
  const buf = typeof text === "string" ? Buffer.from(text, "utf8") : text;
  let offset = 0;
  while (offset < buf.length) {
    try {
      offset += writeSync(1, buf, offset, buf.length - offset);
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code === "EAGAIN") continue;
      if (code === "EPIPE") process.exit(0);
      throw error;
    }
  }
}

/** mdcat の見出し行（`┄` マーカー付き）か。ANSI は無視する。 */
export function isHeadingLine(line: string): boolean {
  const plain = line.replace(/\x1b\[[0-9;]*[A-Za-z]/g, "");
  return /^┄/.test(plain);
}

export interface HeadingMark {
  index: number;
  level: number;
  text: string;
  block: number;
  line: number;
}

export class StreamSink implements Sink {
  async text(body: string): Promise<void> {
    writeBytes(body);
  }

  async image(_rows: number, emit: () => void): Promise<void> {
    emit();
  }
}

export class BufferSink implements Sink {
  readonly blocks: Block[] = [];
  readonly marks: HeadingMark[] = [];

  private expected: Heading[] = [];

  /** これから描画される文書の見出しを登録する（順番に突き合わせる）。 */
  expectHeadings(headings: Heading[]): void {
    this.expected.push(...headings);
  }

  async text(body: string): Promise<void> {
    if (body === "") return;
    const lines = body.split("\n");
    if (lines.at(-1) === "") lines.pop();
    if (lines.length === 0) return;

    const block = this.blocks.length;
    lines.forEach((line, index) => {
      if (!isHeadingLine(line)) return;
      const heading = this.expected.shift();
      if (heading) this.marks.push({ ...heading, block, line: index });
    });

    this.blocks.push({ kind: "text", lines });
  }

  async image(rows: number, emit: () => void): Promise<void> {
    this.blocks.push({ kind: "image", rows, emit });
  }
}

export type Entry =
  | { kind: "text"; line: string; block: number; line_: number }
  | { kind: "image"; rows: number; emit: () => void };

/** ブロックを 1 ページ分ずつに分割する。 */
export function paginate(blocks: Block[], bodyRows: number): Entry[][] {
  const pages: Entry[][] = [];
  let page: Entry[] = [];
  let used = 0;

  const flush = (): void => {
    if (page.length > 0) pages.push(page);
    page = [];
    used = 0;
  };

  blocks.forEach((block, blockIndex) => {
    if (block.kind === "text") {
      block.lines.forEach((line, lineIndex) => {
        if (used >= bodyRows) flush();
        page.push({ kind: "text", line, block: blockIndex, line_: lineIndex });
        used += 1;
      });
      return;
    }
    const rows = Math.min(block.rows, bodyRows);
    if (used > 0 && used + rows > bodyRows) flush();
    page.push({ kind: "image", rows, emit: block.emit });
    used += rows;
  });

  flush();
  return pages;
}

/** 受信したチャンクを 1 キーずつに分解する（矢印キーなどはまとめて 1 つ）。 */
export function takeKeys(buffer: string): string[] {
  const keys: string[] = [];
  let i = 0;
  while (i < buffer.length) {
    const char = buffer[i]!;
    if (char === "\x1b") {
      const match = /^\x1b\[[0-9;]*[A-Za-z~]/.exec(buffer.slice(i));
      if (match) {
        keys.push(match[0]);
        i += match[0].length;
        continue;
      }
      keys.push("\x1b");
      i += 1;
      continue;
    }
    const code = buffer.codePointAt(i) ?? 0;
    const key = String.fromCodePoint(code);
    keys.push(key);
    i += key.length;
  }
  return keys;
}

const HELP = "space:次へ b:前へ ]/[ 見出し t:一覧 g/G:先頭/末尾 q:終了";

function stripAnsi(text: string): string {
  return text.replace(/\x1b\[[0-9;]*[A-Za-z]/g, "");
}

export interface PagerOptions {
  rows: number;
  columns: number;
}

/** 画面を保持して前後に移動できるページャ（TOC・セクションジャンプ付き）。 */
export async function runPager(
  blocks: Block[],
  marks: HeadingMark[],
  options: PagerOptions,
): Promise<void> {
  let rows = Math.max(5, options.rows);
  let columns = Math.max(20, options.columns);

  let pages = paginate(blocks, rows - 1);
  if (pages.length === 0) pages = [[{ kind: "text", line: "", block: 0, line_: 0 }]];

  const pageOf = new Map<string, number>();
  pages.forEach((page, index) => {
    for (const entry of page) {
      if (entry.kind === "text") pageOf.set(`${entry.block}:${entry.line_}`, index);
    }
  });
  const headingPages = marks.map((mark) => ({
    ...mark,
    page: pageOf.get(`${mark.block}:${mark.line}`) ?? 0,
  }));

  let index = 0;
  let mode: "page" | "toc" = "page";
  let tocIndex = 0;

  const currentHeading = (): HeadingMark | undefined => {
    let found: HeadingMark | undefined;
    for (const heading of headingPages) {
      if (heading.page <= index) found = heading;
      else break;
    }
    return found;
  };

  const drawPage = (): void => {
    const page = pages[index] ?? [];
    writeBytes("\x1b[H\x1b[2J");
    let row = 1;
    for (const entry of page) {
      if (entry.kind === "text") {
        writeBytes(`\x1b[${row};1H${entry.line}`);
        row += 1;
      } else {
        writeBytes(`\x1b[${row};1H`);
        entry.emit();
        row += entry.rows;
      }
    }
    const heading = currentHeading();
    const label = heading ? `§ ${stripAnsi(heading.text)}` : "";
    const status = ` ${index + 1}/${pages.length}  ${label}  ${HELP}`;
    writeBytes(`\x1b[${rows};1H\x1b[7m${status.slice(0, columns).padEnd(columns)}\x1b[0m`);
  };

  const drawToc = (): void => {
    const visible = rows - 2;
    const offset = Math.max(0, Math.min(tocIndex - Math.floor(visible / 2), headingPages.length - visible));
    writeBytes("\x1b[H\x1b[2J");
    writeBytes(`\x1b[1m見出し一覧\x1b[0m  (j/k: 移動, Enter: ジャンプ, t/Esc: 戻る)`);
    for (let i = 0; i < visible; i += 1) {
      const heading = headingPages[offset + i];
      if (!heading) break;
      const selected = offset + i === tocIndex;
      const indent = "  ".repeat(Math.max(0, heading.level - 1));
      const page = `p.${heading.page + 1}`;
      const text = ` ${String(heading.index).padStart(3)} ${indent}${stripAnsi(heading.text)}`;
      const line = `${text.padEnd(Math.max(1, columns - page.length - 1))}${page} `;
      writeBytes(`\x1b[${i + 2};1H${selected ? `\x1b[7m${line}\x1b[0m` : line}`);
    }
    writeBytes(`\x1b[${rows};1H\x1b[7m${"".padEnd(columns)}\x1b[0m`);
  };

  const draw = (): void => {
    if (mode === "toc") drawToc();
    else drawPage();
  };

  const stdin = process.stdin;

  // 常時リスナーを張ってキーを取りこぼさない（入力が無い間もプロセスを維持する）
  const queue: string[] = [];
  let waiter: ((key: string) => void) | null = null;
  const deliver = (key: string): void => {
    if (waiter) {
      const resolve = waiter;
      waiter = null;
      resolve(key);
    } else {
      queue.push(key);
    }
  };
  const onData = (buf: Buffer): void => {
    for (const key of takeKeys(buf.toString("utf8"))) deliver(key);
  };
  const onEnd = (): void => deliver("q");

  const nextKey = async (): Promise<string> => {
    const queued = queue.shift();
    if (queued !== undefined) return queued;
    return await new Promise<string>((resolve) => {
      waiter = resolve;
    });
  };

  stdin.setRawMode(true);
  stdin.resume();
  stdin.on("data", onData);
  stdin.on("end", onEnd);
  writeBytes("\x1b[?1049h\x1b[?25l");

  const onResize = (): void => {
    const nextRows = process.stdout.rows ?? 0;
    const nextCols = process.stdout.columns ?? 0;
    if (nextRows >= 5) rows = nextRows;
    if (nextCols >= 20) columns = nextCols;
    pages = paginate(blocks, rows - 1);
    index = Math.min(index, pages.length - 1);
    draw();
  };
  process.stdout.on("resize", onResize);

  try {
    draw();
    for (;;) {
      const key = await nextKey();

      if (mode === "toc") {
        if (key === "j" || key === "\u001b[B") {
          tocIndex = Math.min(headingPages.length - 1, tocIndex + 1);
        } else if (key === "k" || key === "\u001b[A") {
          tocIndex = Math.max(0, tocIndex - 1);
        } else if (key === "\r" || key === "\n") {
          index = headingPages[tocIndex]?.page ?? index;
          mode = "page";
        } else if (key === "t" || key === "q" || key === "\u001b" || key === "\u0003") {
          mode = "page";
        } else {
          continue;
        }
        draw();
        continue;
      }

      if (key === "q" || key === "Q" || key === "\u0003" || key === "\u001b") break;
      if (key === "t") {
        mode = "toc";
        tocIndex = Math.max(0, headingPages.findIndex((heading) => heading.page >= index));
        draw();
        continue;
      }
      if ([" ", "\r", "\n", "j", "f", "\u001b[B", "\u001b[6~"].includes(key)) {
        if (index < pages.length - 1) index += 1;
      } else if (["b", "k", "\u001b[A", "\u001b[5~"].includes(key)) {
        if (index > 0) index -= 1;
      } else if (key === "]") {
        const next = headingPages.find((heading) => heading.page > index);
        if (next) index = next.page;
      } else if (key === "[") {
        const prev = [...headingPages].reverse().find((heading) => heading.page < index);
        if (prev) index = prev.page;
      } else if (key === "g") {
        index = 0;
      } else if (key === "G") {
        index = pages.length - 1;
      } else {
        continue;
      }
      draw();
    }
  } finally {
    process.stdout.off("resize", onResize);
    stdin.off("data", onData);
    stdin.off("end", onEnd);
    writeBytes("\x1b[?25h\x1b[?1049l");
    try {
      stdin.setRawMode(false);
    } catch {
      /* ignore */
    }
    stdin.pause();
  }
}
