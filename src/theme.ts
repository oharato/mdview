export type MermaidTheme = "default" | "dark" | "neutral";

export const FONT_FAMILY = "Noto Sans CJK JP, sans-serif";

interface Theme {
  bg: string;
  fg: string;
  muted: string;
  line: string;
  surface: string;
  surfaceAlt: string;
  border: string;
  accent: string;
  series: string[];
}

export const THEMES: Record<MermaidTheme, Theme> = {
  default: {
    bg: "none",
    fg: "#1f1f1f",
    muted: "#6e6e6e",
    line: "#1f1f1f",
    surface: "#f3f3f3",
    surfaceAlt: "#e6e6e6",
    border: "#8a8a8a",
    accent: "#205ea6",
    series: ["#205ea6", "#a02f6f", "#66800b", "#bc5215", "#5e409d", "#24837b", "#a05b1c", "#8a8a8a"],
  },
  dark: {
    bg: "none",
    fg: "#d6d6d6",
    muted: "#9a9a9a",
    line: "#a8a8a8",
    surface: "#262626",
    surfaceAlt: "#333333",
    border: "#6b6b6b",
    accent: "#7aa2f7",
    series: ["#7aa2f7", "#f7768e", "#9ece6a", "#e0af68", "#bb9af7", "#7dcfff", "#ff9e64", "#a9b1d6"],
  },
  neutral: {
    bg: "none",
    fg: "#333333",
    muted: "#666666",
    line: "#555555",
    surface: "#f0f0f0",
    surfaceAlt: "#e2e2e2",
    border: "#888888",
    accent: "#0b6bcb",
    series: ["#0b6bcb", "#b3261e", "#4e6b0b", "#a35200", "#5b3fa8", "#146c6c", "#8a5a00", "#888888"],
  },
};

/** `var(--name, fallback)` を fallback に置き換える（CSS 変数を解決できない resvg 向け）。 */
export function applyCssVarFallbacks(svg: string): string {
  return svg.replace(
    /var\((--[a-z0-9-]+),\s*([^()]*(?:\([^()]*\))?[^()]*)\)/gi,
    (whole, _name: string, fallback: string) => fallback.trim() || whole,
  );
}

/**
 * Pele の SVG は CSS 変数（`--_fg:var(--pele-fg,#1f1f1f)` 等）で配色されるが、
 * resvg はカスタムプロパティを解決できない。実色に置換してから渡す。
 */
export function resolveCssVariables(svg: string, themeName: MermaidTheme): string {
  const theme = THEMES[themeName];
  const shortVars: Record<string, string> = {
    bg: theme.bg,
    fg: theme.fg,
    m: theme.muted,
    l: theme.line,
    s: theme.surface,
    a: theme.surfaceAlt,
    b: theme.border,
    c: theme.accent,
  };
  const longVars: Record<string, string> = {
    "pele-bg": theme.bg,
    "pele-fg": theme.fg,
    "pele-muted": theme.muted,
    "pele-line": theme.line,
    "pele-surface": theme.surface,
    "pele-surface-alt": theme.surfaceAlt,
    "pele-border": theme.border,
    "pele-accent": theme.accent,
  };
  theme.series.forEach((color, index) => {
    longVars[`pele-series-${index + 1}`] = color;
  });

  let out = svg.replace(
    /var\(--_([a-z]+)(?:,\s*([^()]*(?:\([^()]*\))?[^()]*))?\)/g,
    (whole, key: string, fallback: string | undefined) => {
      if (key in shortVars) return shortVars[key]!;
      return fallback !== undefined ? fallback.trim() : whole;
    },
  );

  out = out.replace(
    /var\((--pele-[a-z0-9-]+)(?:,\s*([^()]*(?:\([^()]*\))?[^()]*))?\)/g,
    (whole, key: string, fallback: string | undefined) => {
      const name = key.replace(/^--/, "");
      if (name in longVars) return longVars[name]!;
      return fallback !== undefined ? fallback.trim() : whole;
    },
  );

  // resvg はルートの font-family を継承しないことがあるため、text に明示する。
  out = out.replace(/<text(?![^>]*font-family)([\s>])/g, `<text font-family="${FONT_FAMILY}"$1`);

  return out;
}

/** Pele 由来の SVG かどうか。 */
export function isPeleSvg(svg: string): boolean {
  return svg.includes('class="pele') || svg.includes("--pele-");
}
