#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  statSync,
  writeFileSync,
  writeSync,
} from "node:fs";
import { readFile } from "node:fs/promises";
import { basename, dirname, extname, isAbsolute, resolve } from "node:path";
import { parseArgs } from "node:util";
import { planHalfBlocks, renderHalfBlocks, type ColorMode } from "./blocks.js";
import {
  flattenImage,
  isSvg,
  loadRaster,
  loadSvg,
  parseBackground,
  planDisplaySize,
  resizeImage,
  toPng,
} from "./images.js";
import { extractImages, extractMermaid, splitSegments, type ImageBlock, type MermaidBlock } from "./markdown.js";
import { mermaidToPng, type MermaidTheme } from "./mermaid.js";
import { extractTables, renderTable } from "./tables.js";
import { mermaidCacheDir, remoteCacheDir, shortHash } from "./paths.js";
import { encodeSixel } from "./sixel.js";
import { encodeIterm2, detectIterm2 } from "./iterm.js";
import { encodeKitty } from "./kitty.js";
import { BufferSink, StreamSink, runPager, type Sink } from "./pager.js";
import { extractSection, listHeadings } from "./sections.js";
import { checkForUpdate, currentVersion, describeInstall, runSelfUpdate } from "./update.js";
import {
  cellPixels,
  isInteractive,
  isWezTerm,
  kittySupported,
  probeTerminal,
  sixelSupported,
  terminalName,
  windowPixels,
} from "./term.js";

const VERSION = currentVersion();
const DIM = "\x1b[2m";
const RESET = "\x1b[0m";

type Renderer = "auto" | "kitty" | "sixel" | "iterm" | "blocks";
type ThemeSetting = "auto" | MermaidTheme;

interface RenderContext {
  sink: Sink;
  /** ページャ用に溜めているか（画像の配置方法が変わる）。 */
  pageMode: boolean;
}

interface Options {
  files: string[];
  width: number;
  imageWidth: number;
  renderer: Renderer;
  colorMode: ColorMode;
  imageColors: number;
  imageBg: string;
  pixelScale: number;
  upscale: boolean;
  light: boolean;
  noColour: boolean;
  mermaidTheme: ThemeSetting;
  mermaidWidth: number;
  check: boolean;
  refresh: boolean;
  update: boolean;
  checkUpdate: boolean;
  paginate: boolean;
  toc: boolean;
  section: number | null;
}

const HELP = `mdview ${VERSION} — mdcat + 端末内画像/Mermaid 描画

使い方:
  mdview [options] [files...]

オプション:
  -w, --width N             テキストの最大桁数 (既定: 端末幅, 最大 120)
  -i, --image-width N       画像・図の横幅 (端末桁数, 既定: 端末幅いっぱい)
  -r, --renderer MODE       auto | kitty | sixel | iterm | blocks (既定: auto)
      --colours MODE        blocks 描画時の色数: truecolor | 256
      --image-colours N     sixel 描画時の色数 (既定: 256)
      --image-bg SPEC       透過画像の背景: auto | none | #RRGGBB
      --pixel-scale F       セルサイズを取得できない端末での倍率 (HiDPI なら 2)
      --[no-]upscale        小さい画像を端末幅まで拡大 (既定: 有効, 最大3倍)
      --light               明るい背景の端末として扱う
  -c, --no-colour           色・画像を無効化
  -p, --paginate            ページャで表示（space:次へ b:前へ t:見出し一覧 q:終了）
      --toc                 見出しの一覧を表示して終了
      --section N           N 番目の見出しのセクションだけを表示
      --mermaid-theme T     auto | default | dark | neutral
      --mermaid-width N     Mermaid のラスタライズ幅 px (既定: 2048)
      --check               端末・依存の対応状況を表示して終了
      --refresh             端末判定をやり直す
      --check-update        新しいバージョンがあるか確認
      --update              最新バージョンへ自己アップデート
  -V, --version             バージョン表示
  -h, --help                このヘルプ

環境変数:
  MDVIEW_SIXEL=0|1    Sixel の強制
  MDVIEW_CELL=18x36   セルサイズの上書き
  MDVIEW_CACHE=DIR    キャッシュ先 (既定: ~/.cache/mdview)
`;

