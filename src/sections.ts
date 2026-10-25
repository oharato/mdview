/** 見出しの抽出とセクション切り出し（長い文書のナビゲーション用）。 */

export interface Heading {
  /** 1 始まりの通し番号（`--section N` の N）。 */
  index: number;
  /** `#` の数。 */
  level: number;
  text: string;
  /** 0 始まりの行番号。 */
  line: number;
}

const HEADING_RE = /^(#{1,6})\s+(.+?)\s*#*\s*$/;
const FENCE_RE = /^[ \t]*(```+|~~~+)/;

/** 見出しを文書順に列挙する（コードフェンス内は除外）。 */
export function listHeadings(markdown: string): Heading[] {
  const headings: Heading[] = [];
  let inFence = false;
  let fenceChar = "";

  markdown.split("\n").forEach((line, lineNumber) => {
    const fence = FENCE_RE.exec(line);
    if (fence?.[1]) {
      const char = fence[1][0]!;
      if (!inFence) {
        inFence = true;
        fenceChar = char;
      } else if (char === fenceChar) {
        inFence = false;
      }
      return;
    }
    if (inFence) return;

    const match = HEADING_RE.exec(line);
    if (match?.[1] && match[2]) {
      headings.push({
        index: headings.length + 1,
        level: match[1].length,
        text: match[2].trim(),
        line: lineNumber,
      });
    }
  });

  return headings;
}

/**
 * N 番目の見出しから、同じか上位レベルの次の見出しまでの本文を返す。
 * 見つからなければ null。
 */
export function extractSection(markdown: string, index: number): string | null {
  const headings = listHeadings(markdown);
  const target = headings.find((heading) => heading.index === index);
  if (!target) return null;

  const next = headings.find(
    (heading) => heading.index > target.index && heading.level <= target.level,
  );

  const lines = markdown.split("\n");
  const end = next ? next.line : lines.length;
  return lines.slice(target.line, end).join("\n").replace(/\s+$/, "") + "\n";
}
