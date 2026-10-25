#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readdirSync,
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
import { mermaidCacheDir, remoteCacheDir, shortHash } from "./paths.js";
import { encodeSixel } from "./sixel.js";
import { encodeIterm2, detectIterm2 } from "./iterm.js";
import { encodeKitty } from "./kitty.js";
import {
  cellPixels,
  isInteractive,
  kittySupported,
  probeTerminal,
  sixelSupported,
  terminalName,
  windowPixels,
} from "./term.js";

const VERSION = "0.3.1";
const DIM = "\x1b[2m";
const RESET = "\x1b[0m";

type Renderer = "auto" | "kitty" | "sixel" | "iterm" | "blocks";
type MermaidEngine = "auto" | "pele" | "mmdc";
type ThemeSetting = "auto" | MermaidTheme;

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
  mermaidEngine: MermaidEngine;
  mermaidTheme: ThemeSetting;
  mermaidWidth: number;
  mermaidTimeout: number;
  check: boolean;
  refresh: boolean;
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
      --mermaid-engine E    auto | pele | mmdc (既定: auto)
      --mermaid-theme T     auto | default | dark | neutral
      --mermaid-width N     Mermaid のラスタライズ幅 px (既定: 2048)
      --mermaid-timeout S   Mermaid のタイムアウト秒 (既定: 90)
      --check               端末・依存の対応状況を表示して終了
      --refresh             端末判定をやり直す
  -V, --version             バージョン表示
  -h, --help                このヘルプ

環境変数:
  MDVIEW_SIXEL=0|1    Sixel の強制
  MDVIEW_CELL=18x36   セルサイズの上書き
  MDVIEW_CACHE=DIR    キャッシュ先 (既定: ~/.cache/mdview)
