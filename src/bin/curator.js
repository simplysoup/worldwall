'use strict';

const search = document.getElementById('search');
const list = document.getElementById('scene-list');
const sceneCount = document.getElementById('scene-count');
const title = document.getElementById('selected-title');
const sceneId = document.getElementById('selected-id');
const details = document.getElementById('scene-details');
const previewStatus = document.getElementById('preview-status');
const rulesSummary = document.getElementById('rules-summary');
const loadStatus = document.getElementById('load-status');
const canvas = document.getElementById('preview');
const gl = canvas.getContext('webgl2', { alpha: false, antialias: false, depth: false, stencil: false, preserveDrawingBuffer: false });
let scenes = [];
let selectedId = null;
let indexTexture = null;
let paletteTexture = null;
let imageWidth = 0;
let imageHeight = 0;
let program = null;
let vao = null;
let resizeObserver = null;

const vertexSource = `#version 300 es
const vec2 positions[3] = vec2[3](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
void main() { gl_Position = vec4(positions[gl_VertexID], 0.0, 1.0); }
`;
const fragmentSource = `#version 300 es
precision highp float;
precision highp int;
uniform highp usampler2D uIndices;
uniform highp sampler2D uPalette;
uniform vec2 uCanvasSize;
uniform vec2 uImageSize;
out vec4 outColor;
void main() {
  vec2 uv = gl_FragCoord.xy / uCanvasSize;
  uv.y = 1.0 - uv.y;
  float outputAspect = uCanvasSize.x / uCanvasSize.y;
  float imageAspect = uImageSize.x / uImageSize.y;
  if (outputAspect > imageAspect) {
    float visibleHeight = imageAspect / outputAspect;
    uv.y = (uv.y - (1.0 - visibleHeight) * 0.5) / visibleHeight;
  } else {
    float visibleWidth = outputAspect / imageAspect;
    uv.x = (uv.x - (1.0 - visibleWidth) * 0.5) / visibleWidth;
  }
  ivec2 source = ivec2(floor(uv * uImageSize));
  source = clamp(source, ivec2(0), ivec2(uImageSize) - ivec2(1));
  uint index = texelFetch(uIndices, source, 0).r;
  vec3 authoredSrgb = texelFetch(uPalette, ivec2(int(index), 0), 0).rgb;
  outColor = vec4(authoredSrgb, 1.0);
}
`;

function setStatus(message, isError = false) {
  loadStatus.textContent = message;
  loadStatus.style.color = isError ? '#ffad9e' : '#b4c1ca';
}

function appendDetail(label, value) {
  const dt = document.createElement('dt');
  const dd = document.createElement('dd');
  dt.textContent = label;
  dd.textContent = value == null || value === '' ? 'Not recorded' : String(value);
  details.append(dt, dd);
}

function createProgram() {
  if (!gl) throw new Error('Firefox did not provide WebGL2; indexed GPU preview is unavailable.');
  const compile = (kind, source) => {
    const shader = gl.createShader(kind);
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      const message = gl.getShaderInfoLog(shader) || 'unknown shader compile error';
      gl.deleteShader(shader);
      throw new Error(message);
    }
    return shader;
  };
  const vertex = compile(gl.VERTEX_SHADER, vertexSource);
  const fragment = compile(gl.FRAGMENT_SHADER, fragmentSource);
  const result = gl.createProgram();
  gl.attachShader(result, vertex);
  gl.attachShader(result, fragment);
  gl.linkProgram(result);
  gl.deleteShader(vertex);
  gl.deleteShader(fragment);
  if (!gl.getProgramParameter(result, gl.LINK_STATUS)) {
    const message = gl.getProgramInfoLog(result) || 'unknown shader link error';
    gl.deleteProgram(result);
    throw new Error(message);
  }
  program = result;
  vao = gl.createVertexArray();
}

function resizeCanvas() {
  if (!gl || !program || !imageWidth || !imageHeight) return;
  const rect = canvas.getBoundingClientRect();
  const scale = Math.min(window.devicePixelRatio || 1, 2);
  const width = Math.max(1, Math.round(rect.width * scale));
  const height = Math.max(1, Math.round(rect.height * scale));
  if (canvas.width === width && canvas.height === height) return;
  canvas.width = width;
  canvas.height = height;
  gl.viewport(0, 0, width, height);
  drawPreview();
}

function drawPreview() {
  if (!gl || !program || !indexTexture || !paletteTexture) return;
  gl.useProgram(program);
  gl.bindVertexArray(vao);
  gl.activeTexture(gl.TEXTURE0);
  gl.bindTexture(gl.TEXTURE_2D, indexTexture);
  gl.uniform1i(gl.getUniformLocation(program, 'uIndices'), 0);
  gl.activeTexture(gl.TEXTURE1);
  gl.bindTexture(gl.TEXTURE_2D, paletteTexture);
  gl.uniform1i(gl.getUniformLocation(program, 'uPalette'), 1);
  gl.uniform2f(gl.getUniformLocation(program, 'uCanvasSize'), canvas.width, canvas.height);
  gl.uniform2f(gl.getUniformLocation(program, 'uImageSize'), imageWidth, imageHeight);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
}

