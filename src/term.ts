import { spawnSync } from "node:child_process";
import {
  closeSync,
  constants as fsConstants,
  mkdirSync,
  openSync,
  readFileSync,
  readSync,
  writeFileSync,
  writeSync,
} from "node:fs";
import { join } from "node:path";
import { KITTY_QUERY, parseKittyResponse } from "./kitty.js";
import { CACHE_DIR, sleepSync } from "./paths.js";

/** DA1 / XTWINOPS の問い合わせ結果。 */
export interface ProbeResult {
  /** Sixel 描画に対応しているか（null = 判定不能）。 */
  sixel: boolean | null;
  /** Kitty graphics protocol に対応しているか（null = 判定不能）。 */
  kitty: boolean | null;
  /** 文字セルの実ピクセルサイズ [幅, 高さ]。 */
  cell: [number, number] | null;
  /** 端末ウィンドウの実ピクセルサイズ [幅, 高さ]。 */
  window: [number, number] | null;
}

const QUERY = "\x1b[c\x1b[16t\x1b[14t" + KITTY_QUERY;
const DA1_RE = /\x1b\[\?([0-9;]+)c/;
const CELL_RE = /\x1b\[6;(\d+);(\d+)t/;
const WINDOW_RE = /\x1b\[4;(\d+);(\d+)t/;
const TIMEOUT_MS = 250;

let probePromise: Promise<ProbeResult> | null = null;

/** 端末に描画できる状況か（出力が TTY か）。 */
export function isInteractive(): boolean {
  return Boolean(process.stdout.isTTY);
}

function parseProbe(raw: string): ProbeResult {
  const result: ProbeResult = { sixel: null, kitty: parseKittyResponse(raw), cell: null, window: null };
  const da1 = DA1_RE.exec(raw);
  if (da1?.[1]) {
    result.sixel = da1[1].split(";").includes("4");
  }
  const cell = CELL_RE.exec(raw);
  if (cell) {
    const height = Number(cell[1]);
    const width = Number(cell[2]);
    if (width > 0 && height > 0) result.cell = [width, height];
  }
  const win = WINDOW_RE.exec(raw);
  if (win) {
    const height = Number(win[1]);
    const width = Number(win[2]);
    if (width > 0 && height > 0) result.window = [width, height];
  }
  return result;
}

function isComplete(raw: string): boolean {
  return DA1_RE.test(raw) && CELL_RE.test(raw) && WINDOW_RE.test(raw) && raw.includes("_G");
}

/** stdin が TTY のときの問い合わせ（raw モード + data イベント）。 */
async function queryViaStdin(): Promise<string> {
  const stdin = process.stdin;
  if (!stdin.isTTY) return "";

  return await new Promise<string>((resolve) => {
    const chunks: string[] = [];
    let timer: NodeJS.Timeout | undefined;

    const finish = (): void => {
      if (timer) clearTimeout(timer);
      stdin.off("data", onData);
      stdin.pause();
      try {
        stdin.setRawMode(false);
      } catch {
        /* ignore */
      }
      resolve(chunks.join(""));
    };

    const onData = (buf: Buffer): void => {
      chunks.push(buf.toString("latin1"));
      if (isComplete(chunks.join(""))) finish();
    };

    timer = setTimeout(finish, TIMEOUT_MS);
    stdin.setRawMode(true);
    stdin.resume();
    stdin.on("data", onData);
    process.stdout.write(QUERY);
  });
}

/** stdin が TTY でないとき: /dev/tty を非ブロッキングで開いて問い合わせる。 */
function queryViaDevTty(): string {
  let fd: number;
  try {
    fd = openSync("/dev/tty", fsConstants.O_RDWR | fsConstants.O_NONBLOCK);
  } catch {
    return "";
  }

  // エコーバックを止める（応答が画面に出ないように）。復元用に現在値を保存する。
  const saved = spawnSync("stty", ["-F", "/dev/tty", "-g"], { encoding: "utf8" });
  const savedState = saved.status === 0 ? saved.stdout.trim() : "";
  if (savedState) {
    spawnSync("stty", ["-F", "/dev/tty", "raw", "-echo"], { stdio: "ignore" });
  }

  try {
    writeSync(fd, QUERY);
    let acc = "";
    const deadline = Date.now() + TIMEOUT_MS;
    const buf = Buffer.alloc(1024);
    while (Date.now() < deadline) {
      let n = 0;
      try {
        n = readSync(fd, buf, 0, buf.length, null);
      } catch (error) {
        const code = (error as NodeJS.ErrnoException).code;
        if (code !== "EAGAIN" && code !== "EWOULDBLOCK") break;
      }
      if (n > 0) {
        acc += buf.toString("latin1", 0, n);
        if (isComplete(acc)) break;
      } else {
        sleepSync(5);
      }
    }
    return acc;
  } finally {
    if (savedState) {
      spawnSync("stty", ["-F", "/dev/tty", savedState], { stdio: "ignore" });
    }
    closeSync(fd);
  }
}

/** 端末の能力を問い合わせる（1 プロセス 1 回だけ実行）。 */
export async function probeTerminal(): Promise<ProbeResult> {
  probePromise ??= (async () => {
    if (!isInteractive()) {
      return { sixel: null, kitty: null, cell: null, window: null };
    }
    const raw = process.stdin.isTTY ? await queryViaStdin() : queryViaDevTty();
    return parseProbe(raw);
  })();
  return await probePromise;
}

function capsKey(): string {
  return ["TERM", "TERM_PROGRAM", "TERM_PROGRAM_VERSION", "SSH_CONNECTION"]
    .map((name) => process.env[name] ?? "")
    .join("|");
}

function readCapsCache(): Record<string, boolean> {
  try {
    return JSON.parse(readFileSync(join(CACHE_DIR, "caps.json"), "utf8")) as Record<string, boolean>;
  } catch {
    return {};
  }
}

function writeCapsCache(cache: Record<string, boolean>): void {
  try {
    mkdirSync(CACHE_DIR, { recursive: true });
    writeFileSync(join(CACHE_DIR, "caps.json"), JSON.stringify(cache));
  } catch {
    /* ignore */
  }
}

/** Sixel が使えるか（環境変数 → キャッシュ → 端末へ問い合わせ）。 */
export async function sixelSupported(refresh = false): Promise<boolean> {
  const override = process.env.MDVIEW_SIXEL;
  if (override !== undefined && override !== "") {
    return !["0", "false", "no"].includes(override.toLowerCase());
  }

  const key = capsKey();
  const cache = readCapsCache();
  if (!refresh && key in cache) return Boolean(cache[key]);

  const probe = await probeTerminal();
  if (probe.sixel === null) return false; // 判定不能なら半ブロック
  cache[key] = probe.sixel;
  writeCapsCache(cache);
  return probe.sixel;
}

/**
 * Kitty graphics protocol が使えるか。
 * 端末へ問い合わせ、応答が無い場合は環境変数から推定する。
 */
export async function kittySupported(): Promise<boolean> {
  const override = process.env.MDVIEW_KITTY;
  if (override !== undefined && override !== "") {
    return !["0", "false", "no"].includes(override.toLowerCase());
  }
  const probe = await probeTerminal();
  if (probe.kitty !== null) return probe.kitty;

  // クエリに応答しない端末向けのフォールバック
  if (process.env.KITTY_WINDOW_ID) return true;
  if ((process.env.TERM ?? "").includes("kitty")) return true;
  const program = (process.env.TERM_PROGRAM ?? "").toLowerCase();
  return program === "wezterm" || program === "ghostty" || program === "mintty";
}

/** 文字セルの実ピクセルサイズ（`MDVIEW_CELL=18x36` で上書き可）。 */
export async function cellPixels(): Promise<[number, number]> {
  const override = process.env.MDVIEW_CELL ?? "";
  const m = /^(\d+)\s*[xX,]\s*(\d+)$/.exec(override.trim());
  if (m?.[1] && m[2]) return [Number(m[1]), Number(m[2])];

  const probe = await probeTerminal();
  if (probe.cell) return probe.cell;

  // 最後の手段。HiDPI 端末では --pixel-scale で補正する。
  return [9, 18];
}

export async function windowPixels(): Promise<[number, number] | null> {
  const probe = await probeTerminal();
  return probe.window;
}
