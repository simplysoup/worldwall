/* Curator preview palette reference (S12 AC6).
 *
 * This module is the exact JavaScript twin of the palette math in the
 * Worldwall WGSL scene shader (src/renderer.rs INDEXED_SCENE_SHADER) and its
 * accepted CPU reference `Animation::apply_palette` (src/animation.rs). The
 * preview shader port is compared against this twin at runtime, and offline
 * tests compare this twin against Rust-generated goldens in
 * fixtures/preview-palette-goldens.json.
 *
 * Inputs use the same byte payloads as the GPU path: authored sRGB RGBA8
 * palette bytes, `Animation::metadata_bytes()` (32 bytes per slot: four u32
 * then four f32, native little-endian on the supported hosts), and
 * `Animation::cycle_index_bytes()` (one u32 per slot).
 */
(function (root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
  } else {
    root.CuratorPalette = factory();
  }
})(typeof self !== "undefined" ? self : this, function () {
  "use strict";

  const TAU = 6.28318530718;

  function srgbToLinear(value) {
    if (value <= 0.04045) {
      return Math.fround(value / 12.92);
    }
    return Math.fround(Math.pow((value + 0.055) / 1.055, 2.4));
  }

  function linearToSrgb(value) {
    let encoded;
    if (value <= 0.0031308) {
      encoded = value * 12.92;
    } else {
      encoded = 1.055 * Math.pow(value, 1.0 / 2.4) - 0.055;
    }
    return Math.min(255, Math.max(0, Math.round(encoded * 255)));
  }

  function fract(value) {
    return value - Math.floor(value);
  }

  function rgbToHsv(color) {
    const maximum = Math.max(color[0], Math.max(color[1], color[2]));
    const minimum = Math.min(color[0], Math.min(color[1], color[2]));
    const delta = maximum - minimum;
    let hue = 0.0;
    if (delta !== 0.0) {
      if (maximum === color[0]) {
        hue = (color[1] - color[2]) / delta / 6.0;
      } else if (maximum === color[1]) {
        hue = ((color[2] - color[0]) / delta + 2.0) / 6.0;
      } else {
        hue = ((color[0] - color[1]) / delta + 4.0) / 6.0;
      }
    }
    if (hue < 0.0) {
      hue = hue + 1.0;
    }
    let saturation = 0.0;
    if (maximum !== 0.0) {
      saturation = delta / maximum;
    }
    return [hue, saturation, maximum];
  }

  function hsvToRgb(hsv) {
    const scaled = fract(hsv[0]) * 6.0;
    const sector = Math.floor(scaled) % 6;
    const fraction = fract(scaled);
    const p = hsv[2] * (1.0 - hsv[1]);
    const q = hsv[2] * (1.0 - hsv[1] * fraction);
    const t = hsv[2] * (1.0 - hsv[1] * (1.0 - fraction));
    switch (sector) {
      case 0:
        return [hsv[2], t, p];
      case 1:
        return [q, hsv[2], p];
      case 2:
        return [p, hsv[2], t];
      case 3:
        return [p, q, hsv[2]];
      case 4:
        return [t, p, hsv[2]];
      default:
        return [hsv[2], p, q];
    }
  }

  /* Decode `Animation::metadata_bytes()` into per-slot records. */
  function parseMetadata(bytes) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const entries = [];
    for (let index = 0; index < 256; index += 1) {
      const offset = index * 32;
      entries.push({
        effect: view.getUint32(offset, true),
        groupOffset: view.getUint32(offset + 4, true),
        memberCount: view.getUint32(offset + 8, true),
        ordinal: view.getUint32(offset + 12, true),
        speed: view.getFloat32(offset + 16, true),
        amplitude: view.getFloat32(offset + 20, true),
        phase: view.getFloat32(offset + 24, true),
      });
    }
    return entries;
  }

  /* Decode `Animation::cycle_index_bytes()` (256 u32 slot references). */
  function parseCycleIndices(bytes) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const indices = new Uint32Array(256);
    for (let index = 0; index < 256; index += 1) {
      indices[index] = view.getUint32(index * 4, true);
    }
    return indices;
  }

  /*
   * Shared palette source: the authored base palette, or one byte-floor fade
   * between time-of-day atlas rows (civil keyframe sampling or injected
   * preview row state), exactly like the shader's `time_of_day_bytes`.
   * `tod` is:
   *   null                                   -> base palette
   *   {mode: "rows", lower, upper, amount}   -> injected row-pair preview
   *   {mode: "civil", civilSeconds, keyframes:[{civilSeconds,row}], rows}
   */
  function todBytes(state, index) {
    const palette = state.palette;
    const at = index * 4;
    const base = [palette[at], palette[at + 1], palette[at + 2]];
    const tod = state.tod;
    if (!tod || tod.count < 2) {
      return base;
    }
    if (tod.mode === "rows") {
      const lower = todAtlasRow(state, tod.lower);
      const upper = todAtlasRow(state, tod.upper);
      return [
        Math.floor(lower[0] + (upper[0] - lower[0]) * tod.amount),
        Math.floor(lower[1] + (upper[1] - lower[1]) * tod.amount),
        Math.floor(lower[2] + (upper[2] - lower[2]) * tod.amount),
      ];
    }
    if (tod.mode === "civil") {
      const civil = Math.min(tod.civilSeconds, 86399);
      const civilU = Math.floor(civil);
      const count = tod.count;
      let lower = count - 1;
      for (let i = 0; i < count && i < 256; i += 1) {
        if (tod.keyframes[i].civilSeconds <= civilU) {
          lower = i;
        }
      }
      const upper = (lower + 1) % count;
      let lowerSeconds = tod.keyframes[lower].civilSeconds;
      let upperSeconds = tod.keyframes[upper].civilSeconds;
      let sampleSeconds = civilU;
      if (upperSeconds <= lowerSeconds) {
        upperSeconds = upperSeconds + 86400;
      }
      if (sampleSeconds < lowerSeconds) {
        sampleSeconds = sampleSeconds + 86400;
      }
      const duration = upperSeconds - lowerSeconds;
      const amount = Math.min(
        1,
        Math.max(0, (sampleSeconds - lowerSeconds) / duration),
      );
      const lowerColor = todAtlasRow(state, tod.keyframes[lower].row);
      const upperColor = todAtlasRow(state, tod.keyframes[upper].row);
      return [
        Math.floor(lowerColor[0] + (upperColor[0] - lowerColor[0]) * amount),
        Math.floor(lowerColor[1] + (upperColor[1] - lowerColor[1]) * amount),
        Math.floor(lowerColor[2] + (upperColor[2] - lowerColor[2]) * amount),
      ];
    }
    return base;
  }

  function todAtlasRow(state, row) {
    const atlas = state.tod.atlas;
    const at = Math.floor(row) * 256 * 4 + state.lastIndex * 4;
    return [atlas[at], atlas[at + 1], atlas[at + 2]];
  }

  function paletteBytes(state, index) {
    state.lastIndex = index;
    return todBytes(state, index);
  }

  function byteSpaceToLinear(color) {
    const r = color[0] / 255.0;
    const g = color[1] / 255.0;
    const b = color[2] / 255.0;
    return [srgbToLinear(r), srgbToLinear(g), srgbToLinear(b)];
  }

  function paletteLinear(state, index) {
    const at = index * 4;
    const palette = state.palette;
    return [
      srgbToLinear(palette[at] / 255.0),
      srgbToLinear(palette[at + 1] / 255.0),
      srgbToLinear(palette[at + 2] / 255.0),
    ];
  }

  function paletteAtTime(state, index) {
    if (!state.tod) {
      return paletteLinear(state, index);
    }
    return byteSpaceToLinear(paletteBytes(state, index));
  }

  /* One animated palette slot in linear light (shader `animated_palette`). */
  function animatedPalette(state, index) {
    const metadata = state.metadata[index];
    const base = paletteAtTime(state, index);
    const phase = fract(
      Math.fround(metadata.phase + Math.fround(state.time * metadata.speed)),
    );
    switch (metadata.effect) {
      case 1: {
        const hsv = rgbToHsv(base);
        return hsvToRgb([
          fract(hsv[0] + Math.fround(metadata.amplitude * phase)),
          hsv[1],
          hsv[2],
        ]);
      }
      case 2: {
        const factor = Math.min(
          2,
          Math.max(
            0,
            Math.fround(
              1.0 +
                Math.fround(
                  metadata.amplitude * Math.sin(TAU * phase),
                ),
            ),
          ),
        );
        return [
          Math.fround(base[0] * factor),
          Math.fround(base[1] * factor),
          Math.fround(base[2] * factor),
        ];
      }
      case 3: {
        const position = metadata.ordinal + phase;
        const lower = Math.floor(position) % metadata.memberCount;
        const upper = (lower + 1) % metadata.memberCount;
        const fraction = fract(position);
        const lowerColor = paletteAtTime(
          state,
          state.cycles[metadata.groupOffset + lower],
        );
        const upperColor = paletteAtTime(
          state,
          state.cycles[metadata.groupOffset + upper],
        );
        return [
          Math.fround(lowerColor[0] + (upperColor[0] - lowerColor[0]) * fraction),
          Math.fround(lowerColor[1] + (upperColor[1] - lowerColor[1]) * fraction),
          Math.fround(lowerColor[2] + (upperColor[2] - lowerColor[2]) * fraction),
        ];
      }
      case 4: {
        const amount = Math.fround(phase * metadata.memberCount);
        const shift = Math.floor(amount) % metadata.memberCount;
        const fraction = Math.floor(fract(amount) * 100.0) / 100.0;
        const current =
          (metadata.ordinal + metadata.memberCount - shift) % metadata.memberCount;
        const previous =
          (current + metadata.memberCount - 1) % metadata.memberCount;
        const currentColor = paletteBytes(
          state,
          state.cycles[metadata.groupOffset + current],
        );
        const previousColor = paletteBytes(
          state,
          state.cycles[metadata.groupOffset + previous],
        );
        const blended = [
          Math.floor(
            currentColor[0] + (previousColor[0] - currentColor[0]) * fraction,
          ),
          Math.floor(
            currentColor[1] + (previousColor[1] - currentColor[1]) * fraction,
          ),
          Math.floor(
            currentColor[2] + (previousColor[2] - currentColor[2]) * fraction,
          ),
        ];
        return byteSpaceToLinear(blended);
      }
      default:
        return base;
    }
  }

  /*
   * Build the state object shared by the twin and the self-check harness.
   *   palette: Uint8Array(256*4) authored sRGB bytes
   *   metadataBytes: Uint8Array(256*32) from Animation::metadata_bytes()
   *   cycleBytes: Uint8Array(256*4) from Animation::cycle_index_bytes()
   *   time: animation phase seconds
   *   tod: null or the record described in `todBytes`
   */
  function createState(options) {
    return {
      palette: options.palette,
      metadata: parseMetadata(options.metadataBytes),
      cycles: parseCycleIndices(options.cycleBytes),
      time: options.time || 0.0,
      tod: options.tod || null,
      lastIndex: 0,
    };
  }

  /* All 256 animated slots as sRGB bytes (RGBA), matching
   * `Animation::apply_palette` byte output for base-palette inputs. */
  function applyPalette(options) {
    const state = createState(options);
    const output = new Uint8Array(256 * 4);
    for (let index = 0; index < 256; index += 1) {
      const color = animatedPalette(state, index);
      output[index * 4] = linearToSrgb(color[0]);
      output[index * 4 + 1] = linearToSrgb(color[1]);
      output[index * 4 + 2] = linearToSrgb(color[2]);
      output[index * 4 + 3] = 255;
    }
    return output;
  }

  return {
    TAU,
    srgbToLinear,
    linearToSrgb,
    parseMetadata,
    parseCycleIndices,
    createState,
    animatedPalette,
    applyPalette,
  };
});
