import { render } from "pele";
import { rasterizeSvg } from "./images.js";
import { FONT_FAMILY, resolveCssVariables, type MermaidTheme } from "./theme.js";

export type { MermaidTheme };

/** Mermaid のコードを SVG 文字列にする（Pele）。 */
export function mermaidToSvg(code: string, theme: MermaidTheme): string {
  const { svg } = render(code, { fontFamily: FONT_FAMILY, fontFamilyMono: "monospace" });
  return resolveCssVariables(svg, theme);
}

/** Mermaid のコードを PNG（指定幅）にする。 */
export function mermaidToPng(code: string, theme: MermaidTheme, width: number): Buffer {
  return rasterizeSvg(mermaidToSvg(code, theme), width);
}
