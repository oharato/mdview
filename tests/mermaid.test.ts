import { describe, expect, it } from "vitest";
import { mermaidToSvg } from "../src/mermaid.js";
import { resolveCssVariables } from "../src/theme.js";

const SAMPLE = `<svg xmlns="http://www.w3.org/2000/svg" style="--_fg:var(--pele-fg,#1f1f1f);font-family:var(--pele-font,sans-serif)" fill="var(--_fg)"><rect fill="var(--_s)" stroke="var(--_b)"/><text class="pele-label">A</text><text font-family="monospace">B</text></svg>`;

describe("resolveCssVariables", () => {
  it("CSS 変数を実色に置き換える", () => {
    const out = resolveCssVariables(SAMPLE, "dark");
    expect(out).not.toContain("var(--pele-fg");
    expect(out).not.toContain("var(--_s)");
    expect(out).toContain("#d6d6d6");
    expect(out).toContain("#262626");
  });

  it("テーマごとに色が変わる", () => {
    const dark = resolveCssVariables(SAMPLE, "dark");
    const light = resolveCssVariables(SAMPLE, "default");
    expect(dark).not.toBe(light);
  });

  it("font-family の無い text に既定フォントを補う", () => {
    const out = resolveCssVariables(SAMPLE, "dark");
    expect(out).toContain('<text font-family="Noto Sans CJK JP, sans-serif" class="pele-label">');
    const monospace = out.match(/<text font-family="monospace">/g) ?? [];
    expect(monospace).toHaveLength(1);
    expect(out).not.toContain('font-family="Noto Sans CJK JP, sans-serif" font-family="monospace"');
  });
});

describe("mermaidToSvg", () => {
  it("Pele で SVG を生成し色を解決する", () => {
    const svg = mermaidToSvg("flowchart LR\n  A[Start] --> B[End]", "dark");
    expect(svg.startsWith("<svg")).toBe(true);
    expect(svg).toContain("<text");
    expect(svg).not.toMatch(/var\(--pele-/);
  });

  it("不正な図は例外を投げる", () => {
    expect(() => mermaidToSvg("this is not a diagram", "dark")).toThrow();
  });
});