function getStdoutSize(): { columns: number; rows: number } {
  const columns = process.stdout.columns ?? 0;
  const rows = process.stdout.rows ?? 0;
  return {
    columns: columns >= 20 ? columns : 100,
    rows: rows >= 5 ? rows : 40,
  };
}

/** fd 1 へ確実に書き込む（バッファリングによる順序の乱れを避ける）。 */
function writeOut(data: string | Buffer): void {
  const buf = typeof data === "string" ? Buffer.from(data, "utf8") : data;
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

function warn(message: string): void {
  writeOut(`${DIM}[mdview] ${message}${RESET}\n`);
}

function fail(message: string): never {
  writeOut(`mdview: ${message}\n`);
  process.exit(1);
}

function detectLightBackground(): boolean {
  const fgbg = process.env.COLORFGBG ?? "";
  const parts = fgbg.split(";");
  const last = Number(parts.at(-1));
  return Number.isFinite(last) && last >= 8;
}

function parseOptions(argv: string[]): Options {
  let parsed;
  try {
    parsed = parseArgs({
      args: argv,
      allowPositionals: true,
      options: {
        width: { type: "string", short: "w" },
        "image-width": { type: "string", short: "i" },
        renderer: { type: "string", short: "r" },
        colours: { type: "string" },
        "image-colours": { type: "string" },
        "image-bg": { type: "string" },
        "pixel-scale": { type: "string" },
        upscale: { type: "boolean" },
        "no-upscale": { type: "boolean" },
        light: { type: "boolean" },
        "no-colour": { type: "boolean", short: "c" },
        "mermaid-theme": { type: "string" },
        "mermaid-width": { type: "string" },
        check: { type: "boolean" },
        refresh: { type: "boolean" },
        "check-update": { type: "boolean" },
        update: { type: "boolean" },
        paginate: { type: "boolean", short: "p" },
        toc: { type: "boolean" },
        section: { type: "string" },
        version: { type: "boolean", short: "V" },
        help: { type: "boolean", short: "h" },
      },
    });
  } catch (error) {
    fail(`${(error as Error).message}\n\n${HELP}`);
  }

  const v = parsed.values;
  if (v.help) {
    writeOut(HELP);
    process.exit(0);
  }
  if (v.version) {
    writeOut(`mdview ${VERSION}\n`);
    process.exit(0);
  }

  const { columns } = getStdoutSize();
  const num = (raw: string | undefined, fallback: number): number => {
    if (raw === undefined) return fallback;
    const n = Number.parseInt(raw, 10);
    return Number.isFinite(n) ? n : fallback;
  };
  const float = (raw: string | undefined, fallback: number): number => {
    if (raw === undefined) return fallback;
    const n = Number.parseFloat(raw);
    return Number.isFinite(n) ? n : fallback;
  };
  const pick = <T extends string>(raw: string | undefined, allowed: readonly T[], fallback: T): T => {
    if (raw === undefined) return fallback;
    if ((allowed as readonly string[]).includes(raw)) return raw as T;
    fail(`不正な値です: ${raw} (${allowed.join(" | ")})`);
  };

  const renderer = pick(v.renderer, ["auto", "kitty", "sixel", "iterm", "blocks"] as const, "auto");
  const colorMode = pick(v.colours, ["truecolor", "256"] as const, "truecolor");
  const mermaidTheme = pick(
    v["mermaid-theme"],
    ["auto", "default", "dark", "neutral"] as const,
    "auto",
  );

  const upscale = v["no-upscale"] ? false : (v.upscale ?? true);

  return {
    files: parsed.positionals.length > 0 ? parsed.positionals : ["-"],
    width: Math.min(num(v.width, Math.min(columns, 120)), 400),
    imageWidth: num(v["image-width"], Math.min(Math.max(columns - 2, 10), 200)),
    renderer,
    colorMode,
    imageColors: num(v["image-colours"], 256),
    imageBg: v["image-bg"] ?? "auto",
    pixelScale: float(v["pixel-scale"], 1),
    upscale,
    light: v.light ?? detectLightBackground(),
    noColour: v["no-colour"] ?? false,
    mermaidTheme,
    mermaidWidth: num(v["mermaid-width"], 2048),
    check: v.check ?? false,
    refresh: v.refresh ?? false,
    update: v.update ?? false,
    checkUpdate: v["check-update"] ?? false,
    paginate: v.paginate ?? false,
    toc: v.toc ?? false,
    section: v.section === undefined ? null : Number.parseInt(v.section, 10),
  };
}

async function runMdcat(chunk: string, options: Options, ctx: RenderContext): Promise<void> {
  const mdcat = process.env.MDVIEW_MDCAT ?? "mdcat";
  const args = ["--columns", String(options.width)];
  if (options.noColour) args.push("--no-colour");
  else args.push("--ansi");

  if (!ctx.pageMode) {
    const result = spawnSync(mdcat, args, { input: chunk, stdio: ["pipe", "inherit", "inherit"] });
    if (result.error || result.status !== 0) writeOut(chunk);
    return;
  }

  const result = spawnSync(mdcat, args, { input: chunk, encoding: "utf8" });
  if (result.error || result.status !== 0) await ctx.sink.text(chunk);
  else await ctx.sink.text(result.stdout ?? "");
}

async function resolveTheme(options: Options): Promise<MermaidTheme> {
  if (options.mermaidTheme === "auto") return options.light ? "default" : "dark";
  return options.mermaidTheme;
}

function mermaidCachePath(code: string, theme: MermaidTheme, width: number): string {
  const key = shortHash(code, theme, String(width));
  const dir = mermaidCacheDir();
  mkdirSync(dir, { recursive: true });
  return resolve(dir, `${key}.png`);
}

async function renderMermaid(block: MermaidBlock, options: Options, ctx: RenderContext): Promise<void> {
  const theme = await resolveTheme(options);
  const cachePath = mermaidCachePath(block.code, theme, options.mermaidWidth);

  if (!existsSync(cachePath)) {
    try {
      const png = mermaidToPng(block.code, theme, options.mermaidWidth);
      writeFileSync(cachePath, png);
    } catch (error) {
      warn(`Mermaid を描画できません: ${(error as Error).message}`);
      await runMdcat("```mermaid\n" + block.code + "```\n", options, ctx);
      return;
    }
  }

  if (existsSync(cachePath)) await showImage(cachePath, "mermaid", options, ctx);
}

async function resolveImageSource(src: string, baseDir: string): Promise<string | null> {
  if (/^https?:\/\//i.test(src)) {
    const ext = extname(src.split("?")[0] ?? "") || ".img";
    const dest = resolve(remoteCacheDir(), `${shortHash(src)}${ext}`);
    if (existsSync(dest)) return dest;
    mkdirSync(remoteCacheDir(), { recursive: true });
    try {
      const response = await fetch(src, { signal: AbortSignal.timeout(30_000) });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      writeFileSync(dest, Buffer.from(await response.arrayBuffer()));
      return dest;
    } catch (error) {
      warn(`画像の取得に失敗: ${src} (${(error as Error).message})`);
      return null;
    }
  }

  const path = isAbsolute(src) ? src : resolve(baseDir, src);
  if (!existsSync(path)) {
    warn(`画像が見つかりません: ${src}`);
    return null;
  }
  return path;
}

async function renderImage(block: ImageBlock, baseDir: string, options: Options, ctx: RenderContext): Promise<void> {
  const source = await resolveImageSource(block.src, baseDir);
  if (!source) {
    writeOut(`${DIM}[画像: ${block.alt || block.src}]${RESET}\n`);
    return;
  }
  await showImage(source, block.alt, options, ctx);
}

let warnedKittyTransfer = false;

/**
 * Kitty の画像 ID。
 * 省略すると WezTerm は ID 0 を使い回すため、複数画像が後勝ちで置き換わる。
 * 出現ごとに一意な ID を払い出す。
 */
let kittyImageId = 1000 + Math.floor(Math.random() * 100_000);

function nextKittyImageId(): number {
  kittyImageId += 1;
  if (kittyImageId > 0xfffffffe) kittyImageId = 1001;
  return kittyImageId;
}

async function chooseRenderer(options: Options): Promise<Exclude<Renderer, "auto">> {
  if (options.renderer !== "auto") return options.renderer;
  if (await kittySupported()) return "kitty";

  const probe = await probeTerminal();
  if (
    !options.check &&
    probe.kitty === true &&
    probe.kittyTransfer === false &&
    (await isWezTerm()) &&
    !warnedKittyTransfer
  ) {
    warnedKittyTransfer = true;
    warn(
      "kitty graphics の画像転送を確認できませんでした（-r blocks で半ブロック表示になります）。" +
        "接続経路によっては描画されないことがあります: wezterm ssh / -r iterm もお試しください",
    );
  }

  if (await isWezTerm()) {
    // WezTerm は iTerm2 プロトコルも話せる。ただし Windows の ssh.exe (ConPTY) 経由では
    // 画像自体が描画されないことがあるため、kitty が使えない場合は半ブロックにする。
    return detectIterm2() ? "iterm" : "blocks";
  }
  if (await sixelSupported()) return "sixel";
  if (detectIterm2()) return "iterm";
  return "blocks";
}

async function showImage(path: string, alt: string, options: Options, ctx: RenderContext): Promise<void> {
  if (options.noColour) {
    writeOut(`[画像: ${alt || basename(path)}]\n`);
    return;
  }

  if (isSvg(path)) {
    try {
      const image = await loadSvg(path, options.mermaidWidth, await resolveTheme(options));
      await drawImage(image, options, ctx);
    } catch (error) {
      warn(`SVG を描画できません: ${path} (${(error as Error).message})`);
    }
    return;
  }

  let image;
  try {
    image = await loadRaster(path);
  } catch (error) {
    warn(`画像を開けません: ${path} (${(error as Error).message})`);
    return;
  }
  await drawImage(image, options, ctx);
}

async function drawImage(
  image: { width: number; height: number; data: Buffer },
  options: Options,
  ctx: RenderContext,
): Promise<void> {
  const background = parseBackground(options.imageBg, options.light);
  if (background) image = await flattenImage(image, background);

  const renderer = await chooseRenderer(options);
  if (renderer === "sixel" || renderer === "kitty" || renderer === "iterm") {
    const { rows } = getStdoutSize();
    const [width, height] = await planDisplaySize(image.width, image.height, {
      imageWidthCols: options.imageWidth,
      pixelScale: options.pixelScale,
      upscale: options.upscale,
      terminalRows: rows,
    });
    const resized = await resizeImage(image, width, height);

    if (renderer === "sixel") {
      const [, cellHeight] = await cellPixels();
      const rows = Math.max(1, Math.ceil(resized.height / cellHeight));
      await ctx.sink.image(rows, () => {
        writeOut(encodeSixel(resized, options.imageColors));
        if (!ctx.pageMode) writeOut("\n");
      });
      return;
    }

    const png = await toPng(resized);
    if (renderer === "kitty") {
      const [cellWidth, cellHeight] = await cellPixels();
      const cols = Math.max(1, Math.round(resized.width / cellWidth));
      const rows = Math.max(1, Math.ceil(resized.height / cellHeight));
      const image = encodeKitty(png, {
        cols,
        rows,
        imageId: nextKittyImageId(),
        holdCursor: true,
      });

      if (ctx.pageMode) {
        // ページャは行位置を明示して描画するので、そのまま貼るだけでよい
        await ctx.sink.image(rows, () => writeOut(image));
        return;
      }

      // C=1（カーソル固定）で貼ると、WezTerm は「カーソルより下に残っている行」までしか
      // 画像を貼らない（term/src/terminalstate/image.rs の height_in_cells の上限）。
      // 画面下端で実行すると数行しか貼られず見えなくなるため、先に空行を確保して
      // スクロールさせ、その領域に貼ってからカーソルを進める。
      await ctx.sink.image(rows, () => {
        writeOut("\r" + "\n".repeat(rows) + `\x1b[${rows}A`);
        writeOut(image);
        writeOut(`\x1b[${rows}B`);
      });
      return;
    }

    await ctx.sink.image(
      Math.max(1, Math.ceil(resized.height / (await cellPixels())[1])),
      () => writeOut(encodeIterm2(png)),
    );
    return;
  }

  const { rows } = getStdoutSize();
  const plan = planHalfBlocks(image.width, image.height, options.imageWidth, Math.max(4, rows - 3));
  const resized = await resizeImage(image, plan.cols, plan.rows * 2);
  const rendered = renderHalfBlocks(resized, options.colorMode);
  await ctx.sink.text(rendered);
}

async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) chunks.push(Buffer.from(chunk as Buffer));
  return Buffer.concat(chunks).toString("utf8");
}

