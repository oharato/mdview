import { describe, expect, it } from "vitest";
import { displayWidth, extractTables, formatInline, renderTable } from "../src/tables.js";

describe("displayWidth", () => {
  it("ASCII は 1 桁", () => {
    expect(displayWidth("abc")).toBe(3);
    expect(displayWidth("")).toBe(0);
  });

  it("CJK・全角・絵文字は 2 桁", () => {
    expect(displayWidth("あいう")).toBe(6);
    expect(displayWidth("半ブロック")).toBe(10);
    expect(displayWidth("ＡＢ")).toBe(4);
    expect(displayWidth("🎨")).toBe(2);
  });

  it("結合文字は 0 桁", () => {
    expect(displayWidth("e\u0301")).toBe(1);
  });
});

describe("extractTables", () => {
  const md = ["| 名前 | 値 |", "|:---|---:|", "| あ | 1 |", "| b | 22 |", "", "after"].join("\n");

  it("ヘッダ・本文・配置を抽出する", () => {
    const { tables } = extractTables(md);
    expect(tables).toHaveLength(1);
    expect(tables[0]?.header).toEqual(["名前", "値"]);
    expect(tables[0]?.alignments).toEqual(["left", "right"]);
    expect(tables[0]?.rows).toEqual([
      ["あ", "1"],
      ["b", "22"],
    ]);
  });

  it("プレースホルダに置き換える", () => {
    const { text } = extractTables(md);
    expect(text).toContain("@@mdview-table-0-@@");
    expect(text).toContain("after");
  });

  it("コードフェンス内の表は対象外", () => {
    const code = ["```markdown", "| a | b |", "|---|---|", "| 1 | 2 |", "```"].join("\n");
    expect(extractTables(code).tables).toHaveLength(0);
  });

  it("エスケープしたパイプを扱う", () => {
    const { tables } = extractTables(["| a | b |", "|---|---|", "| x \\| y | z |"].join("\n"));
    expect(tables[0]?.rows[0]).toEqual(["x | y", "z"]);
  });
});

describe("renderTable", () => {
  const table = {
    alignments: ["left", "right"] as const,
    header: ["プロトコル", "解像度"],
    rows: [
      ["Kitty graphics", "フル"],
      ["半ブロック", "低"],
    ],
  };

  it("CJK を含んでも全行の表示幅が揃う", () => {
    const out = renderTable({ ...table, alignments: [...table.alignments] }, { maxWidth: 80, colour: false });
    const widths = new Set(out.trimEnd().split("\n").map(displayWidth));
    expect(widths.size).toBe(1);
  });

  it("ANSI を含めない（colour: false）", () => {
    const out = renderTable({ ...table, alignments: [...table.alignments] }, { maxWidth: 80, colour: false });
    expect(out).not.toContain("\x1b[");
  });

  it("幅に収まらない場合は切り詰める", () => {
    const out = renderTable({ ...table, alignments: [...table.alignments] }, { maxWidth: 24, colour: false });
    for (const line of out.trimEnd().split("\n")) {
      expect(displayWidth(line)).toBeLessThanOrEqual(24);
    }
  });
});

describe("formatInline", () => {
  it("マークダウンを ANSI に変換する", () => {
    expect(formatInline("**太字**", true)).toContain("\x1b[1m");
    expect(formatInline("`code`", true)).toContain("\x1b[33m");
    expect(formatInline("**太字**", false)).toBe("太字");
  });
});
