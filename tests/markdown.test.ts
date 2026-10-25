import { describe, expect, it } from "vitest";
import { extractImages, extractMermaid, splitSegments } from "../src/markdown.js";

describe("extractMermaid", () => {
  it("mermaid フェンスをプレースホルダに置き換える", () => {
    const md = ["# title", "", "```mermaid", "graph LR", "  A --> B", "```", "", "after"].join("\n");
    const { text, blocks } = extractMermaid(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0]?.code).toBe("graph LR\n  A --> B\n");
    expect(text).toContain("@@mdview-image-0-@@");
    expect(text).not.toContain("```mermaid");
    expect(text).toContain("after");
  });

  it("チルダフェンスと他のコードブロックはそのまま", () => {
    const md = ["~~~mermaid", "pie", "~~~", "", "```js", "const a = 1", "```"].join("\n");
    const { text, blocks } = extractMermaid(md);
    expect(blocks).toHaveLength(1);
    expect(text).toContain("```js");
    expect(text).toContain("const a = 1");
  });

  it("複数の図に連番を振る", () => {
    const md = ["```mermaid", "a", "```", "", "```mermaid", "b", "```"].join("\n");
    const { text, blocks } = extractMermaid(md);
    expect(blocks.map((b) => b.code.trim())).toEqual(["a", "b"]);
    expect(text).toContain("@@mdview-image-0-@@");
    expect(text).toContain("@@mdview-image-1-@@");
  });
});

describe("extractImages", () => {
  it("単独行の画像だけを抽出する", () => {
    const md = ["text", "", "![alt](pics/a.png)", "", "文中の ![x](b.png) はそのまま"].join("\n");
    const { text, blocks } = extractImages(md, 2);
    expect(blocks).toEqual([{ src: "pics/a.png", alt: "alt" }]);
    expect(text).toContain("@@mdview-image-2-@@");
    expect(text).toContain("文中の ![x](b.png) はそのまま");
  });

  it("タイトル付き・山括弧付きのパスを扱える", () => {
    const md = '![a](<my pic.png> "title")';
    const { blocks } = extractImages(md);
    expect(blocks).toHaveLength(0); // 空白入りパスは対象外
    const md2 = '![a](pic.png "title")';
    expect(extractImages(md2).blocks).toEqual([{ src: "pic.png", alt: "a" }]);
  });
});

describe("splitSegments", () => {
  it("テキストと画像を順序どおりに分割する", () => {
    const text = ["before", "", "@@mdview-image-0-@@", "", "middle", "", "@@mdview-image-1-@@", "", "after"].join("\n");
    const segments = splitSegments(text);
    expect(segments).toEqual([
      { kind: "text", body: "before\n" },
      { kind: "asset", index: 0 },
      { kind: "text", body: "\nmiddle\n" },
      { kind: "asset", index: 1 },
      { kind: "text", body: "\nafter" },
    ]);
  });

  it("空のテキストセグメントは作らない", () => {
    const segments = splitSegments("@@mdview-image-0-@@");
    expect(segments).toEqual([{ kind: "asset", index: 0 }]);
  });
});