/** `--toc`: 見出しの一覧を表示する。 */
async function printToc(options: Options): Promise<void> {
  for (const name of options.files) {
    const text = name === "-" ? await readStdin() : await readFile(resolve(name), "utf8");
    const headings = listHeadings(text);
    if (options.files.length > 1) writeOut(`\n${name}\n`);
    if (headings.length === 0) {
      writeOut("(見出しがありません)\n");
      continue;
    }
    for (const heading of headings) {
      const indent = "  ".repeat(heading.level - 1);
      writeOut(`${String(heading.index).padStart(3)}  ${indent}${heading.text}\n`);
    }
  }
}

async function processDocument(
  text: string,
  baseDir: string,
  options: Options,
  ctx: RenderContext,
): Promise<void> {
  const mermaid = extractMermaid(text);
  const images = extractImages(mermaid.text, mermaid.blocks.length);
  const tables = extractTables(images.text);
  const assets: (MermaidBlock | ImageBlock)[] = [...mermaid.blocks, ...images.blocks];

  for (const segment of splitSegments(tables.text)) {
    if (segment.kind === "text") {
      await runMdcat(segment.body, options, ctx);
      continue;
    }
    if (segment.kind === "table") {
      const table = tables.tables[segment.index];
      if (table) {
        await ctx.sink.text(
          renderTable(table, { maxWidth: options.width, colour: !options.noColour }) + "\n",
        );
      }
      continue;
    }
    const asset = assets[segment.index];
    if (!asset) continue;
    if ("code" in asset) await renderMermaid(asset, options, ctx);
    else await renderImage(asset, baseDir, options, ctx);
  }
}

