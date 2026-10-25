import { describe, expect, it } from "vitest";
import { isHeadingLine, paginate, takeKeys, type Block } from "../src/pager.js";

describe("takeKeys", () => {
  it("1 文字ずつに分解する", () => {
    expect(takeKeys("ab")).toEqual(["a", "b"]);
  });

  it("矢印キーはまとめて 1 つ", () => {
    expect(takeKeys("\x1b[B")).toEqual(["\x1b[B"]);
    expect(takeKeys("a\x1b[Ab")).toEqual(["a", "\x1b[A", "b"]);
    expect(takeKeys("\x1b[6~")).toEqual(["\x1b[6~"]);
  });

  it("単独の Esc は 1 キー", () => {
    expect(takeKeys("\x1b")).toEqual(["\x1b"]);
  });

  it("絵文字を壊さない", () => {
    expect(takeKeys("🎨x")).toEqual(["🎨", "x"]);
  });
});

describe("isHeadingLine", () => {
  it("ANSI 付きでも見出しを検出する", () => {
    expect(isHeadingLine("\x1b[1m\x1b[34m┄┄\x1b[0m\x1b[1m\x1b[34m見出し\x1b[0m")).toBe(true);
    expect(isHeadingLine("┄見出し")).toBe(true);
  });

  it("本文や罫線は見出しではない", () => {
    expect(isHeadingLine("ふつうの本文")).toBe(false);
    expect(isHeadingLine("────────")).toBe(false);
  });
});

describe("paginate", () => {
  const blocks: Block[] = [
    { kind: "text", lines: ["a", "b", "c"] },
    { kind: "image", rows: 2, emit: () => {} },
    { kind: "text", lines: ["d", "e"] },
  ];

  it("ページ高さで分割する", () => {
    const pages = paginate(blocks, 3);
    expect(pages.map((page) => page.length)).toEqual([3, 2, 1]);
    expect(pages[0]!.every((entry) => entry.kind === "text")).toBe(true);
    expect(pages[1]!.map((entry) => entry.kind)).toEqual(["image", "text"]);
  });

  it("画像は分割しない（収まらなければ次ページへ）", () => {
    const pages = paginate(blocks, 4);
    // a,b,c + image(2) は 5 行なので分割される
    expect(pages[0]!.map((entry) => entry.kind)).toEqual(["text", "text", "text"]);
    expect(pages[1]!.map((entry) => entry.kind)).toEqual(["image", "text", "text"]);
  });

  it("元のブロック位置を保持する", () => {
    const pages = paginate(blocks, 10);
    const texts = pages[0]!.filter((entry) => entry.kind === "text");
    expect(texts.map((entry) => entry.kind === "text" && `${entry.block}:${entry.line_}`)).toEqual([
      "0:0",
      "0:1",
      "0:2",
      "2:0",
      "2:1",
    ]);
  });
});
