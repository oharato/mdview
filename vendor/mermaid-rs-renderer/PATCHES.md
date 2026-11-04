# Vendored mermaid-rs-renderer

Upstream: https://github.com/1jehuang/mermaid-rs-renderer
Version: 0.3.1 (MIT)

This copy includes local rendering and label-placement patches that cannot be
configured through the public `LayoutConfig` API.

## Patches

1. `src/layout/sequence.rs`: raise the hard-coded sequence label wrap limit
   from 14 to 40 characters (instead of inheriting the global 22). The upstream code overrides
   `LayoutConfig::max_label_width_chars` with `min(14)`, which wraps short
   messages such as `mdview README.md` and `Markdown を分解` into two lines.
2. `src/render.rs`: draw sequence lifelines with `stroke-width="1.0"` instead
   of `0.5`. At terminal scale the 0.5px line often disappeared, making the
   vertical participant lines look missing.
3. `src/render.rs`: orient ER cardinality markers into the relationship at both
   endpoints and keep all marker strokes outside node rectangles. Crow's feet
   now fan out toward the entity; circles and bars sit farther along the edge.
4. `src/layout/label_placement.rs`: increase class multiplicity offsets and use
   candidates beside the edge so labels cannot be placed directly on its line.
   Let ER relationship labels move from router-seeded anchors using the existing
   collision avoidance, targeting 8px clearance from their carrying edge to protect
   cardinality markers. Allow the canvas to grow around these labels.

Upstream checked at `3726ccbffe0e8032361eb9668694b24f77858060` (2026-09-07):
the marker geometry and class/ER label-placement constraints above remain there.
Regression coverage for these patches is in mdview's `src/mermaid.rs` tests.

Only `src/`, `Cargo.toml`, `LICENSE`, `README.md`, and `CHANGELOG.md` are
vendored; upstream tests, examples, and benches are omitted.
