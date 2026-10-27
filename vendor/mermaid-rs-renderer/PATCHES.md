# Vendored mermaid-rs-renderer

Upstream: https://github.com/1jehuang/mermaid-rs-renderer
Version: 0.3.1 (MIT)

This is an unmodified copy except for two small local patches. It is vendored
because the sequence-diagram fixes are not configurable through the public
`LayoutConfig` API.

## Patches

1. `src/layout/sequence.rs`: raise the hard-coded sequence label wrap limit
   from 14 to 40 characters (instead of inheriting the global 22). The upstream code overrides
   `LayoutConfig::max_label_width_chars` with `min(14)`, which wraps short
   messages such as `mdview README.md` and `Markdown を分解` into two lines.
2. `src/render.rs`: draw sequence lifelines with `stroke-width="1.0"` instead
   of `0.5`. At terminal scale the 0.5px line often disappeared, making the
   vertical participant lines look missing.

Only `src/`, `Cargo.toml`, `LICENSE`, `README.md`, and `CHANGELOG.md` are
vendored; upstream tests, examples, and benches are omitted.
