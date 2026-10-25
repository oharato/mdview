/** Kitty graphics protocol（WezTerm / kitty / Ghostty / Herdr など）。 */

const CHUNK_BYTES = 4096;
const ESC = "\x1b";

export interface KittyPlacement {
  /** 表示する桁数。 */
  cols: number;
  /** 表示する行数。 */
  rows: number;
  /** 画像 ID（省略時は端末が採番）。 */
  imageId?: number;
}

/**
 * PNG を Kitty graphics protocol で表示するエスケープ列にする。
 *
 * - `a=T,f=100` : PNG を送ってカーソル位置に表示
 * - `C=1`       : カーソルは動かさない（呼び出し側で改行して位置を進める）
 * - `q=2`       : 端末からの応答を抑制
 */
export function encodeKitty(png: Buffer, placement: KittyPlacement): Buffer {
  if (png.length === 0) return Buffer.alloc(0);

  const payload = png.toString("base64");
  const chunks: string[] = [];
  for (let i = 0; i < payload.length; i += CHUNK_BYTES) {
    chunks.push(payload.slice(i, i + CHUNK_BYTES));
  }

  const parts: Buffer[] = [];
  chunks.forEach((chunk, index) => {
    const more = index === chunks.length - 1 ? 0 : 1;
    const control =
      index === 0
        ? [
            "a=T",
            "f=100",
            "C=1",
            `c=${Math.max(1, placement.cols)}`,
            `r=${Math.max(1, placement.rows)}`,
            "q=2",
            `m=${more}`,
          ]
        : [`m=${more}`];
    if (index === 0 && placement.imageId !== undefined) control.splice(1, 0, `i=${placement.imageId}`);
    parts.push(Buffer.from(`${ESC}_G${control.join(",")};${chunk}${ESC}\\`, "latin1"));
  });

  return Buffer.concat(parts);
}

/** Kitty graphics のサポート確認に使うクエリ（`ESC_Gi=31;OK` が返る）。 */
export const KITTY_QUERY = `${ESC}_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA${ESC}\\`;

export function parseKittyResponse(raw: string): boolean | null {
  if (!raw.includes(`${ESC}_G`)) return null;
  return /_G[^;]*;OK/.test(raw);
}