`;

function getStdoutSize(): { columns: number; rows: number } {
  const columns = process.stdout.columns ?? 100;
  const rows = process.stdout.rows ?? 40;
  return { columns, rows };
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
        "mermaid-engine": { type: "string" },
        "mermaid-theme": { type: "string" },
        "mermaid-width": { type: "string" },
        "mermaid-timeout": { type: "string" },
        check: { type: "boolean" },
        refresh: { type: "boolean" },
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
  const mermaidEngine = pick(v["mermaid-engine"], ["auto", "pele", "mmdc"] as const, "auto");
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
    mermaidEngine,
    mermaidTheme,
    mermaidWidth: num(v["mermaid-width"], 2048),
    mermaidTimeout: float(v["mermaid-timeout"], 90) * 1000,
    check: v.check ?? false,
    refresh: v.refresh ?? false,
  };
}

function runMdcat(chunk: string, options: Options): void {
  const mdcat = process.env.MDVIEW_MDCAT ?? "mdcat";
  const args = ["--columns", String(options.width)];
  if (options.noColour) args.push("--no-colour");
  else args.push("--ansi");

  const result = spawnSync(mdcat, args, { input: chunk, stdio: ["pipe", "inherit", "inherit"] });
  if (result.error || result.status !== 0) {
    writeOut(chunk);
  }
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

async function renderMermaid(block: MermaidBlock, options: Options): Promise<void> {
  const theme = await resolveTheme(options);
  const cachePath = mermaidCachePath(block.code, theme, options.mermaidWidth);

  if (!existsSync(cachePath)) {
    try {
      const png = mermaidToPng(block.code, theme, options.mermaidWidth);
      writeFileSync(cachePath, png);
    } catch (error) {
      warn(`Mermaid を描画できません: ${(error as Error).message}`);
      if (options.mermaidEngine !== "mmdc") {
        runMdcat("```mermaid\n" + block.code + "```\n", options);
        return;
      }
      await renderWithMmdc(block, options, cachePath);
    }
  }

  if (existsSync(cachePath)) await showImage(cachePath, "mermaid", options);
}

/** mermaid-cli 用の Chrome を探す（任意のフォールバック）。 */
function findChrome(): string | null {
  const fromEnv = process.env.PUPPETEER_EXECUTABLE_PATH;
  if (fromEnv && existsSync(fromEnv)) return fromEnv;
  const cache = resolve(process.env.HOME ?? "~", ".cache", "puppeteer");
  if (existsSync(cache)) {
    const candidates = [
      resolve(cache, "chrome-headless-shell"),
      resolve(cache, "chrome"),
    ];
    for (const base of candidates) {
      try {
        for (const entry of readdirSync(base)) {
          for (const bin of ["chrome-headless-shell-linux64/chrome-headless-shell", "chrome-linux64/chrome"]) {
            const candidate = resolve(base, entry, bin);
            if (existsSync(candidate)) return candidate;
          }
        }
      } catch {
        /* ignore */
      }
    }
  }
  for (const name of ["chromium", "chromium-browser", "google-chrome", "google-chrome-stable"]) {
    const found = spawnSync("which", [name], { encoding: "utf8" });
    const path = found.stdout?.trim();
    if (path) return path;
  }
  return null;
}

async function renderWithMmdc(
  block: MermaidBlock,
  options: Options,
  cachePath: string,
): Promise<void> {
  const dir = dirname(cachePath);
  const source = resolve(dir, `${shortHash(block.code)}.mmd`);
  writeFileSync(source, block.code);
  const env = { ...process.env };
  const chrome = findChrome();
  if (chrome && !env.PUPPETEER_EXECUTABLE_PATH) env.PUPPETEER_EXECUTABLE_PATH = chrome;
  const result = spawnSync(
    "mmdc",
    ["-i", source, "-o", cachePath, "-t", "dark", "-b", "transparent", "-s", "3"],
    { encoding: "utf8", timeout: options.mermaidTimeout, env },
  );
  if (result.error || result.status !== 0 || !existsSync(cachePath)) {
    const detail = (result.stderr ?? "").trim().split("\n").at(-1) ?? "unknown error";
    warn(`Mermaid のレンダリングに失敗: ${detail}`);
    runMdcat("```mermaid\n" + block.code + "```\n", options);
  }
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

async function renderImage(block: ImageBlock, baseDir: string, options: Options): Promise<void> {
  const source = await resolveImageSource(block.src, baseDir);
  if (!source) {
    writeOut(`${DIM}[画像: ${block.alt || block.src}]${RESET}\n`);
    return;
  }
  await showImage(source, block.alt, options);
}

/**
 * 端末が WezTerm か。WezTerm は DA1 で Sixel 対応 (属性 4) を返すが実装していないため、
 * XTVERSION (`CSI > 0 q`) の応答で識別して Sixel を使わない。
 * 参考: https://github.com/wezterm/wezterm/issues/8070
 */
async function isWezTerm(): Promise<boolean> {
  if ((process.env.TERM_PROGRAM ?? "").toLowerCase() === "wezterm") return true;
  return (await terminalName()) === "wezterm";
}

let warnedKittyTransfer = false;

async function chooseRenderer(options: Options): Promise<Exclude<Renderer, "auto">> {
  if (options.renderer !== "auto") return options.renderer;
  if (await kittySupported()) return "kitty";

  const probe = await probeTerminal();
  if (probe.kitty === true && probe.kittyTransfer === false && !warnedKittyTransfer) {
    warnedKittyTransfer = true;
    warn(
      "kitty graphics の画像転送に失敗しました（Windows の ssh.exe / ConPTY 経由では画像が描画されないことがあります）。" +
        "`wezterm ssh` での接続か、-r blocks をお試しください",
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

async function showImage(path: string, alt: string, options: Options): Promise<void> {
  if (options.noColour) {
    writeOut(`[画像: ${alt || basename(path)}]\n`);
    return;
  }

  if (isSvg(path)) {
    try {
      const image = await loadSvg(path, options.mermaidWidth, await resolveTheme(options));
      await drawImage(image, options);
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
  await drawImage(image, options);
}

async function drawImage(
  image: { width: number; height: number; data: Buffer },
  options: Options,
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
      writeOut(encodeSixel(resized, options.imageColors));
      writeOut("\n");
      return;
    }

    const png = await toPng(resized);
    if (renderer === "kitty") {
      const [cellWidth, cellHeight] = await cellPixels();
      const cols = Math.max(1, Math.round(resized.width / cellWidth));
      const placementRows = Math.max(1, Math.ceil(resized.height / cellHeight));
      writeOut(encodeKitty(png, { cols, rows: placementRows }));
      // C=1（カーソルを動かさない）で送っているので、こちらで行を進める
      writeOut("\n".repeat(placementRows));
      return;
    }

    writeOut(encodeIterm2(png));
    return;
  }

  const { rows } = getStdoutSize();
  const plan = planHalfBlocks(image.width, image.height, options.imageWidth, Math.max(4, rows - 3));
  const resized = await resizeImage(image, plan.cols, plan.rows * 2);
  writeOut(renderHalfBlocks(resized, options.colorMode));
}

async function processDocument(text: string, baseDir: string, options: Options): Promise<void> {
  const mermaid = extractMermaid(text);
  const images = extractImages(mermaid.text, mermaid.blocks.length);
  const assets: (MermaidBlock | ImageBlock)[] = [...mermaid.blocks, ...images.blocks];

  for (const segment of splitSegments(images.text)) {
    if (segment.kind === "text") {
      runMdcat(segment.body, options);
      continue;
    }
    const asset = assets[segment.index];
    if (!asset) continue;
    if ("code" in asset) await renderMermaid(asset, options);
    else await renderImage(asset, baseDir, options);
  }
}

function commandVersion(command: string): string | null {
  const result = spawnSync(command, ["--version"], { encoding: "utf8", timeout: 5000 });
  if (result.error || result.status !== 0) return null;
  const line = `${result.stdout ?? ""}${result.stderr ?? ""}`.trim().split("\n")[0] ?? "";
  return line || "found";
}

function commandExists(command: string): boolean {
  return spawnSync("which", [command], { encoding: "utf8" }).status === 0;
}

/** セルフテストの結果を文字列にする。 */
async function kittyTransferStatus(): Promise<string> {
  const probe = await probeTerminal();
  if (probe.kitty !== true) return "n/a (kitty 非対応)";
  if (probe.kittyTransfer === true) return "ok";
  if (probe.kittyTransfer === false) return "失敗 (画像データが届かない)";
  return "未検証";
}

async function printCheck(options: Options): Promise<void> {
  const sixel = await sixelSupported(options.refresh);
  const kitty = await kittySupported();
  const [cellWidth, cellHeight] = await cellPixels();
  const window = await windowPixels();
  const renderer = await chooseRenderer(options);
  const { columns } = getStdoutSize();
  const mdcat = process.env.MDVIEW_MDCAT ?? "mdcat";
  const lines: [string, string][] = [
    ["mdview", VERSION],
    ["node", process.version],
    ["mdcat", commandVersion(mdcat) ?? "見つかりません"],
    ["pele", "built-in"],
    ["resvg", "built-in (@resvg/resvg-js)"],
    ["mmdc", commandExists("mmdc") ? (commandVersion("mmdc") ?? "found") : "未インストール (任意)"],
    ["TERM", process.env.TERM ?? ""],
    ["terminal", (await terminalName()) || "不明 (XTVERSION 未応答)"],
    ["columns", String(columns)],
    ["cell pixels", `${cellWidth}x${cellHeight}`],
    ["window pixels", window ? `${window[0]}x${window[1]}` : "不明"],
    ["interactive", String(isInteractive())],
    ["kitty support", kitty ? "yes" : "no"],
    ["kitty transfer", await kittyTransferStatus()],
    ["sixel support", sixel ? "yes" : "no"],
    ["iterm2 support", detectIterm2() ? "yes" : "no"],
    ["renderer", renderer],
    ["image width", `${options.imageWidth} cols`],
  ];
  for (const [label, value] of lines) {
    writeOut(`${label.padEnd(14)}: ${value}\n`);
  }
}

async function main(): Promise<void> {
  const options = parseOptions(process.argv.slice(2));

  if (options.check) {
    await printCheck(options);
    return;
  }

  if (isInteractive() && !options.noColour) {
    // 端末判定を先に済ませる（後続の出力と混ざらないように）。
    await chooseRenderer(options);
  }

  for (const name of options.files) {
    if (name === "-") {
      const chunks: Buffer[] = [];
      for await (const chunk of process.stdin) chunks.push(Buffer.from(chunk as Buffer));
      await processDocument(Buffer.concat(chunks).toString("utf8"), process.cwd(), options);
    } else {
      const path = resolve(name);
      if (!existsSync(path)) fail(`ファイルがありません: ${name}`);
      if (statSync(path).isDirectory()) fail(`ディレクトリは指定できません: ${name}`);
      const text = await readFile(path, "utf8");
      await processDocument(text, dirname(path), options);
    }
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
