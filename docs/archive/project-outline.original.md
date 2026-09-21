# Worldwall

A lightweight, GPU-accelerated, weather- and time-aware pixel-art wallpaper engine for Hyprland/Wayland.

Worldwall is inspired by the palette-based animation techniques used by [Effect Games Worlds](https://www.effectgames.com/demos/worlds/).

Instead of generating animated images on the CPU or playing video wallpapers, Worldwall keeps an indexed scene and its animation metadata resident on the GPU. A fragment shader converts palette indices into final colors and continuously animates selected palette entries.

The intended result is a living pixel-art wallpaper with low CPU overhead, minimal network activity, and no continuously regenerated image data.

---

# 1. Architectural Decisions

The following decisions are normative for the implementation.

## 1.1 Wayland surface ownership

`smithay-client-toolkit` owns:

* Wayland connection
* compositor
* outputs
* layer-shell global
* `wl_surface`
* layer-shell configuration
* Wayland event dispatch

The layer surface is created at the `Background` layer.

## 1.2 GPU presentation

`wgpu` owns:

* GPU device
* GPU queue
* render pipeline
* swapchain/presentation surface
* scene textures
* palette textures
* shader
* frame rendering

The wgpu presentation surface is created from the same Wayland display and `wl_surface` used by the layer-shell surface.

No intermediate X11 window, EGL window, GTK widget, or hidden window is required.

For the initial implementation, Vulkan is the primary backend.

## 1.3 Scene representation

Scenes use an 8-bit palette index per pixel:

```text
scene texture
    R8Uint
       │
       ▼
palette[ index ]
       │
       ▼
final RGB
```

The scene texture is never recolored on the CPU during normal operation.

## 1.4 Animation

Animation is implemented in the fragment shader.

The CPU updates only a small uniform block containing values such as:

```text
elapsed time
animation phase
brightness
output dimensions
scene dimensions
```

The CPU does not manipulate individual pixels.

## 1.5 Scene selection

Scene selection happens only when the environment is explicitly refreshed:

* Initial startup/login
* Resume from sleep
* Explicit user command

A weather update does **not** automatically change the current scene.

This preserves the original requirement that the selected image remains stable until login/resume.

## 1.6 Weather

Weather is polled approximately every 15 minutes and stored in memory/cache.

Weather is used for the **next scene selection**, not for continuous rendering.

Network failure falls back to the last known weather.

## 1.7 Animation timing

`time` in the shader is strictly an animation clock.

It is not used to decide whether the scene should become "day", "night", "rain", etc.

Those decisions are made by the scene selector when it runs.

---

# 2. Final Runtime Architecture

```text
                           ┌──────────────────────┐
                           │   Power/session      │
                           │   resume events      │
                           └──────────┬───────────┘
                                      │
                                      ▼
┌─────────────────┐          ┌──────────────────┐
│ 15-minute timer │─────────►│ Environment      │
└─────────────────┘          │ Manager          │
                             │                  │
                             │ Clock            │
                             │ Astronomy        │
                             │ Weather cache    │
                             └────────┬─────────┘
                                      │
                         startup/resume/manual only
                                      │
                                      ▼
                             ┌─────────────────┐
                             │ Scene Selector  │
                             └────────┬────────┘
                                      │
                                      ▼
                             ┌─────────────────┐
                             │ Selected Scene  │
                             └────────┬────────┘
                                      │
                                      ▼
                    ┌─────────────────────────────────┐
                    │          Shared GPU State        │
                    │                                 │
                    │ R8Uint scene texture            │
                    │ 256-entry palette texture       │
                    │ Animation metadata texture      │
                    │ Render pipeline                 │
                    └──────────────┬──────────────────┘
                                   │
                    shared across all outputs
                                   │
                ┌──────────────────┴──────────────────┐
                │                                     │
                ▼                                     ▼
        ┌───────────────┐                     ┌───────────────┐
        │ Output 1      │                     │ Output 2      │
        │               │                     │               │
        │ wl_surface    │                     │ wl_surface    │
        │ layer-shell   │                     │ layer-shell   │
        │ wgpu Surface  │                     │ wgpu Surface  │
        └───────┬───────┘                     └───────┬───────┘
                │                                     │
                ▼                                     ▼
             Hyprland                              Hyprland
```

---

# 3. Project Structure

The final repository should use the following structure:

```text
worldwall/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── PLAN.md
├── LICENSE
│
├── assets/
│   ├── scenes/
│   │   ├── village/
│   │   │   ├── scene.png
│   │   │   ├── palette.png
│   │   │   ├── animation.png
│   │   │   └── scene.toml
│   │   │
│   │   ├── forest/
│   │   └── coast/
│   │
│   └── shaders/
│       └── wallpaper.wgsl
│
├── config/
│   └── worldwall.toml
│
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── config.rs
│   │
│   ├── environment/
│   │   ├── mod.rs
│   │   ├── clock.rs
│   │   ├── astronomy.rs
│   │   └── weather.rs
│   │
│   ├── scene/
│   │   ├── mod.rs
│   │   ├── manifest.rs
│   │   ├── selector.rs
│   │   └── loader.rs
│   │
│   ├── renderer/
│   │   ├── mod.rs
│   │   ├── gpu.rs
│   │   ├── pipeline.rs
│   │   ├── texture.rs
│   │   └── output.rs
│   │
│   ├── wayland/
│   │   ├── mod.rs
│   │   ├── connection.rs
│   │   ├── layer.rs
│   │   └── events.rs
│   │
│   └── ipc/
│       ├── mod.rs
│       └── socket.rs
│
├── system/
│   └── worldwall.service
│
└── tools/
    └── convert_scene/
```

The initial prototype can remain much smaller and expand into this structure later.

---

# 4. Main Runtime Components

## 4.1 `App`

`App` owns high-level state and coordinates subsystems.

```rust
pub struct App {
    environment: EnvironmentManager,
    scenes: SceneRegistry,
    renderer: Renderer,
}
```

`App` is responsible for:

* Startup
* Scene selection
* Resume handling
* Configuration reload
* Manual commands

It does not directly perform low-level Wayland rendering.

---

# 5. Environment System

```rust
pub struct Environment {
    pub local_time: DateTime<Local>,
    pub sunrise: DateTime<Local>,
    pub sunset: DateTime<Local>,
    pub season: Season,
    pub weather: Weather,
}
```

Weather:

```rust
pub struct Weather {
    pub condition: WeatherCondition,
    pub temperature_c: f32,
    pub precipitation: bool,
    pub timestamp: DateTime<Local>,
}
```

The environment system has two different update frequencies.

### Fast state

Animation time is updated continuously for rendering.

### Slow state

Environmental information is refreshed only when needed:

```text
startup
resume
15-minute weather timer
manual reload
```

The weather timer updates the weather cache.

It does not trigger scene selection.

---

# 6. Scene Selection

Scene selection is a discrete operation:

```rust
pub fn select_scene(
    scenes: &[Scene],
    environment: &Environment,
) -> &Scene
```

A weighted score can initially be used:

```text
time_match       × 5
weather_match    × 4
season_match     × 2
random_factor    × 1
```

The chosen scene remains active until the next explicit selection event.

---

# 7. Scene Format

A scene consists of:

```rust
pub struct Scene {
    pub id: String,
    pub image: PathBuf,
    pub palette: PathBuf,
    pub animation: PathBuf,
    pub tags: SceneTags,
}
```

Scene metadata:

```toml
id = "village"

image = "scene.png"
palette = "palette.png"
animation = "animation.png"

times = [
    "dawn",
    "day",
    "dusk",
    "night"
]

weather = [
    "clear",
    "cloudy"
]

seasons = [
    "spring",
    "summer",
    "autumn",
    "winter"
]
```

---

# 8. GPU Resource Model

Each selected scene produces three primary GPU textures.

## 8.1 Scene texture

Format:

```text
R8Uint
```

Each texel contains a palette index from `0` to `255`.

Example:

```text
12 12 12 13 13 13
12 12 14 14 13 13
15 15 14 14 13 13
```

No RGB conversion occurs on the CPU after the scene has been uploaded.

## 8.2 Palette texture

Format:

```text
RGBA8Unorm
```

Dimensions:

```text
256 × 1
```

Entry `N` contains the RGB color for palette index `N`.

## 8.3 Animation metadata

Format:

```text
RGBA16Float
```

Dimensions:

```text
256 × 1
```

Each entry contains shader parameters for that palette index.

Conceptually:

```text
R = animation speed
G = amplitude
B = phase offset
A = animation/group flag
```

These textures are static for the lifetime of the selected scene.

They are uploaded again only when the scene changes.

---

# 9. Renderer Ownership

The renderer owns one shared GPU context:

```rust
pub struct Renderer {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,

    pipeline: wgpu::RenderPipeline,

    scene_texture: wgpu::Texture,
    palette_texture: wgpu::Texture,
    animation_texture: wgpu::Texture,
}
```

Each monitor gets a separate output object:

```rust
pub struct OutputRenderer {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub width: u32,
    pub height: u32,
}
```

All outputs share:

```text
Device
Queue
Pipeline
Scene texture
Palette texture
Animation texture
```

Each output owns:

```text
Wayland surface
wgpu surface
surface configuration
current size
```

This avoids loading the artwork multiple times into GPU memory.

---

# 10. Wayland Ownership

The Wayland subsystem owns:

```text
Connection
EventQueue
Compositor
LayerShell
Outputs
wl_surface
LayerSurface
```

For each monitor:

```text
wl_output
   │
   ▼
wl_surface
   │
   ▼
layer-shell surface
   │
   ▼
Background layer
```

The layer surface is configured using:

```text
Layer::Background
Anchor::TOP
Anchor::BOTTOM
Anchor::LEFT
Anchor::RIGHT
KeyboardInteractivity::None
exclusive zone = -1
```

The surface is therefore a full-output background layer.

---

# 11. wgpu + Wayland Boundary

The critical integration is:

```text
Smithay Client Toolkit
        │
        │ owns
        ▼
    wl_surface
        │
        │ raw Wayland handle
        ▼
      wgpu
        │
        ▼
   Vulkan surface
        │
        ▼
    swapchain
```

The Wayland `wl_surface` must remain alive for the entire lifetime of the corresponding wgpu `Surface`.

Creating the wgpu surface is the only intentionally unsafe FFI boundary in this part of the renderer.

The implementation should use raw Wayland display/surface handles rather than introducing a second windowing abstraction.

---

# 12. Layer-Shell Configure Lifecycle

A layer-shell surface cannot simply be rendered immediately after creation.

The lifecycle is:

```text
create wl_surface
       │
       ▼
create layer-shell role
       │
       ▼
set layer/anchors/etc.
       │
       ▼
commit initial state
       │
       ▼
receive configure
       │
       ▼
ack configure
       │
       ▼
obtain final output dimensions
       │
       ▼
configure wgpu surface
       │
       ▼
render
```

The first frame must therefore wait until the compositor has supplied the surface configuration.

Resizes follow the same process.

---

# 13. wgpu Surface Configuration

Each output configures its own wgpu surface:

```rust
wgpu::SurfaceConfiguration {
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    format,
    width,
    height,
    present_mode: wgpu::PresentMode::Fifo,
    alpha_mode,
    desired_maximum_frame_latency: 2,
    view_formats: vec![],
}
```

`Fifo` is the initial choice because it is guaranteed to be supported and corresponds to normal vsync behavior.

No assumption should be made that a particular monitor refresh rate equals the configured wallpaper animation rate.

---

# 14. Render Loop

The important correction to the previous architecture is:

> GPU-side animation does not eliminate CPU frame submission.

Every visible animation frame ultimately requires:

```text
CPU
 │
 ├── update time uniform
 ├── encode render commands
 ├── submit command buffer
 └── present surface
        │
        ▼
      GPU
```

The expensive operation — transforming every wallpaper pixel — remains entirely on the GPU.

The CPU cost is therefore expected to be low, but not literally zero.

---

# 15. Frame Pacing

The renderer should not use a busy loop.

Bad:

```rust
loop {
    render();
}
```

Also bad:

```rust
loop {
    render();
    thread::sleep(Duration::from_millis(1));
}
```

The render loop should wait for the next intended frame.

Initial implementation:

```text
target 15 FPS
      or
target 30 FPS
```

The default should be 30 FPS for desktop systems.

A later low-power mode can use 15 FPS.

The render scheduler should avoid generating frames faster than the configured target.

---

# 16. Wayland Frame Callbacks

Wayland's `wl_surface.frame` callback should be incorporated as a compositor pacing signal.

Conceptually:

```text
render frame
     │
     ▼
commit surface
     │
     ▼
request frame callback
     │
     ▼
compositor says "draw again"
     │
     ▼
check target FPS
     │
     ├── too early → wait
     │
     └── ready → render
```

This prevents unnecessary rendering when the surface is not currently useful to the compositor.

The timer/frame scheduler should still enforce the configured 15/30 FPS ceiling.

---

# 17. Shader Architecture

The shader should use a fullscreen triangle.

No vertex buffer is required.

```text
Fullscreen triangle
        │
        ▼
   fragment shader
        │
        ├── calculate source UV
        ├── obtain source pixel coordinate
        ├── textureLoad(scene)
        ├── textureLoad(palette)
        ├── textureLoad(animation)
        ├── animate color
        └── write output
```

---

# 18. Indexed Texture Lookup

The scene texture should not be sampled as a normalized color texture.

Instead:

```wgsl
let index =
    textureLoad(
        scene_texture,
        scene_coord,
        0
    ).r;
```

The resulting value is an integer palette index.

Then:

```wgsl
let palette =
    textureLoad(
        palette_texture,
        vec2<i32>(index, 0),
        0
    );
```

The final color is obtained directly from the palette.

This avoids:

```text
normalized 0–1 conversion
rounding
sampler filtering
palette-index precision issues
```

and gives exact 8-bit palette indexing.

---

# 19. Animation Shader

Conceptually:

```wgsl
let index =
    textureLoad(scene_texture, scene_coord, 0).r;

let base_color =
    textureLoad(
        palette_texture,
        vec2<i32>(index, 0),
        0
    );

let animation =
    textureLoad(
        animation_texture,
        vec2<i32>(index, 0),
        0
    );

let phase =
    globals.time * animation.r +
    animation.b;

let animated =
    animate_color(
        base_color,
        phase,
        animation.g,
        animation.a
    );

return animated;
```

The shader never needs to know:

* which scene is selected
* what weather is currently occurring
* what season it is
* where the computer is located

Those are application-level concerns.

---

# 20. Animation Types

The first shader should support a small number of effects.

### Hue cycling

Used for:

* Water
* Magic
* Fire
* Neon

### Brightness pulsing

Used for:

* Windows
* Lamps
* Stars
* Fire

### Palette interpolation

Used for:

* Water movement
* Sky transitions
* Atmospheric effects

### Group-level animation

A group of palette entries can share:

```text
speed
amplitude
phase
effect
```

This keeps neighboring colors visually coherent.

---

# 21. Scene Scaling

Pixel art should use nearest-neighbor behavior.

The shader should derive a source pixel coordinate rather than relying on linear interpolation.

Conceptually:

```text
output UV
   │
   ▼
aspect-ratio calculation
   │
   ▼
source pixel coordinate
   │
   ▼
textureLoad()
```

The initial scaling mode should be:

```text
cover
+ nearest-neighbor
```

Future modes can include:

```text
contain
integer scale
stretch
```

---

# 22. Multi-Monitor Rendering

Multi-monitor support does not mean multiple GPU contexts.

Correct architecture:

```text
                 Renderer
                    │
        ┌───────────┴───────────┐
        │                       │
   Shared GPU State        Shared GPU State
        │                       │
        ▼                       ▼
    Output 1                Output 2
    Surface                 Surface
    Swapchain               Swapchain
```

For each output:

```rust
renderer.render_output(
    &mut output,
    animation_time,
);
```

The same scene texture can be rendered into multiple output surfaces.

---

# 23. Output Lifecycle

When a monitor appears:

```text
wl_output appears
      │
      ▼
create wl_surface
      │
      ▼
create layer surface
      │
      ▼
receive configure
      │
      ▼
create wgpu Surface
      │
      ▼
configure swapchain
      │
      ▼
render
```

When a monitor disappears:

```text
output removed
     │
     ▼
destroy output renderer
     │
     ▼
release surface
```

The shared renderer remains alive.

---

# 24. Resume Handling

Sleep/wake is not a rendering event.

It should be handled by a power/session event source.

The application should conceptually receive:

```text
PrepareForSleep(true)
    ↓
system entering sleep

PrepareForSleep(false)
    ↓
system resumed
```

On resume:

```text
refresh clock
refresh astronomy
refresh weather if stale
select new scene
reload scene textures
resume render loop
```

This avoids treating every frame or Wayland event as a possible scene change.

---

# 25. Weather Polling

The weather service runs independently of the renderer.

```text
Every 15 minutes:

weather provider
       │
       ▼
WeatherService
       │
       ▼
cached Weather
```

The cache should contain:

```json
{
    "temperature_c": 17.4,
    "condition": "cloudy",
    "precipitation": 0.0,
    "timestamp": "..."
}
```

If the request fails:

```text
API failure
    │
    ▼
retain previous value
```

Rendering must continue normally.

---

# 26. Weather Does Not Trigger Scene Changes

This is an intentional design decision.

Suppose:

```text
17:00 login
weather = cloudy
scene = village
```

At:

```text
17:15
weather = rain
```

The scene remains:

```text
village
```

The new weather is simply cached.

At the next:

```text
resume
```

the selector sees:

```text
weather = rain
```

and can choose a rain-compatible scene.

This prevents unexpected wallpaper changes every 15 minutes.

An explicit command can still force reselection:

```bash
worldwallctl reload
```

---

# 27. System Lifecycle

The normal lifecycle is:

```text
Hyprland starts
      │
      ▼
Worldwall starts
      │
      ├── connect Wayland
      ├── discover outputs
      ├── load environment
      ├── select scene
      ├── upload GPU resources
      └── render


Sleep
      │
      ▼
rendering pauses


Resume
      │
      ├── refresh environment
      ├── refresh weather if stale
      ├── select scene
      ├── replace GPU textures
      └── resume rendering
```

---

# 28. Hyprland Integration

Initial integration should be intentionally simple:

```ini
exec-once = worldwall
```

Worldwall itself should remain a long-lived foreground process.

A systemd user service can be added later for restart policy and logging, but it should not be a prerequisite for the rendering architecture.

If a systemd user service is introduced, the Wayland session environment must be available to the service.

---

# 29. IPC

A Unix socket can provide runtime control:

```text
/run/user/$UID/worldwall.sock
```

Commands:

```bash
worldwallctl status
worldwallctl reload
worldwallctl scene village
worldwallctl next
worldwallctl weather
```

Semantics:

### `status`

Returns:

* Outputs
* Current scene
* Current weather
* FPS target
* Renderer backend

### `reload`

Refreshes environment and selects a new scene.

### `scene <id>`

Immediately selects a specific scene.

### `next`

Selects another compatible scene.

### `weather`

Returns cached weather state.

---

# 30. Performance Model

The intended steady-state workload is:

```text
CPU

    Low-frequency:
        weather
        timers
        Wayland events
        scene management

    Per frame:
        update tiny uniform buffer
        encode render pass
        submit/present


GPU

    Per frame:
        fullscreen triangle
        indexed texture lookup
        palette lookup
        animation math
        write output
```

The large per-pixel workload is GPU-side.

The CPU should therefore not be described as "idle" during animation. The correct objective is:

> Minimize CPU work to frame scheduling and GPU command submission.

---

# 31. Power Strategy

The rendering system should expose a configurable target:

```toml
[render]
fps = 30
```

Optional low-power mode:

```toml
[render]
fps = 15
```

The renderer should eventually measure:

* CPU utilization
* GPU utilization
* GPU power draw where available
* frame latency
* missed presentation deadlines

The objective is not merely high FPS.

The objective is:

```text
minimum resource cost
while preserving the intended visual effect
```

---

# 32. Development Phases

## Phase 1 — Wayland Surface

Implement:

```text
Wayland connection
    ↓
wl_surface
    ↓
layer-shell Background
    ↓
configure/ack lifecycle
```

Success criterion:

> A correctly sized, input-free background layer appears on Hyprland.

---

## Phase 2 — wgpu Presentation

Implement:

```text
Wayland wl_surface
       ↓
raw Wayland handles
       ↓
wgpu Surface
       ↓
Vulkan
       ↓
swapchain
```

Success criterion:

> A static GPU-rendered fullscreen triangle or solid color appears on the background layer.

---

## Phase 3 — Static Scene Texture

Implement:

```text
R8Uint scene texture
       ↓
nearest pixel lookup
       ↓
palette
```

Success criterion:

> An indexed pixel-art image renders correctly.

---

## Phase 4 — Palette Animation

Implement:

```text
palette texture
+
animation metadata
+
time uniform
+
WGSL animation
```

Success criterion:

> Selected colors continuously animate without CPU-side image processing.

---

## Phase 5 — Scene Registry

Implement:

```text
scene.toml
    ↓
scene loader
    ↓
scene selector
    ↓
renderer scene replacement
```

Success criterion:

> Multiple scenes can be selected without modifying renderer code.

---

## Phase 6 — Time and Astronomy

Implement:

* Local time
* Season
* Sunrise
* Sunset

Success criterion:

> The selector can choose scenes appropriate to the current date/time.

---

## Phase 7 — Weather

Implement:

* API client
* 15-minute cache
* Last-known fallback
* Weather-aware scene scoring

Success criterion:

> Weather affects future scene selection without interrupting rendering.

---

## Phase 8 — Resume

Implement:

* Sleep detection
* Resume detection
* Environment refresh
* Scene reselection

Success criterion:

> Waking the machine causes Worldwall to reevaluate the environment and select a new scene.

---

## Phase 9 — Multi-Monitor

Implement:

* Per-output layer surfaces
* Per-output wgpu surfaces
* Shared GPU resources

Success criterion:

> Multiple monitors render the same scene without duplicating scene textures.

---

## Phase 10 — IPC and Polish

Implement:

* `worldwallctl`
* Unix socket
* Logging
* Configuration reload
* Performance metrics
* Error recovery

---

# 33. Initial MVP

The first implementation should contain only:

```text
src/
├── main.rs
├── wayland.rs
└── renderer.rs

assets/
├── test.png
└── shaders/
    └── wallpaper.wgsl
```

MVP success criteria:

1. Connect to Hyprland's Wayland compositor.
2. Create a background layer surface.
3. Receive and acknowledge the layer configure event.
4. Create a wgpu surface from the Wayland surface.
5. Configure the Vulkan swapchain.
6. Render a static GPU image.
7. Present it correctly.
8. Render above it with normal Hyprland applications.
9. Handle output resize.
10. Exit cleanly when the compositor closes the layer surface.

No weather system should be implemented until this works.

---

# 34. Final Architecture

The complete system is therefore:

```text
                   ENVIRONMENT
                       │
        ┌──────────────┼──────────────┐
        │              │              │
      Clock        Astronomy       Weather
        │              │              │
        └──────────────┼──────────────┘
                       │
                 startup/resume
                       │
                       ▼
                 SCENE SELECTOR
                       │
                       ▼
                 SELECTED SCENE
                       │
            ┌──────────┼──────────┐
            │          │          │
         indices     palette    animation
            │          │          │
            └──────────┼──────────┘
                       │
                       ▼
                  GPU RESOURCES
                       │
                       ▼
                 WGSL FRAGMENT
                    SHADER
                       │
        ┌──────────────┴──────────────┐
        │                             │
        ▼                             ▼
   Output 1 Surface              Output 2 Surface
        │                             │
        ▼                             ▼
   Wayland layer-shell           Wayland layer-shell
        │                             │
        └──────────────┬──────────────┘
                       │
                       ▼
                    Hyprland
```

The core invariant is:

> **Environment determines the scene. The scene determines GPU resources. The shader determines animation. Wayland determines presentation.**

Those concerns should not bleed into one another.