/** install.sh が導入する mdcat のバージョン（mdcat.version）。 */
function recommendedMdcatVersion(): string | null {
  try {
    const version = readFileSync(resolve(import.meta.dirname, "..", "mdcat.version"), "utf8").trim();
    return version || null;
  } catch {
    return null;
  }
}

function commandVersion(command: string): string | null {
  const result = spawnSync(command, ["--version"], { encoding: "utf8", timeout: 5000 });
  if (result.error || result.status !== 0) return null;
  const line = `${result.stdout ?? ""}${result.stderr ?? ""}`.trim().split("\n")[0] ?? "";
  return line || "found";
}

/** セルフテストの結果を文字列にする。 */
async function kittyTransferStatus(): Promise<string> {
  const probe = await probeTerminal();
  if (probe.kitty !== true) return "n/a (kitty 未対応)";
  if (probe.kittyTransfer === true) return "ok";
  if (probe.kittyTransfer === false) return "失敗 (画像データが届かない)";
  return "未検証";
}

async function printCheck(options: Options): Promise<void> {
  const sixel = await sixelSupported(options.refresh);
  const kitty = await kittySupported();
  const probe = await probeTerminal();
  const [cellWidth, cellHeight] = await cellPixels();
  const window = await windowPixels();
  const renderer = await chooseRenderer(options);
  const { columns } = getStdoutSize();
  const mdcat = process.env.MDVIEW_MDCAT ?? "mdcat";
  const recommended = recommendedMdcatVersion();
  const lines: [string, string][] = [
    ["mdview", VERSION],
    ["node", process.version],
    ["mdcat", `${commandVersion(mdcat) ?? "見つかりません"}${recommended ? ` (推奨 ${recommended})` : ""}`],
    ["pele", "built-in"],
    ["resvg", "built-in (@resvg/resvg-js)"],
    ["TERM", process.env.TERM ?? ""],
    ["terminal", (await terminalName()) || "不明 (XTVERSION 未応答)"],
    ["columns", String(columns)],
    ["cell pixels", `${cellWidth}x${cellHeight}`],
    ["window pixels", window ? `${window[0]}x${window[1]}` : "不明"],
    ["interactive", String(isInteractive())],
    ["kitty support", probe.kitty === null ? "no response" : probe.kitty ? "yes" : "no"],
    ["kitty transfer", await kittyTransferStatus()],
    ["kitty usable", kitty ? "yes" : "no"],
    ["sixel support", sixel ? "yes" : "no"],
    ["iterm2 support", detectIterm2() ? "yes" : "no"],
    ["renderer", renderer],
    ["image width", `${options.imageWidth} cols`],
  ];
  for (const [label, value] of lines) {
    writeOut(`${label.padEnd(14)}: ${value}\n`);
  }

  if (recommended) {
    const installed = /(\d+\.\d+\.\d+)/.exec(commandVersion(mdcat) ?? "")?.[1];
    if (installed && installed !== recommended) {
      writeOut(
        `\nhint: mdcat は ${installed} ですが推奨は ${recommended} です。` +
          `\n      MDCAT_VERSION=${recommended} ./install.sh  で更新できます。\n`,
      );
    }
  }

  if (probe.kitty === true && probe.kittyTransfer === false && (await isWezTerm())) {
    writeOut(
      "\n" +
        "hint: 端末は kitty graphics を話しますが、画像データの転送を確認できませんでした。\n" +
        "      - wezterm ssh <user>@<host>  で接続し直す（ConPTY を迂回）\n" +
        "      - mdview -r iterm <file>     OSC 1337 経由を試す\n" +
        "      - mdview -r blocks <file>    半ブロックで表示（現状のフォールバック）\n",
    );
  }
}

