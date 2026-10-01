/* Worldwall scene-pool curator UI (S12 AC3/AC6/AC7).
 *
 * The preview renders through WebGL2 with the same inputs as the accepted GPU
 * path (decoded indexed pixels, authored RGBA8 palette, `Animation` metadata
 * and cycle-index bytes, time-of-day atlas rows) and one palette-math port of
 * the Worldwall WGSL shader. All large resources are uploaded once per scene;
 * per frame only clock uniforms change. A readback self-check compares the
 * shader's 256-color palette against `curator_palette.js`, the offline-tested
 * twin of the accepted CPU reference.
 */
"use strict";

const state = {
  catalog: null,
  rules: null,
  selected: null,
  previewAssets: new Map(),
  thumbnails: new Map(),
  eligibility: [],
};

const $ = (id) => document.getElementById(id);

function setStatus(message, isError) {
  const status = $("status");
  status.textContent = message;
  status.classList.toggle("error", Boolean(isError));
}

async function fetchJson(url, options) {
  const response = await fetch(url, options);
  const value = await response.json().catch(() => ({}));
  if (!response.ok || value.ok === false) {
    throw new Error(value.error || `${url} failed (${response.status})`);
  }
  return value;
}

async function fetchBinary(url) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url} failed (${response.status})`);
  }
  return response.arrayBuffer();
}

// ---------------------------------------------------------------------------
// Library list, thumbnails, and keyboard navigation.

function renderSceneList() {
  const list = $("scene-list");
  const query = $("search").value.trim().toLowerCase();
  list.replaceChildren();
  for (const scene of state.catalog.scenes) {
    const haystack = `${scene.id} ${scene.title} ${scene.source_collection} ${scene.world} ${scene.appearance}`.toLowerCase();
    if (query && !haystack.includes(query)) {
      continue;
    }
    const item = document.createElement("li");
    item.tabIndex = -1;
    item.setAttribute("role", "option");
    item.setAttribute("aria-selected", String(scene.id === state.selected));
    item.dataset.sceneId = scene.id;
    const thumb = document.createElement("canvas");
    thumb.width = 48;
    thumb.height = 48;
    thumb.setAttribute("aria-hidden", "true");
    item.appendChild(thumb);
    const label = document.createElement("span");
    const name = scene.display_name || scene.title;
    label.textContent = scene.source === "registry" ? `${name} · registry` : name;
    item.appendChild(label);
    const badge = document.createElement("span");
    badge.className = "badge";
    badge.textContent = eligibilityBadge(scene.id);
    item.appendChild(badge);
    item.addEventListener("click", () => selectScene(scene.id));
    list.appendChild(item);
    renderThumbnail(scene.id, thumb);
  }
}

function eligibilityBadge(sceneId) {
  const entry = state.eligibility.find((row) => row.id === sceneId);
  if (!entry) {
    return "…";
  }
  return entry.eligible ? "eligible" : "ineligible";
}

async function renderThumbnail(sceneId, canvas) {
  if (state.thumbnails.has(sceneId)) {
    drawThumbnail(canvas, state.thumbnails.get(sceneId));
    return;
  }
  try {
    const body = await fetchBinary(`preview/${sceneId}/indices`);
    const view = new DataView(body);
    const width = view.getUint32(0);
    const height = view.getUint32(4);
    const indices = new Uint8Array(body, 8);
    const palette = new Uint8Array(await fetchBinary(`preview/${sceneId}/palette`));
    const snapshot = { width, height, indices, palette };
    const pixels = thumbnailPixels(snapshot, canvas.width, canvas.height);
    state.thumbnails.set(sceneId, pixels);
    drawThumbnail(canvas, pixels);
  } catch (error) {
    // Missing assets stay visible as an empty thumbnail with the list label.
  }
}

function thumbnailPixels(snapshot, width, height) {
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const sourceX = Math.min(snapshot.width - 1, Math.floor((x * snapshot.width) / width));
      const sourceY = Math.min(snapshot.height - 1, Math.floor((y * snapshot.height) / height));
      const index = snapshot.indices[sourceY * snapshot.width + sourceX];
      const at = index * 4;
      const to = (y * width + x) * 4;
      pixels[to] = snapshot.palette[at];
      pixels[to + 1] = snapshot.palette[at + 1];
      pixels[to + 2] = snapshot.palette[at + 2];
      pixels[to + 3] = 255;
    }
  }
  return pixels;
}

function drawThumbnail(canvas, pixels) {
  const context = canvas.getContext("2d");
  const image = context.createImageData(canvas.width, canvas.height);
  image.data.set(pixels);
  context.putImageData(image, 0, 0);
}

// ---------------------------------------------------------------------------
// WebGL2 preview: port of `INDEXED_SCENE_SHADER` (src/renderer.rs).

const VERTEX_SHADER = `#version 300 es
void main() {
  vec2 positions[3] = vec2[3](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
  gl_Position = vec4(positions[gl_VertexID], 0.0, 1.0);
}`;

const FRAGMENT_SHADER = `#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;

uniform usampler2D uIndices;
uniform sampler2D uPalette;
uniform usampler2D uMetaA;
uniform sampler2D uMetaB;
uniform usampler2D uCycles;
uniform sampler2D uTodAtlas;
uniform usampler2D uTodKeys;
uniform vec4 uClock;   /* x: animation seconds, y: civil seconds, z: tod count, w: tod enabled */
uniform vec4 uSolar;   /* x: lower row, y: upper row, z: amount, w: mode (0 civil, 1 rows, 2 base) */
uniform vec2 uSource;
uniform vec2 uOutput;
out vec4 outColor;

float srgbToLinear(float value) {
  if (value <= 0.04045) {
    return value / 12.92;
  }
  return pow((value + 0.055) / 1.055, 2.4);
}

float linearToSrgb(float value) {
  float clamped = clamp(value, 0.0, 1.0);
  if (clamped <= 0.0031308) {
    return clamped * 12.92;
  }
  return 1.055 * pow(clamped, 1.0 / 2.4) - 0.055;
}

vec3 byteSpaceToLinear(vec3 color) {
  return vec3(
    srgbToLinear(color.r / 255.0),
    srgbToLinear(color.g / 255.0),
    srgbToLinear(color.b / 255.0)
  );
}

vec3 paletteBytesOf(uint index) {
  return floor(texelFetch(uPalette, ivec2(int(index), 0), 0).rgb * 255.0 + vec3(0.5));
}

vec3 timeOfDayBytes(uint index) {
  if (uClock.w < 0.5 || uSolar.w > 1.5) {
    return paletteBytesOf(index);
  }
  uint count = uint(uClock.z);
  if (count < 2u) {
    return paletteBytesOf(index);
  }
  // ToD preview sampling is injected state: the client computes the keyframe
  // row-pair and amount once per frame (civil slider or solar keyframe walk)
  // and the shader only performs the byte-floor fade between the two rows —
  // the same contract as the runtime's resolved-palette-sample path. Keeping
  // the keyframe search out of the fragment shader avoids the per-fragment
  // loop that crashed browser GPU processes (FM-0018).
  ivec2 rowPair = ivec2(int(uSolar.x), int(uSolar.y));
  float amount = uSolar.z;
  vec3 lowerColor = floor(texelFetch(uTodAtlas, ivec2(int(index), rowPair.x), 0).rgb * 255.0 + vec3(0.5));
  vec3 upperColor = floor(texelFetch(uTodAtlas, ivec2(int(index), rowPair.y), 0).rgb * 255.0 + vec3(0.5));
  return floor(lowerColor + (upperColor - lowerColor) * vec3(amount));
}

vec3 paletteLinear(uint index) {
  vec3 authored = texelFetch(uPalette, ivec2(int(index), 0), 0).rgb;
  return vec3(
    srgbToLinear(authored.r),
    srgbToLinear(authored.g),
    srgbToLinear(authored.b)
  );
}

vec3 paletteAtTime(uint index) {
  if (uClock.w < 0.5) {
    return paletteLinear(index);
  }
  return byteSpaceToLinear(timeOfDayBytes(index));
}

vec3 rgbToHsv(vec3 color) {
  float maximum = max(color.r, max(color.g, color.b));
  float minimum = min(color.r, min(color.g, color.b));
  float delta = maximum - minimum;
  float hue = 0.0;
  if (delta != 0.0) {
    if (maximum == color.r) {
      hue = ((color.g - color.b) / delta / 6.0);
    } else if (maximum == color.g) {
      hue = ((color.b - color.r) / delta + 2.0) / 6.0;
    } else {
      hue = ((color.r - color.g) / delta + 4.0) / 6.0;
    }
  }
  if (hue < 0.0) {
    hue = hue + 1.0;
  }
  float saturation = 0.0;
  if (maximum != 0.0) {
    saturation = delta / maximum;
  }
  return vec3(hue, saturation, maximum);
}

vec3 hsvToRgb(vec3 hsv) {
  float scaled = fract(hsv.x) * 6.0;
  uint sector = uint(floor(scaled)) % 6u;
  float fraction = fract(scaled);
  float p = hsv.z * (1.0 - hsv.y);
  float q = hsv.z * (1.0 - hsv.y * fraction);
  float t = hsv.z * (1.0 - hsv.y * (1.0 - fraction));
  if (sector == 0u) return vec3(hsv.z, t, p);
  if (sector == 1u) return vec3(q, hsv.z, p);
  if (sector == 2u) return vec3(p, hsv.z, t);
  if (sector == 3u) return vec3(p, q, hsv.z);
  if (sector == 4u) return vec3(t, p, hsv.z);
  return vec3(hsv.z, p, q);
}

vec3 animatedPalette(uint index) {
  uvec4 metaA = texelFetch(uMetaA, ivec2(int(index), 0), 0);
  vec4 metaB = texelFetch(uMetaB, ivec2(int(index), 0), 0);
  vec3 base = paletteAtTime(index);
  float phase = fract(metaB.z + uClock.x * metaB.x);
  if (metaA.x == 1u) {
    vec3 hsv = rgbToHsv(base);
    return hsvToRgb(vec3(fract(hsv.x + metaB.y * phase), hsv.y, hsv.z));
  }
  if (metaA.x == 2u) {
    float factor = clamp(1.0 + metaB.y * sin(6.28318530718 * phase), 0.0, 2.0);
    return base * factor;
  }
  if (metaA.x == 3u) {
    float position = float(metaA.w) + phase;
    uint lower = uint(floor(position)) % metaA.z;
    uint upper = (lower + 1u) % metaA.z;
    float fraction = fract(position);
    vec3 lowerColor = paletteAtTime(texelFetch(uCycles, ivec2(int(metaA.y + lower), 0), 0).r);
    vec3 upperColor = paletteAtTime(texelFetch(uCycles, ivec2(int(metaA.y + upper), 0), 0).r);
    return mix(lowerColor, upperColor, vec3(fraction));
  }
  if (metaA.x == 4u) {
    float amount = phase * float(metaA.z);
    uint shift = uint(floor(amount)) % metaA.z;
    float fraction = floor(fract(amount) * 100.0) / 100.0;
    uint current = (metaA.w + metaA.z - shift) % metaA.z;
    uint previous = (current + metaA.z - 1u) % metaA.z;
    vec3 currentColor = timeOfDayBytes(texelFetch(uCycles, ivec2(int(metaA.y + current), 0), 0).r);
    vec3 previousColor = timeOfDayBytes(texelFetch(uCycles, ivec2(int(metaA.y + previous), 0), 0).r);
    vec3 blended = floor(currentColor + (previousColor - currentColor) * vec3(fraction));
    return byteSpaceToLinear(blended);
  }
  return base;
}

void main() {
  // gl_FragCoord is already bottom-left origin, which equals the WGSL
  // shader's mirrored output_position.y (uOutput.y - position.y); using
  // the WGSL position value here would flip the presented image vertically.
  vec2 position = gl_FragCoord.xy;
  float scale = max(uOutput.x / uSource.x, uOutput.y / uSource.y);
  vec2 crop = (uSource * scale - uOutput) * 0.5;
  vec2 sourceCoordinate = floor((position + crop) / scale);
  vec2 sourceMax = uSource - vec2(1.0);
  vec2 clamped = clamp(sourceCoordinate, vec2(0.0), sourceMax);
  ivec2 coordinate = ivec2(clamped.x, sourceMax.y - clamped.y);
  uint index = texelFetch(uIndices, coordinate, 0).r;
  vec3 color = animatedPalette(index);
  outColor = vec4(linearToSrgb(color.r), linearToSrgb(color.g), linearToSrgb(color.b), 1.0);
}`;

const STATIC_METADATA = new Uint32Array(256 * 8);
const STATIC_CYCLES = new Uint32Array(256);

class Preview {
  constructor(canvas) {
    this.canvas = canvas;
    // preserveDrawingBuffer is deliberately off: it forces the compositor to
    // copy and rescale the buffer every frame on this host, which exhausts the
    // GPU process and kills the context within seconds (FM-0018). The preview
    // redraws every frame, so no preserved buffer is needed.
    this.gl = canvas.getContext("webgl2", { antialias: false, preserveDrawingBuffer: false });
    if (!this.gl) {
      throw new Error("WebGL2 is required for the indexed preview");
    }
    canvas.addEventListener("webglcontextlost", (event) => {
      event.preventDefault();
      const status = $("self-check");
      if (status) {
        status.className = "no";
        status.textContent = "Preview: the browser lost the WebGL context; reload the page to restore the preview.";
      }
    });
    const gl = this.gl;
    this.program = this.buildProgram(VERTEX_SHADER, FRAGMENT_SHADER);
    gl.useProgram(this.program);
    this.stripProgram = this.program;
    this.uniforms = {};
    for (const name of [
      "uClock", "uSolar", "uSource", "uOutput",
    ]) {
      this.uniforms[name] = gl.getUniformLocation(this.program, name);
    }
    const units = {
      uIndices: 0, uPalette: 1, uMetaA: 2, uMetaB: 3,
      uCycles: 4, uTodAtlas: 5, uTodKeys: 6,
    };
    for (const [name, unit] of Object.entries(units)) {
      gl.uniform1i(gl.getUniformLocation(this.program, name), unit);
    }
    this.emptyTexture = this.makeTexture(new Uint8Array([0, 0, 0, 255]), 1, 1, "rgba8");
    this.strip = this.createStripTarget();
    this.scene = null;
  }

  buildProgram(vertexSource, fragmentSource) {
    const gl = this.gl;
    const compile = (type, source) => {
      const shader = gl.createShader(type);
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
        throw new Error(`preview shader failed: ${gl.getShaderInfoLog(shader)}`);
      }
      return shader;
    };
    const program = gl.createProgram();
    gl.attachShader(program, compile(gl.VERTEX_SHADER, vertexSource));
    gl.attachShader(program, compile(gl.FRAGMENT_SHADER, fragmentSource));
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(`preview program failed: ${gl.getProgramInfoLog(program)}`);
    }
    return program;
  }

  makeTexture(data, width, height, format) {
    const gl = this.gl;
    // Indexed R8UI rows may have arbitrary byte widths; the WebGL default
    // unpack alignment of four would otherwise introduce row padding.
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    if (format === "r8ui") {
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8UI, width, height, 0, gl.RED_INTEGER, gl.UNSIGNED_BYTE, data);
    } else if (format === "rgba32ui") {
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32UI, width, height, 0, gl.RGBA_INTEGER, gl.UNSIGNED_INT, data);
    } else if (format === "r32ui") {
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.R32UI, width, height, 0, gl.RED_INTEGER, gl.UNSIGNED_INT, data);
    } else if (format === "rgba32f") {
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, width, height, 0, gl.RGBA, gl.FLOAT, data);
    } else {
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, data);
    }
    return texture;
  }

  createStripTarget() {
    const gl = this.gl;
    const width = 32;
    const height = 8;
    const indices = new Uint8Array(width * height);
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        // Multiplication by 7 permutes all 256 indices (full palette
        // coverage), and the 2-D layout is asymmetric under vertical or
        // horizontal flips so the self-check also verifies spatial mapping —
        // a single-row strip cannot see a Y orientation bug.
        indices[y * width + x] = ((y * 32 + x) * 7) & 255;
      }
    }
    const target = {
      framebuffer: gl.createFramebuffer(),
      texture: this.makeTexture(indices, width, height, "r8ui"),
      width,
      height,
      indices,
    };
    gl.bindFramebuffer(gl.FRAMEBUFFER, target.framebuffer);
    const output = this.makeTexture(new Uint8Array(width * height * 4), width, height, "rgba8");
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, output, 0);
    target.output = output;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    return target;
  }

  /* Upload every large resource once; per frame only uniforms change. */
  load(assets) {
    const gl = this.gl;
    this.destroy();
    this.scene = {
      indices: this.makeTexture(assets.indices, assets.width, assets.height, "r8ui"),
      palette: this.makeTexture(assets.palette, 256, 1, "rgba8"),
      metaA: this.makeTexture(assets.metaA, 256, 1, "rgba32ui"),
      metaB: this.makeTexture(assets.metaB, 256, 1, "rgba32f"),
      cycles: this.makeTexture(assets.cycles, 256, 1, "r32ui"),
      todAtlas: assets.todAtlas
        ? this.makeTexture(assets.todAtlas.rgba, 256, assets.todAtlas.rows, "rgba8")
        : this.emptyTexture,
      todKeys: this.makeTexture(assets.todKeys || new Uint32Array(256 * 4), 256, 1, "rgba32ui"),
      width: assets.width,
      height: assets.height,
    };
    void gl;
  }

  destroy() {
    if (!this.scene) {
      return;
    }
    const gl = this.gl;
    for (const key of ["indices", "palette", "metaA", "metaB", "cycles", "todAtlas", "todKeys"]) {
      if (this.scene[key] && this.scene[key] !== this.emptyTexture) {
        gl.deleteTexture(this.scene[key]);
      }
    }
    this.scene = null;
  }

  draw(clock) {
    if (!this.scene) {
      return;
    }
    const gl = this.gl;
    const width = this.canvas.width;
    const height = this.canvas.height;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, width, height);
    gl.useProgram(this.program);
    const scene = this.scene;
    const bind = (unit, texture) => {
      gl.activeTexture(gl.TEXTURE0 + unit);
      gl.bindTexture(gl.TEXTURE_2D, texture);
    };
    bind(0, scene.indices);
    bind(1, scene.palette);
    bind(2, scene.metaA);
    bind(3, scene.metaB);
    bind(4, scene.cycles);
    bind(5, scene.todAtlas);
    bind(6, scene.todKeys);
    gl.uniform4f(this.uniforms.uClock, clock.animationSeconds, clock.civilSeconds, clock.todCount, clock.todEnabled);
    gl.uniform4f(this.uniforms.uSolar, clock.lowerRow, clock.upperRow, clock.amount, clock.mode);
    gl.uniform2f(this.uniforms.uSource, scene.width, scene.height);
    gl.uniform2f(this.uniforms.uOutput, width, height);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  /* Read back the shader's rendered index grid for the fidelity self-check.
   * Returns top-down rows plus the expected index per texel. */
  readStrip(clock) {
    const gl = this.gl;
    const scene = this.scene;
    const strip = this.strip;
    if (!scene) {
      return null;
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, strip.framebuffer);
    gl.viewport(0, 0, strip.width, strip.height);
    gl.useProgram(this.program);
    const bind = (unit, texture) => {
      gl.activeTexture(gl.TEXTURE0 + unit);
      gl.bindTexture(gl.TEXTURE_2D, texture);
    };
    bind(0, strip.texture);
    bind(1, scene.palette);
    bind(2, scene.metaA);
    bind(3, scene.metaB);
    bind(4, scene.cycles);
    bind(5, scene.todAtlas);
    bind(6, scene.todKeys);
    gl.uniform4f(this.uniforms.uClock, clock.animationSeconds, clock.civilSeconds, clock.todCount, clock.todEnabled);
    gl.uniform4f(this.uniforms.uSolar, clock.lowerRow, clock.upperRow, clock.amount, clock.mode);
    gl.uniform2f(this.uniforms.uSource, strip.width, strip.height);
    gl.uniform2f(this.uniforms.uOutput, strip.width, strip.height);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    const raw = new Uint8Array(strip.width * strip.height * 4);
    gl.readPixels(0, 0, strip.width, strip.height, gl.RGBA, gl.UNSIGNED_BYTE, raw);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    // readPixels returns bottom-up rows; present top-down rows to the check.
    const pixels = new Uint8Array(raw.length);
    for (let y = 0; y < strip.height; y += 1) {
      const source = (strip.height - 1 - y) * strip.width * 4;
      pixels.set(raw.subarray(source, source + strip.width * 4), y * strip.width * 4);
    }
    return { width: strip.width, height: strip.height, indices: strip.indices, pixels };
  }
}

// ---------------------------------------------------------------------------
// Preview loading, playback, ToD scrub, and the fidelity self-check.

let preview = null;
let playback = { started: performance.now(), playing: true };

function parseMetadataBytes(buffer) {
  return new Uint8Array(buffer);
}

async function selectScene(sceneId) {
  state.selected = sceneId;
  renderSceneList();
  const scene = state.catalog.scenes.find((entry) => entry.id === sceneId);
  $("scene-title").textContent = scene
    ? `${scene.display_name || scene.title} (${scene.id})`
    : sceneId;
  try {
    const [indices, palette, metadata, cycles, info] = await Promise.all([
      fetchBinary(`preview/${sceneId}/indices`),
      fetchBinary(`preview/${sceneId}/palette`),
      fetchBinary(`preview/${sceneId}/animation`),
      fetchBinary(`preview/${sceneId}/cycle-indices`),
      fetchJson(`preview/${sceneId}/info`),
    ]);
    if (state.selected !== sceneId) {
      return;
    }
    const indexView = new DataView(indices);
    const width = indexView.getUint32(0);
    const height = indexView.getUint32(4);
    const metadataBytes = parseMetadataBytes(metadata);
    const metaA = new Uint32Array(256 * 4);
    const metaB = new Float32Array(256 * 4);
    const metaView = new DataView(metadata);
    for (let index = 0; index < 256; index += 1) {
      const at = index * 32;
      metaA[index * 4] = metaView.getUint32(at, true);
      metaA[index * 4 + 1] = metaView.getUint32(at + 4, true);
      metaA[index * 4 + 2] = metaView.getUint32(at + 8, true);
      metaA[index * 4 + 3] = metaView.getUint32(at + 12, true);
      metaB[index * 4] = metaView.getFloat32(at + 16, true);
      metaB[index * 4 + 1] = metaView.getFloat32(at + 20, true);
      metaB[index * 4 + 2] = metaView.getFloat32(at + 24, true);
      metaB[index * 4 + 3] = metaView.getFloat32(at + 28, true);
    }
    const cycleView = new DataView(cycles);
    const cycleArray = new Uint32Array(256);
    for (let index = 0; index < 256; index += 1) {
      cycleArray[index] = cycleView.getUint32(index * 4, true);
    }
    let todAtlas = null;
    let todKeysArray = new Uint32Array(256 * 4);
    let todCount = 0;
    let todSolar = false;
    if (info.time_of_day) {
      const atlasBody = await fetchBinary(`preview/${sceneId}/tod-atlas`);
      const atlasView = new DataView(atlasBody);
      const rows = atlasView.getUint32(0);
      todAtlas = { rows, rgba: new Uint8Array(atlasBody, 4) };
      const keyframes = await fetchJson(`preview/${sceneId}/tod-keyframes`);
      todSolar = keyframes.solar;
      todCount = keyframes.keyframes.length;
      keyframes.keyframes.forEach((keyframe, index) => {
        // Solar keyframes are preview-injected state: scrubbing walks the
        // authored keyframe order instead of resolving runtime solar events.
        todKeysArray[index * 4] = keyframe.civil_seconds ?? index;
        todKeysArray[index * 4 + 1] = keyframe.row;
      });
    }
    if (state.selected !== sceneId) {
      return;
    }
    state.previewAssets.clear();
    state.previewAssets.set(sceneId, {
      width,
      height,
      indices: new Uint8Array(indices, 8),
      palette: new Uint8Array(palette),
      metaA,
      metaB,
      metadataBytes,
      cycles: cycleArray,
      cycleBytes: new Uint8Array(cycles),
      todAtlas,
      todKeysArray,
      todCount,
      todSolar,
    });
    preview.load(state.previewAssets.get(sceneId));
    playback.started = performance.now();
    $("scene-info").textContent = describeScene(scene, info);
    updateRuleEditor();
    refreshEligibility();
    runSelfCheck();
  } catch (error) {
    setStatus(`Preview failed: ${error.message}`, true);
  }
}

function describeScene(scene, info) {
  const parts = [
    `${info.width}×${info.height}`,
    `license ${scene.license_state}`,
    scene.animation_manifest_present ? "authored animation" : "static (no authored animation)",
  ];
  if (scene.cycle_semantics) {
    parts.push(`cycles ${scene.cycle_semantics}`);
  }
  if (info.time_of_day) {
    parts.push(`time-of-day atlas ${info.time_of_day.rows} rows (${info.time_of_day.solar ? "solar v2" : "civil v1"})`);
  }
  return parts.join(" · ");
}

function todRowPair(assets, civilSeconds) {
  // Client-side port of the WGSL civil keyframe search (FM-0018 fix): runs
  // once per frame instead of per fragment.
  const count = Math.min(assets.todCount, 256);
  const civil = Math.min(Math.floor(civilSeconds), 86399);
  let lower = count - 1;
  for (let i = 0; i < count; i += 1) {
    if (assets.todKeysArray[i * 4] <= civil) {
      lower = i;
    }
  }
  const upper = (lower + 1) % count;
  let lowerSeconds = assets.todKeysArray[lower * 4];
  let upperSeconds = assets.todKeysArray[upper * 4];
  let sampleSeconds = civil;
  if (upperSeconds <= lowerSeconds) {
    upperSeconds += 86400;
  }
  if (sampleSeconds < lowerSeconds) {
    sampleSeconds += 86400;
  }
  const duration = upperSeconds - lowerSeconds;
  const amount = Math.min(1, Math.max(0, (sampleSeconds - lowerSeconds) / duration));
  return {
    lowerRow: assets.todKeysArray[lower * 4 + 1],
    upperRow: assets.todKeysArray[upper * 4 + 1],
    amount,
  };
}

function previewTimeClass(civilSeconds) {
  // Preview-only approximation of the time-of-day classes used by rules; the
  // curator has no location by policy, so solar-derived classes (polar_*)
  // never match from the slider.
  const hour = civilSeconds / 3600;
  if (hour < 5 || hour >= 21) return "night";
  if (hour < 8) return "dawn";
  if (hour < 17) return "day";
  return "dusk";
}

function currentClock() {
  const assets = state.selected && state.previewAssets.get(state.selected);
  const animationSeconds = $("play").checked
    ? (performance.now() - playback.started) / 1000
    : 0;
  const clock = {
    animationSeconds,
    civilSeconds: Number($("tod").value),
    todCount: 0,
    todEnabled: 0,
    lowerRow: 0,
    upperRow: 0,
    amount: 0,
    mode: 2,
  };
  if (assets && assets.todAtlas && assets.todCount >= 2) {
    clock.todEnabled = 1;
    clock.todCount = assets.todCount;
    clock.mode = 1;
    if (assets.todSolar) {
      // Injected preview state: interpolate between adjacent authored rows in
      // keyframe order. This is preview scrubbing, not runtime selection.
      const span = 86400 / assets.todCount;
      const position = clock.civilSeconds / span;
      const lower = Math.floor(position) % assets.todCount;
      const upper = (lower + 1) % assets.todCount;
      clock.lowerRow = assets.todKeysArray[lower * 4 + 1];
      clock.upperRow = assets.todKeysArray[upper * 4 + 1];
      clock.amount = position - Math.floor(position);
    } else {
      const pair = todRowPair(assets, clock.civilSeconds);
      clock.lowerRow = pair.lowerRow;
      clock.upperRow = pair.upperRow;
      clock.amount = pair.amount;
    }
  }
  return clock;
}

function runSelfCheck() {
  const assets = state.selected && state.previewAssets.get(state.selected);
  const target = $("self-check");
  if (!assets || typeof CuratorPalette === "undefined") {
    target.textContent = "";
    return;
  }
  const probes = [0, 0.37, 1.85];
  let worst = 0;
  let checked = 0;
  for (const time of probes) {
    const clock = currentClock();
    clock.animationSeconds = time;
    clock.mode = 2; /* base palette; the twin's applyPalette contract */
    clock.todEnabled = 0;
    const strip = preview.readStrip(clock);
    const expected = CuratorPalette.applyPalette({
      palette: assets.palette,
      metadataBytes: assets.metadataBytes,
      cycleBytes: assets.cycleBytes,
      time,
      tod: null,
    });
    for (let y = 0; y < strip.height; y += 1) {
      for (let x = 0; x < strip.width; x += 1) {
        const index = strip.indices[y * strip.width + x];
        const at = (y * strip.width + x) * 4;
        for (let channel = 0; channel < 4; channel += 1) {
          worst = Math.max(worst, Math.abs(strip.pixels[at + channel] - expected[index * 4 + channel]));
        }
        checked += 1;
      }
    }
  }
  target.className = worst === 0 ? "ok" : "no";
  target.textContent = worst === 0
    ? `Preview self-check: shader palette and spatial mapping match the reference twin exactly (${checked} texels × ${probes.length} probe times).`
    : `Preview self-check: shader output differs from the twin by up to ${worst}/255 (palette or spatial mapping).`;
}

function frame() {
  const assets = state.selected && state.previewAssets.get(state.selected);
  if (preview && assets) {
    preview.draw(currentClock());
  }
  requestAnimationFrame(frame);
}

// ---------------------------------------------------------------------------
// Eligibility, rules editing, save, and explicit reload.

async function refreshEligibility() {
  // The ToD slider is the preview's time input; the snapshot has no separate
  // time field (preview-only class approximation, see previewTimeClass).
  const time = previewTimeClass(Number($("tod").value));
  const weather = $("snap-weather").value;
  const temperature = $("snap-temp").value === "" ? "unknown" : $("snap-temp").value;
  try {
    const result = await fetchJson(`eligibility?time=${time}&weather=${weather}&temperature=${temperature}`);
    state.eligibility = result.scenes;
    const list = $("eligibility");
    list.replaceChildren();
    for (const entry of result.scenes) {
      const item = document.createElement("li");
      const badge = document.createElement("span");
      badge.className = `badge ${entry.eligible ? "ok" : "no"}`;
      badge.textContent = entry.eligible ? "eligible" : "ineligible";
      item.appendChild(badge);
      item.appendChild(document.createTextNode(` ${entry.id}: `));
      const reasonText = entry.eligible
        ? "matches the preview snapshot"
        : entry.reasons.map((reason) => reason.detail).join("; ");
      const reasons = document.createElement("span");
      reasons.className = "hint";
      reasons.textContent = reasonText;
      item.appendChild(reasons);
      list.appendChild(item);
    }
    renderSceneList();
    updateRuleEditor();
  } catch (error) {
    setStatus(`Eligibility failed: ${error.message}`, true);
  }
}

function ruleFor(sceneId) {
  if (!state.rules) {
    return null;
  }
  return state.rules.scene_rules.find((rule) => rule.scene_id === sceneId) || null;
}

function updateRuleEditor() {
  const editor = $("rule-editor");
  editor.replaceChildren();
  const sceneId = state.selected;
  if (!sceneId) {
    editor.textContent = "Select a scene to edit its availability rule.";
    return;
  }
  if (!state.rules) {
    editor.textContent = "No sidecar is configured; use Save sidecar to create one with one pool per source collection.";
    return;
  }
  const rule = ruleFor(sceneId) || {
    scene_id: sceneId,
    enabled: true,
    time_of_day: null,
    weather_any: null,
    temperature_min_c: null,
    temperature_max_c: null,
  };

  // Curator-authored display name (library scenes only; provenance titles are
  // never rewritten).
  const known = state.catalog.scenes.find((entry) => entry.id === sceneId);
  const nameRow = document.createElement("div");
  nameRow.className = "row";
  const nameLabel = document.createElement("label");
  nameLabel.textContent = "Display name ";
  const nameInput = document.createElement("input");
  nameInput.id = "rule-name";
  nameInput.type = "text";
  nameInput.maxLength = 200;
  nameInput.value = (known && known.display_name) || "";
  nameInput.placeholder = known ? known.title : "";
  if (!known || known.source !== "library") {
    nameInput.disabled = true;
    nameInput.placeholder = "library scenes only";
  }
  nameLabel.appendChild(nameInput);
  nameRow.appendChild(nameLabel);
  editor.appendChild(nameRow);
  const nameHint = document.createElement("div");
  nameHint.className = "hint";
  nameHint.textContent =
    "Shown in the list and in worldwallctl scenes; source titles stay untouched.";
  editor.appendChild(nameHint);

  const enabled = document.createElement("label");
  const enabledInput = document.createElement("input");
  enabledInput.type = "checkbox";
  enabledInput.id = "rule-enabled";
  enabledInput.checked = rule.enabled;
  enabled.appendChild(enabledInput);
  enabled.appendChild(document.createTextNode(" Scene enabled"));
  editor.appendChild(enabled);

  for (const [key, values] of [
    ["rule-time", ["dawn", "day", "dusk", "night", "polar_day", "polar_night"]],
    ["rule-weather", ["clear", "cloudy", "rain", "snow", "storm", "fog"]],
  ]) {
    const field = document.createElement("fieldset");
    const legend = document.createElement("legend");
    legend.textContent = key === "rule-time" ? "Time of day" : "Weather (any of)";
    field.appendChild(legend);
    const row = document.createElement("div");
    row.className = "row";
    const configured = key === "rule-time" ? rule.time_of_day : rule.weather_any;
    for (const value of values) {
      const label = document.createElement("label");
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.dataset.ruleKey = key;
      checkbox.value = value;
      checkbox.checked = Array.isArray(configured) && configured.includes(value);
      label.appendChild(checkbox);
      label.appendChild(document.createTextNode(value));
      row.appendChild(label);
    }
    field.appendChild(row);
    editor.appendChild(field);
  }

  const temperatures = document.createElement("div");
  temperatures.className = "row";
  for (const [id, labelText, value] of [
    ["rule-temp-min", "Min °C", rule.temperature_min_c],
    ["rule-temp-max", "Max °C", rule.temperature_max_c],
  ]) {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "number";
    input.step = "0.5";
    input.style.width = "5rem";
    input.id = id;
    input.value = value === null || value === undefined ? "" : String(value);
    label.appendChild(document.createTextNode(labelText));
    label.appendChild(input);
    temperatures.appendChild(label);
  }
  editor.appendChild(temperatures);
}

function collectRulesPayload() {
  const sceneId = state.selected;
  const rules = (state.rules ? state.rules.scene_rules : []).filter((rule) => rule.scene_id !== sceneId);
  const enabledInput = $("rule-enabled");
  if (sceneId && state.rules && enabledInput) {
    const times = Array.from(document.querySelectorAll('[data-rule-key="rule-time"]:checked')).map((box) => box.value);
    const weathers = Array.from(document.querySelectorAll('[data-rule-key="rule-weather"]:checked')).map((box) => box.value);
    const minimum = $("rule-temp-min").value === "" ? null : Number($("rule-temp-min").value);
    const maximum = $("rule-temp-max").value === "" ? null : Number($("rule-temp-max").value);
    rules.push({
      scene_id: sceneId,
      enabled: $("rule-enabled").checked,
      time_of_day: times.length ? times : null,
      weather_any: weathers.length ? weathers : null,
      temperature_min_c: minimum,
      temperature_max_c: maximum,
    });
  }
  return {
    pools: state.rules.pools.map((pool) => ({ ...pool })),
    scene_rules: rules,
  };
}

function defaultPools(existingPools = []) {
  return CuratorPools.defaultPools(state.catalog.scenes, existingPools);
}

function includeAvailableScenes() {
  const before = new Set(state.rules.pools.flatMap((pool) => pool.scene_ids));
  state.rules.pools = defaultPools(state.rules.pools);
  const added = state.rules.pools
    .flatMap((pool) => pool.scene_ids)
    .filter((id) => !before.has(id)).length;
  renderPools();
  setStatus(
    `${added} available scene(s) added to the draft. Save the sidecar, then request config reload to apply it.`,
  );
}

function renderPools() {
  const container = $("pools");
  container.replaceChildren();
  const note = $("pool-defaults-note");
  note.textContent = state.catalog.rules_present
    ? "Unassigned scenes can be added without changing existing pool switches or scene rules. New pools default enabled; scenes have no extra criteria. Save and request config reload to apply."
    : "Suggested defaults include every loaded scene in enabled pools with no extra criteria. This is an unsaved draft; save it, ensure worldwall.toml references this sidecar, then request config reload.";
  if (!state.rules) {
    container.textContent = "No sidecar yet.";
    return;
  }
  const assigned = new Set(state.rules.pools.flatMap((pool) => pool.scene_ids));
  const available = state.catalog.scenes.filter((scene) => !assigned.has(scene.id)).length;
  if (available > 0) {
    const add = document.createElement("button");
    add.type = "button";
    add.textContent = `Add ${available} unassigned scene(s)`;
    add.addEventListener("click", includeAvailableScenes);
    container.appendChild(add);
  }
  for (const pool of state.rules.pools) {
    const row = document.createElement("div");
    row.className = "row";
    const label = document.createElement("label");
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = pool.enabled;
    checkbox.addEventListener("change", () => {
      pool.enabled = checkbox.checked;
    });
    label.appendChild(checkbox);
    label.appendChild(document.createTextNode(` ${pool.id} (${pool.source}, ${pool.scene_ids.length} packages)`));
    row.appendChild(label);
    container.appendChild(row);
  }
}

async function saveRules() {
  if (!state.rules) {
    state.rules = { pools: defaultPools(), scene_rules: [] };
    renderPools();
  }
  try {
    const result = await fetchJson("rules", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(collectRulesPayload()),
    });
    const nameInput = $("rule-name");
    if (nameInput && !nameInput.disabled && state.selected) {
      await fetchJson("name", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          id: state.selected,
          display_name: nameInput.value.trim() || null,
        }),
      });
    }
    setStatus(result.message || "Sidecar saved.");
    await loadCatalog();
    await refreshEligibility();
  } catch (error) {
    setStatus(`Save failed: ${error.message}`, true);
  }
}

async function requestReload() {
  try {
    const result = await fetchJson("reload-config", { method: "POST" });
    const response = result.response || {};
    setStatus(
      response.ok ? "Config reload applied (no scene was selected)." : `Config reload rejected: ${JSON.stringify(response)}`,
      !response.ok,
    );
  } catch (error) {
    setStatus(`Config reload failed: ${error.message}`, true);
  }
}

async function loadCatalog() {
  const catalog = await fetchJson("catalog");
  state.catalog = catalog;
  state.rules = catalog.rules || state.rules;
  if (!state.rules) {
    state.rules = { pools: defaultPools(), scene_rules: [] };
  }
  $("rules-path").textContent = `Sidecar: ${catalog.rules_path}${catalog.rules_present ? "" : " (not created yet)"}`;
  renderPools();
  renderSceneList();
  const libraryCount = catalog.scenes.filter((scene) => scene.source === "library").length;
  const registryCount = catalog.scenes.length - libraryCount;
  setStatus(
    `${catalog.rules_present ? "Catalog and sidecar loaded" : "Suggested defaults loaded as an unsaved draft"} ` +
      `(${libraryCount} library, ${registryCount} registry scenes; ${catalog.registry_note}).`,
  );
}

function installKeyboardNavigation() {
  const list = $("scene-list");
  list.addEventListener("keydown", (event) => {
    const visible = Array.from(list.children);
    const index = visible.findIndex((item) => item.dataset.sceneId === state.selected);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : -1;
      const next = visible[Math.min(visible.length - 1, Math.max(0, (index < 0 ? 0 : index + delta)))];
      if (next) {
        next.scrollIntoView({ block: "nearest" });
        selectScene(next.dataset.sceneId);
      }
    } else if (event.key === "Enter" && state.selected) {
      selectScene(state.selected);
    }
  });
}

async function main() {
  try {
    preview = new Preview($("preview"));
  } catch (error) {
    setStatus(error.message, true);
    return;
  }
  installKeyboardNavigation();
  $("search").addEventListener("input", renderSceneList);
  for (const id of ["snap-weather", "snap-temp"]) {
    $(id).addEventListener("change", refreshEligibility);
  }
  $("tod").addEventListener("input", () => {
    const seconds = Number($("tod").value);
    $("tod-label").textContent =
      `${String(Math.floor(seconds / 3600)).padStart(2, "0")}:${String(Math.floor((seconds % 3600) / 60)).padStart(2, "0")}`;
    runSelfCheck();
    refreshEligibility();
  });
  $("play").addEventListener("change", () => {
    playback.started = performance.now();
  });
  $("save").addEventListener("click", saveRules);
  $("reload").addEventListener("click", requestReload);
  await loadCatalog();
  await refreshEligibility();
  if (state.catalog.scenes.length > 0) {
    await selectScene(state.catalog.scenes[0].id);
  }
  requestAnimationFrame(frame);
}

main();
