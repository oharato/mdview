import { describe, expect, it } from "vitest";
import { extractSection, listHeadings } from "../src/sections.js";

const DOC = [
  "# タイトル",
  "",
  "はじめに",
  "",
  "## 準備",
  "",
  "```bash",
  "# これはコードブロック内の見出しではない",
  "```",
  "",
  "本文",
  "",
  "## 使い方",
  "",
  "### 応用",
  "",
  "応用の本文",
  "",
  "## まとめ",
  "",
  "おわり",
].join("\n");

describe("listHeadings", () => {
  it("見出しを順に列挙する", () => {
    const headings = listHeadings(DOC);
    expect(headings.map((h) => `${h.index}:${h.level}:${h.text}`)).toEqual([
      "1:1:タイトル",
      "2:2:準備",
      "3:2:使い方",
      "4:3:応用",
      "5:2:まとめ",
    ]);
  });

  it("コードフェンス内の # は無視する", () => {
    expect(listHeadings(DOC).some((h) => h.text.includes("コードブロック"))).toBe(false);
  });
});

describe("extractSection", () => {
  it("同レベル以上の次の見出しまでを返す", () => {
    const section = extractSection(DOC, 3);
    expect(section).toContain("## 使い方");
    expect(section).toContain("### 応用");
    expect(section).toContain("応用の本文");
    expect(section).not.toContain("## まとめ");
  });

  it("上位見出しは含めない", () => {
    const section = extractSection(DOC, 4);
    expect(section?.trim().startsWith("### 応用")).toBe(true);
    expect(section).not.toContain("## まとめ");
    expect(section?.trim().endsWith("応用の本文")).toBe(true);
  });

  it("存在しない番号は null", () => {
    expect(extractSection(DOC, 99)).toBeNull();
    expect(extractSection(DOC, 0)).toBeNull();
  });
});
