/** Kitty graphics protocol（WezTerm / kitty / Ghostty / Herdr など）。 */

const CHUNK_BYTES = 4096;
const ESC = "\x1b";

export interface KittyPlacement {
  /** 表示する桁数（c=）。 */
  cols?: number;
  /** 表示する行数（r=）。 */
  rows?: number;
  /** 画像 ID（i=）。 */
  imageId?: number;
  /** カーソルを動かさない（C=1）。 */
  holdCursor?: boolean;
}

function buildControl(placement: KittyPlacement, more: number, first: boolean): string {
  const keys: string[] = [];
  if (first) {
    keys.push("a=T", "f=100");
    if (placement.imageId !== undefined) keys.push(`i=${placement.imageId}`);
    if (placement.holdCursor) keys.push("C=1");
    if (placement.cols !== undefined) keys.push(`c=${Math.max(1, placement.cols)}`);
    if (placement.rows !== undefined) keys.push(`r=${Math.max(1, placement.rows)}`);
    keys.push("q=2");
  }
  keys.push(`m=${more}`);
  return keys.join(",");
}

/**
 * PNG を Kitty graphics protocol で表示するエスケープ列にする。
 * 4096 文字ずつ分割して送信する。
 */
export function encodeKitty(png: Buffer, placement: KittyPlacement = {}): Buffer {
  if (png.length === 0) return Buffer.alloc(0);

  const payload = png.toString("base64");
  const chunks: string[] = [];
  for (let i = 0; i < payload.length; i += CHUNK_BYTES) {
    chunks.push(payload.slice(i, i + CHUNK_BYTES));
  }

  return Buffer.concat(
    chunks.map((chunk, index) => {
      const more = index === chunks.length - 1 ? 0 : 1;
      const control = buildControl(placement, more, index === 0);
      return Buffer.from(`${ESC}_G${control};${chunk}${ESC}\\`, "latin1");
    }),
  );
}

/** Kitty graphics のサポート確認に使うクエリ（`ESC_Gi=31;OK` が返る）。 */
export const KITTY_QUERY = `${ESC}_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA${ESC}\\`;

export function parseKittyResponse(raw: string): boolean | null {
  if (!raw.includes(`${ESC}_G`)) return null;
  return /_G[^;]*;OK/.test(raw);
}