async function main(): Promise<void> {
  const options = parseOptions(process.argv.slice(2));

  if (options.checkUpdate) {
    const { current, latest, hasUpdate, install } = await checkForUpdate();
    writeOut(`mdview        : ${current}\n`);
    writeOut(`install       : ${describeInstall(install)}\n`);
    if (!latest) {
      writeOut("latest        : 取得できませんでした（リリース未作成、またはネットワーク）\n");
      return;
    }
    writeOut(`latest        : ${latest.version}\n`);
    writeOut(`status        : ${hasUpdate ? "更新があります" : "最新です"}\n`);
    if (hasUpdate) writeOut(`\n  mdview --update  で更新できます (${latest.url})\n`);
    return;
  }

  if (options.update) {
    const result = await runSelfUpdate();
    writeOut(`${result.ok ? "" : "mdview: "}${result.message}\n`);
    if (!result.ok) process.exit(1);
    return;
  }

  if (options.check) {
    await printCheck(options);
    return;
  }

  if (isInteractive() && !options.noColour) {
    // 端末判定を先に済ませる（後続の出力と混ざらないように）。
    await chooseRenderer(options);
  }

  if (options.toc) {
    await printToc(options);
    return;
  }

  if (options.section !== null && !Number.isInteger(options.section)) {
    fail("--section には番号を指定してください（--toc で一覧を表示できます）");
  }

  const rows = process.stdout.rows ?? 0;
  const usePager = options.paginate && isInteractive() && Boolean(process.stdin.isTTY);
  const sink = usePager ? new BufferSink() : new StreamSink();
  const ctx: RenderContext = { sink, pageMode: usePager };

  for (const name of options.files) {
    const baseDir = name === "-" ? process.cwd() : dirname(resolve(name));
    let text: string;

    if (name === "-") {
      text = await readStdin();
    } else {
      const path = resolve(name);
      if (!existsSync(path)) fail(`ファイルがありません: ${name}`);
      if (statSync(path).isDirectory()) fail(`ディレクトリは指定できません: ${name}`);
      text = await readFile(path, "utf8");
    }

    if (options.section !== null) {
      const section = extractSection(text, options.section);
      if (section === null) {
        fail(`セクション ${options.section} が見つかりません（--toc で一覧を表示できます）`);
      }
      text = section;
    }

    if (usePager) (sink as BufferSink).expectHeadings(listHeadings(text));
    await processDocument(text, baseDir, options, ctx);
  }

  if (usePager) {
    const buffer = sink as BufferSink;
    await runPager(buffer.blocks, buffer.marks, {
      rows: rows >= 5 ? rows : 24,
      columns: (process.stdout.columns ?? 0) >= 20 ? (process.stdout.columns as number) : 100,
    });
  }
}

try {
  await main();
} catch (error) {
  if ((error as NodeJS.ErrnoException).code === "EPIPE") {
    closeSync(openSync("/dev/null", "w"));
    process.exit(0);
  }
  fail((error as Error).message);
}