async function loadPreview(scene) {
  const requestedId = scene.id;
  previewStatus.textContent = `Loading indexed pixels for ${requestedId}…`;
  if (!gl) throw new Error('WebGL2 is unavailable in this Firefox session.');
  if (!program) createProgram();
  const [indicesResponse, paletteResponse] = await Promise.all([
    fetch(`preview/${encodeURIComponent(scene.id)}/indices`, { cache: 'no-store' }),
    fetch(`preview/${encodeURIComponent(scene.id)}/palette`, { cache: 'no-store' }),
  ]);
  if (!indicesResponse.ok) throw new Error(`Scene PNG decode failed (${indicesResponse.status}).`);
  if (!paletteResponse.ok) throw new Error(`Palette PNG decode failed (${paletteResponse.status}).`);
  const packed = await indicesResponse.arrayBuffer();
  const palette = new Uint8Array(await paletteResponse.arrayBuffer());
  if (selectedId !== requestedId) return;
  if (packed.byteLength < 8 || palette.byteLength !== 1024) throw new Error('Preview data has an invalid size.');
  const header = new DataView(packed);
  imageWidth = header.getUint32(0, false);
  imageHeight = header.getUint32(4, false);
  const indices = new Uint8Array(packed, 8);
  if (!imageWidth || !imageHeight || indices.length !== imageWidth * imageHeight) throw new Error('Indexed image dimensions do not match its pixel data.');

  if (indexTexture) gl.deleteTexture(indexTexture);
  if (paletteTexture) gl.deleteTexture(paletteTexture);
  indexTexture = gl.createTexture();
  gl.activeTexture(gl.TEXTURE0);
  gl.bindTexture(gl.TEXTURE_2D, indexTexture);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8UI, imageWidth, imageHeight, 0, gl.RED_INTEGER, gl.UNSIGNED_BYTE, indices);

  paletteTexture = gl.createTexture();
  gl.activeTexture(gl.TEXTURE1);
  gl.bindTexture(gl.TEXTURE_2D, paletteTexture);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, 256, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, palette);
  resizeCanvas();
  drawPreview();
  const animationNote = scene.animation_manifest_present ? ' An animation manifest is recorded but not applied in this slice.' : '';
  previewStatus.textContent = `Static indexed GPU preview · ${imageWidth} × ${imageHeight}.${animationNote}`;
}

function showScene(scene) {
  selectedId = scene.id;
  for (const button of list.querySelectorAll('.scene-option')) {
    button.setAttribute('aria-selected', String(button.dataset.sceneId === selectedId));
  }
  title.textContent = scene.title || scene.id;
  sceneId.textContent = scene.id;
  details.replaceChildren();
  appendDetail('Source collection', scene.source_collection);
  appendDetail('World relationship', scene.world);
  appendDetail('Appearance relationship', scene.appearance);
  appendDetail('License state', scene.license_state);
  appendDetail('Source-native cycles', scene.cycle_semantics || 'No cycle metadata recorded');
  setStatus('Preview is read-only. No scene selection or Worldwall IPC is performed.');
  loadPreview(scene).catch(error => {
    if (selectedId !== scene.id) return;
    previewStatus.textContent = 'Preview unavailable.';
    setStatus(error.message || String(error), true);
  });
}

function renderList() {
  const query = search.value.trim().toLocaleLowerCase();
  const filtered = scenes.filter(scene => [scene.id, scene.title, scene.world, scene.appearance, scene.source_collection]
    .some(value => String(value || '').toLocaleLowerCase().includes(query)));
  list.replaceChildren();
  sceneCount.textContent = `${filtered.length} of ${scenes.length} catalog scene${scenes.length === 1 ? '' : 's'}`;
  if (!filtered.length) {
    const empty = document.createElement('p');
    empty.className = 'empty';
    empty.textContent = 'No matching catalog entries.';
    list.append(empty);
    return;
  }
  for (const scene of filtered) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'scene-option';
    button.setAttribute('role', 'option');
    button.setAttribute('aria-selected', String(scene.id === selectedId));
    button.dataset.sceneId = scene.id;
    const name = document.createElement('strong');
    name.textContent = scene.title || scene.id;
    const meta = document.createElement('small');
    meta.textContent = `${scene.id} · ${scene.world} / ${scene.appearance}`;
    button.append(name, meta);
    button.addEventListener('click', () => showScene(scene));
    list.append(button);
  }
}

function renderRules(rules) {
  if (!rules) {
    rulesSummary.textContent = 'No sidecar found at the default path. This view does not create or edit a rules file.';
    return;
  }
  const lines = [`Read-only summary: ${rules.path}`, `Pools: ${rules.pools.length} · scene rules: ${rules.rule_count}`];
  for (const pool of rules.pools) {
    lines.push(`• ${pool.id} (${pool.source}) — ${pool.enabled ? 'enabled' : 'disabled'}, ${pool.scene_count} scene entries`);
  }
  lines.push('Rules are summarized only; this UI does not evaluate eligibility.');
  rulesSummary.textContent = lines.join('\n');
}

search.addEventListener('input', renderList);

(async function start() {
  try {
    if ('ResizeObserver' in window) {
      resizeObserver = new ResizeObserver(resizeCanvas);
      resizeObserver.observe(canvas);
    } else {
      window.addEventListener('resize', resizeCanvas);
    }
    const response = await fetch('catalog', { cache: 'no-store' });
    if (!response.ok) throw new Error(`Catalog request failed (${response.status}).`);
    const data = await response.json();
    scenes = Array.isArray(data.scenes) ? data.scenes : [];
    renderRules(data.rules);
    renderList();
    if (scenes.length) showScene(scenes[0]);
    else setStatus('The local catalog contains no scenes.', true);
    setStatus('Read-only preview: Worldwall is not contacted or controlled.');
  } catch (error) {
    sceneCount.textContent = 'Catalog could not be loaded.';
    list.replaceChildren();
    const message = document.createElement('p');
    message.className = 'empty';
    message.textContent = error.message || String(error);
    list.append(message);
    setStatus(error.message || String(error), true);
  }
})();
