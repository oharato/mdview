/** iTerm2 inline image protocol（iTerm2 / VSCode / WezTerm など）。 */

export interface Iterm2Options {
  /** ファイル名（表示用）。 */
  name?: string;
  /** 幅（px）。省略すると画像の実寸。 */
  widthPx?: number;
  /** 高さ（px）。省略すると画像の実寸。 */
  heightPx?: number;
}

/**
 * PNG を iTerm2 のインライン画像エスケープ列にする。
 * `preserveAspectRatio=1` で縦横比を維持する。
 */
export function encodeIterm2(png: Buffer, options: Iterm2Options = {}): Buffer {
  if (png.length === 0) return Buffer.alloc(0);

  const params = ["inline=1", `size=${png.length}`];
  if (options.name) params.push(`name=${Buffer.from(options.name, "utf8").toString("base64")}`);
  if (options.widthPx) params.push(`width=${Math.round(options.widthPx)}px`);
  if (options.heightPx) params.push(`height=${Math.round(options.heightPx)}px`);
  params.push("preserveAspectRatio=1");

  const payload = `\x1b]1337;File=${params.join(";")}:${png.toString("base64")}\x07`;
  return Buffer.from(payload, "latin1");
}

/** TERM_PROGRAM / LC_TERMINAL から iTerm2 プロトコル対応端末かを推定する。 */
export function detectIterm2(): boolean {
  const program = (process.env.TERM_PROGRAM ?? "").toLowerCase();
  const lcTerminal = (process.env.LC_TERMINAL ?? "").toLowerCase();
  if (program === "iterm.app" || lcTerminal.includes("iterm")) return true;
  // VSCode 1.80+ の統合ターミナルは iTerm2 方式のインライン画像に対応
  if (program === "vscode") return true;
  return false;
}
