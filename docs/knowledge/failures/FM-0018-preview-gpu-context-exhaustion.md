# FM-0018 — Animated preview canvases can exhaust the GPU process and lose the WebGL context

**Classification:** suspected (mechanism unconfirmed; reproduced in one
automated browser environment, **not** reproduced on the maintainer's Firefox)
· **Areas:** curator preview, browser GL hosting ·
**Related:** [AC3–AC7 implementation update](../../updates/2026-09-23-s12-ac3-ac7-editing-runtime-filter-preview.md)

## Trigger

Long-lived `worldwallctl curator` preview sessions with an animated scene and
per-frame canvas drawing (`preserveDrawingBuffer: true` on an in-DOM,
CSS-scaled canvas).

## Symptom

In the automated Chromium test browser: WebGL contexts were lost within
seconds of per-frame drawing (a bare clear-loop on a detached canvas and FBO
self-check renders survived), and after repeated losses the browser disabled
WebGL entirely (`getContext("webgl2")` returned null). Window teardown or a
server exit at the same time produced confusing blank/black UI captures.

## Suspected mechanism

With `preserveDrawingBuffer: true`, the compositor must preserve, copy, and
rescale the drawing buffer for every frame; sustained drawing may exhaust the
browser GPU process until the context is lost. Not confirmed: the maintainer's
Firefox rendered the same pages correctly throughout, so an environment
component cannot be excluded.

## Impact

Preview session stability in affected browser environments only. The runtime
wallpaper path (wgpu/Vulkan) is unrelated.

## Prevention / recovery

- `preserveDrawingBuffer` is off for the preview (the preview redraws every
  frame and needs no preserved buffer).
- The civil keyframe search is out of the fragment shader: the client computes
  the row-pair once per frame and the shader only fades between two atlas rows
  (the suspect loop is gone).
- `webglcontextlost` is handled with a visible status message instead of a
  silent blank canvas.
- When diagnosing preview rendering, prefer compositor-level captures or
  in-frame `readPixels`; `drawImage(glCanvas)`/late readbacks report empty
  frames without `preserveDrawingBuffer` and can be mistaken for blank output.

## Regression test

None automated (browser-host behavior); the in-page palette/spatial self-check
keeps rendering correctness covered, and window captures verify real output.

## Revalidation trigger

Recurrence in any supported browser, or a change to the preview's frame loop or
context attributes.
