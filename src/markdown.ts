export interface MermaidBlock {
  code: string;
  info: string;
}

export interface ImageBlock {
  src: string;
  alt: string;
}

export type Segment =
  | { kind: "text"; body: string }
  | { kind: "asset"; index: number };

const SENTINEL = (index: number): string => `@@mdview-image-${index}-@@`;
const SENTINEL_RE = /^@@mdview-image-(\d+)-@@[ \t]*$/;
const IMAGE_RE = /!\[([^\]]*)\]\(\s*<?([^)>\s]+)>?(?:\s+(?:"[^"]*"|'[^']*'))?\s*\)/;
const FENCE_RE = /^([ \t]*)(`{3,}|~{3,})[ \t]*mermaid\b(.*)$/;

/** ```mermaid フェンスをプレースホルダ行に置き換える。 */
export function extractMermaid(text: string): { text: string; blocks: MermaidBlock[] } {
  const lines = text.split("\n");
  const out: string[] = [];
  const blocks: MermaidBlock[] = [];

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!;
    const match = FENCE_RE.exec(line);
    if (!match?.[2]) {
      out.push(line);
      continue;
    }

    const fenceChar = match[2][0]!;
    const fenceLength = match[2].length;
    const closing = new RegExp(`^[ \\t]*${fenceChar === "`" ? "`" : "~"}{${fenceLength},}[ \\t]*$`);
    const body: string[] = [];
    i += 1;
    while (i < lines.length && !closing.test(lines[i]!)) {
      body.push(lines[i]!);
      i += 1;
    }
    out.push("", SENTINEL(blocks.length), "");
    blocks.push({ code: body.join("\n") + "\n", info: (match[3] ?? "").trim() });
  }

  return { text: out.join("\n"), blocks };
}

/** 単独行の画像記法をプレースホルダ行に置き換える（文中のインライン画像は残す）。 */
export function extractImages(
  text: string,
  offset = 0,
): { text: string; blocks: ImageBlock[] } {
  const lines = text.split("\n");
  const out: string[] = [];
  const blocks: ImageBlock[] = [];

  for (const line of lines) {
    const match = IMAGE_RE.exec(line.trim());
    if (match && match[0] === line.trim()) {
      out.push("", SENTINEL(offset + blocks.length), "");
      blocks.push({ src: match[2] ?? "", alt: match[1] ?? "" });
    } else {
      out.push(line);
    }
  }

  return { text: out.join("\n"), blocks };
}

/** プレースホルダで本文と画像に分割する。 */
export function splitSegments(text: string): Segment[] {
  const segments: Segment[] = [];
  let buffer: string[] = [];

  const flush = (): void => {
    const body = buffer.join("\n");
    if (body.trim() !== "") segments.push({ kind: "text", body });
    buffer = [];
  };

  for (const line of text.split("\n")) {
    const match = SENTINEL_RE.exec(line);
    if (match?.[1] !== undefined) {
      flush();
      segments.push({ kind: "asset", index: Number(match[1]) });
    } else {
      buffer.push(line);
    }
  }
  flush();

  return segments;
}
