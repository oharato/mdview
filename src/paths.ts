import { createHash } from "node:crypto";
import { homedir } from "node:os";
import { join } from "node:path";

/** キャッシュのルート（`MDVIEW_CACHE` で上書き可能）。 */
export const CACHE_DIR = process.env.MDVIEW_CACHE ?? join(homedir(), ".cache", "mdview");

export function mermaidCacheDir(): string {
  return join(CACHE_DIR, "mermaid");
}

export function svgCacheDir(): string {
  return join(CACHE_DIR, "svg");
}

export function remoteCacheDir(): string {
  return join(CACHE_DIR, "remote");
}

/** 文字列から短い安定ハッシュを作る（キャッシュキー用）。 */
export function shortHash(...parts: string[]): string {
  const h = createHash("sha256");
  for (const part of parts) {
    h.update(part);
    h.update("\0");
  }
  return h.digest("hex").slice(0, 16);
}

/** 指定 ms だけ同期的に待つ（端末プローブのポーリング用）。 */
export function sleepSync(ms: number): void {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}
