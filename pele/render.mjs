#!/usr/bin/env node
// Pele で Mermaid を SVG 化し、CSS 変数を実色に解決して stdout へ出す。
// 使い方: node render.mjs [theme] < diagram.mmd > diagram.svg
import { readFileSync } from "node:fs";
import { render } from "pele";

const THEMES = {
  default: {
    bg: "none", fg: "#1f1f1f", m: "#6e6e6e", l: "#1f1f1f", s: "#f3f3f3",
    a: "#e6e6e6", b: "#8a8a8a", c: "#205ea6",
    series: ["#205ea6", "#a02f6f", "#66800b", "#bc5215", "#5e409d", "#24837b", "#a05b1c", "#8a8a8a"],
  },
  dark: {
    bg: "none", fg: "#d6d6d6", m: "#9a9a9a", l: "#a8a8a8", s: "#262626",
    a: "#333333", b: "#6b6b6b", c: "#7aa2f7",
    series: ["#7aa2f7", "#f7768e", "#9ece6a", "#e0af68", "#bb9af7", "#7dcfff", "#ff9e64", "#a9b1d6"],
  },
  neutral: {
    bg: "none", fg: "#333333", m: "#666666", l: "#555555", s: "#f0f0f0",
    a: "#e2e2e2", b: "#888888", c: "#0b6bcb",
    series: ["#0b6bcb", "#b3261e", "#4e6b0b", "#a35200", "#5b3fa8", "#146c6c", "#8a5a00", "#888888"],
  },
};

const theme = THEMES[process.argv[2]] || THEMES.default;
const src = readFileSync(0, "utf8");

let svg;
try {
  ({ svg } = render(src));
} catch (e) {
  process.stderr.write(`pele: ${e && e.message ? e.message : e}\n`);
  process.exit(1);
}

const vars = {
  bg: theme.bg, fg: theme.fg, m: theme.m, l: theme.l,
  s: theme.s, a: theme.a, b: theme.b, c: theme.c,
};

// 1) var(--_x) / var(--_x, fallback) を実色に置換（色のみ。それ以外は fallback を残す）
svg = svg.replace(/var\(--_([a-z]+)(?:,\s*([^()]*(?:\([^()]*\))?[^()]*))?\)/g, (m, key, fb) => {
  if (key in vars) return vars[key];
  return fb !== undefined ? fb.trim() : m;
});

// 2) var(--pele-*) も同様に解決
const peleVars = {
  "pele-bg": theme.bg, "pele-fg": theme.fg, "pele-muted": theme.m,
  "pele-line": theme.l, "pele-surface": theme.s, "pele-surface-alt": theme.a,
  "pele-border": theme.b, "pele-accent": theme.c,
};
for (let i = 1; i <= 8; i += 1) peleVars[`pele-series-${i}`] = theme.series[i - 1];
svg = svg.replace(/var\((--pele-[a-z0-9-]+)(?:,\s*([^()]*(?:\([^()]*\))?[^()]*))?\)/g, (m, key, fb) => {
  const name = key.replace(/^--/, "");
  if (name in peleVars) return peleVars[name];
  return fb !== undefined ? fb.trim() : m;
});

// 3) resvg はルートの font-family を継承しないことがあるので text に明示する
svg = svg.replace(/<text(?![^>]*font-family)([\s>])/g, '<text font-family="Noto Sans CJK JP, sans-serif"$1');

process.stdout.write(svg);
