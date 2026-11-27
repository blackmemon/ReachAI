# 0004 Iced web renderer: tiny-skia only, with a patched copy

Status: accepted.

Iced 0.13.1 compiled to `wasm32-unknown-unknown`. The software renderer `tiny-skia` is the only renderer (features `tiny-skia`, `web-colors`, `svg`). No GPU renderer is built, so the bundle needs no WebGL or WebGPU and works in every browser the app targets.

The upstream `iced_tiny_skia` 0.13 has faults that showed up in a real browser. A copy is kept in `vendor/iced_tiny_skia` and used through `[patch.crates-io]` in the root `Cargo.toml`. Each patch is marked `PATCH (ADR 0004)` in the code. When upstream fixes these, remove the copy.

1. A canvas of size 0 at start made the renderer panic (zero-size surface and clip mask). The patch uses at least 1 pixel (`src/window/compositor.rs`, `width.max(1)`). The real size arrives at the first resize.
2. Shadows are drawn outside the box of a card, but the changed-area list only covered the box. Old shadow pixels stayed on screen. The changed area is widened by the blur and offset (`src/layer.rs`).
3. **Whole-window redraw on every frame (removed, 2026-11-24).** An earlier patch redrew the full window each frame because soft colours got darker after partial redraws. But every mouse move redraws (hover), so each hover cost a full-screen software paint and copy: 230 to 400 ms per frame at 1280x800, and the app felt frozen. The real cause of the darkening was the shadow code (next item). Partial redraw is back: changed areas are widened by 2 px and snapped to whole pixels, many small areas are merged into one bounding area, and only that area is clear-and-redrawn and handed to the browser (`present_with_damage`).
4. **Shadows drew outside the cleared area.** `draw_quad` computed the whole shadow of a quad on every frame and drew it with no clip mask, on top of pixels that were not cleared, so shadows piled up darker. It now computes only the part inside the redraw area and draws it through the clip mask (`src/engine.rs`).
5. **Unbounded boxes panicked the whole page.** A `Fill` child inside a scrollable gets an infinite height; the rounded-rectangle path then failed to build and `expect` took down the WASM app. This froze `/quick`. Boxes are clamped to a finite size so one bad layout cannot end the page (`rounded_rectangle` in `src/engine.rs`). The Quick screen itself no longer uses `Fill` height inside a scrollable.
6. **SVG icons and the logo were drawn twice-scaled.** The vector pipeline got bounds that were already in physical pixels and also the full scale transform, so icons landed in the wrong place and size whenever the app scale was not 1. Only the rotation is applied now.

## Release profile

`opt-level = "z"` (smallest) made the drawing code, which runs on every frame, several times slower. The rest of the bundle stays at `z`; the crates that draw (tiny-skia, softbuffer, cosmic-text, swash, rustybuzz, ttf-parser, iced core, graphics and widgets, kurbo) are built with `opt-level = 3` in `[profile.release.package.*]`. Cost: the Brotli bundle grew from 1.27 MB to 1.40 MB (about 10%). Measured in headless Chrome at 1280x800: scrolling a 25-row table went from frames of 2 to 3 s to a median of 17 ms (one frame at the screen's refresh rate).
