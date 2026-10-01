//! Exploratory low-resolution wl_shm renderer benchmark.
//!
//! This is an isolated XDG toplevel, not the Worldwall wallpaper process. It
//! renders a synthetic indexed/palette-like scene into a 640x360 XRGB8888
//! shared-memory buffer and asks wp_viewporter to scale it to the window size.
//! No Vulkan or wgpu objects are created.

use std::{
    error::Error,
    time::{Duration, Instant},
};

use calloop::{
    EventLoop,
    timer::{TimeoutAction, Timer},
};
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState, SimpleGlobal},
    registry_handlers,
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
    shm::{
        Shm, ShmHandler,
        slot::{Buffer, SlotPool},
    },
};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

const BUFFER_WIDTH: u32 = 640;
const BUFFER_HEIGHT: u32 = 360;
const TARGET_FPS: u64 = 30;
const MAX_BUFFERS: usize = 3;
// One quarter of the pixels animate; the rest are deliberately static.
const ANIMATED_RECT: (u32, u32, u32, u32) = (160, 90, 320, 180);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderMode {
    Full,
    Dirty,
}

impl RenderMode {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value {
            "full" => Ok(Self::Full),
            "dirty" => Ok(Self::Dirty),
            _ => Err(format!("unknown --mode {value:?}; expected full or dirty").into()),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Stats {
    frames: u64,
    skipped_no_buffer: u64,
    bytes_written: u64,
    damage_pixels: u64,
    render_time: Duration,
}

struct ShmBuffer {
    buffer: Buffer,
    initialized: bool,
}

struct Demo {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    window: Window,
    viewport: WpViewport,
    pool: SlotPool,
    buffers: Vec<ShmBuffer>,
    mode: RenderMode,
    warmup: Duration,
    duration: Duration,
    configured: bool,
    redraw_pending: bool,
    frame_ready: bool,
    tick_due: bool,
    exit_requested: bool,
    started: Option<Instant>,
    stats: Stats,
    frame_number: u64,
    logical_size: (u32, u32),
    indices: Vec<u8>,
    base_palette: [[u8; 3]; 256],
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut mode = RenderMode::Dirty;
    let mut warmup_seconds = 3_u64;
    let mut measured_seconds = 30_u64;
    let mut fps = TARGET_FPS;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => mode = RenderMode::parse(&args.next().ok_or("--mode needs a value")?)?,
            "--warmup" => warmup_seconds = args.next().ok_or("--warmup needs seconds")?.parse()?,
            "--seconds" => {
                measured_seconds = args.next().ok_or("--seconds needs a value")?.parse()?
            }
            "--fps" => fps = args.next().ok_or("--fps needs a value")?.parse()?,
            "--help" | "-h" => {
                println!(
                    "wlshm_lowres_bench [--mode full|dirty] [--fps N] [--warmup seconds] [--seconds seconds]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if fps == 0 || fps > 120 || measured_seconds == 0 {
        return Err("fps must be 1..=120 and measured seconds must be nonzero".into());
    }

    let (indices, base_palette) = synthetic_indexed_scene();
    let connection = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init(&connection)?;
    let qh = event_queue.handle();
    let compositor = CompositorState::bind(&globals, &qh)?;
    let xdg_shell = XdgShell::bind(&globals, &qh)?;
    let shm = Shm::bind(&globals, &qh)?;
    let viewporter = SimpleGlobal::<WpViewporter, 1>::bind(&globals, &qh)?;
    let viewporter = viewporter.get()?;

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::ServerDefault, &qh);
    window.set_title("Worldwall low-resolution wl_shm benchmark");
    window.set_app_id("org.worldwall.WlShmLowResBench");
    window.set_min_size(Some((960, 540)));
    window.set_maximized();
    let viewport = viewporter.get_viewport(window.wl_surface(), &qh, ());
    window.commit();

    let initial_pool_bytes = (BUFFER_WIDTH * BUFFER_HEIGHT * 4 * 2) as usize;
    let pool = SlotPool::new(initial_pool_bytes, &shm)?;
    let interval = Duration::from_nanos(1_000_000_000 / fps);
    let mut event_loop: EventLoop<Demo> = EventLoop::try_new()?;
    event_loop
        .handle()
        .insert_source(Timer::from_duration(interval), move |_, _, demo| {
            demo.tick_due = true;
            TimeoutAction::ToDuration(interval)
        })?;
    WaylandSource::new(connection, event_queue).insert(event_loop.handle())?;

    let mut demo = Demo {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        window,
        viewport,
        pool,
        buffers: Vec::new(),
        mode,
        warmup: Duration::from_secs(warmup_seconds),
        duration: Duration::from_secs(measured_seconds),
        configured: false,
        redraw_pending: false,
        frame_ready: true,
        tick_due: false,
        exit_requested: false,
        started: None,
        stats: Stats::default(),
        frame_number: 0,
        logical_size: (960, 540),
        indices,
        base_palette,
    };

    println!(
        "wlshm-bench starting mode={mode:?} buffer={}x{} fps={fps} warmup={}s measured={}s; close the demo window to stop",
        BUFFER_WIDTH, BUFFER_HEIGHT, warmup_seconds, measured_seconds
    );
    loop {
        event_loop.dispatch(None, &mut demo)?;
        if demo.exit_requested {
            break;
        }
        if demo.configured && (demo.redraw_pending || (demo.tick_due && demo.frame_ready)) {
            demo.draw(&qh)?;
        }
        if demo
            .started
            .is_some_and(|started| started.elapsed() >= demo.warmup + demo.duration)
        {
            demo.exit_requested = true;
        }
    }

    let measured = demo.stats;
    let elapsed = demo.started.map_or(Duration::ZERO, |started| {
        started
            .elapsed()
            .saturating_sub(demo.warmup)
            .min(demo.duration)
    });
    let measured_seconds = elapsed.as_secs_f64();
    let average_fps = if measured_seconds > 0.0 {
        measured.frames as f64 / measured_seconds
    } else {
        0.0
    };
    let average_render_us = if measured.frames > 0 {
        measured.render_time.as_secs_f64() * 1_000_000.0 / measured.frames as f64
    } else {
        0.0
    };
    let write_rate_mib_s = if measured_seconds > 0.0 {
        measured.bytes_written as f64 / measured_seconds / (1024.0 * 1024.0)
    } else {
        0.0
    };
    println!(
        "wlshm-bench result mode={mode:?} buffer={}x{} logical={}x{} measured={measured_seconds:.2}s frames={} avg_fps={average_fps:.2} avg_draw_us={average_render_us:.1} bytes_written={} write_MiB_s={write_rate_mib_s:.2} damage_pixels={} skipped_no_buffer={}",
        BUFFER_WIDTH,
        BUFFER_HEIGHT,
        demo.logical_size.0,
        demo.logical_size.1,
        measured.frames,
        measured.bytes_written,
        measured.damage_pixels,
        measured.skipped_no_buffer,
    );
    Ok(())
}

impl Demo {
    fn draw(&mut self, qh: &QueueHandle<Self>) -> Result<(), Box<dyn Error>> {
        self.redraw_pending = false;
        self.tick_due = false;
        if !self.frame_ready {
            return Ok(());
        }

        let existing = (0..self.buffers.len())
            .find(|&index| self.pool.canvas(&self.buffers[index].buffer).is_some());
        let index = if let Some(index) = existing {
            index
        } else if self.buffers.len() < MAX_BUFFERS {
            let stride = (BUFFER_WIDTH * 4) as i32;
            let (buffer, _) = self.pool.create_buffer(
                BUFFER_WIDTH as i32,
                BUFFER_HEIGHT as i32,
                stride,
                wl_shm::Format::Xrgb8888,
            )?;
            self.buffers.push(ShmBuffer {
                buffer,
                initialized: false,
            });
            self.buffers.len() - 1
        } else {
            self.stats.skipped_no_buffer = self.stats.skipped_no_buffer.saturating_add(1);
            return Ok(());
        };

        let was_initialized = self.buffers[index].initialized;
        let full_redraw = self.mode == RenderMode::Full || !was_initialized;
        let rect = if full_redraw {
            (0, 0, BUFFER_WIDTH, BUFFER_HEIGHT)
        } else {
            ANIMATED_RECT
        };
        let palette = animated_palette(&self.base_palette, self.frame_number);
        let indices = &self.indices;
        let render_start = Instant::now();
        let buffer = &self.buffers[index].buffer;
        let canvas = self
            .pool
            .canvas(buffer)
            .ok_or("selected wl_shm buffer is busy")?;
        render_region(canvas, rect, indices, &palette);
        let render_time = render_start.elapsed();
        self.buffers[index].initialized = true;

        let pixels = u64::from(rect.2) * u64::from(rect.3);
        let bytes = pixels * 4;
        let started = *self.started.get_or_insert_with(Instant::now);
        if started.elapsed() >= self.warmup {
            self.stats.frames = self.stats.frames.saturating_add(1);
            self.stats.bytes_written = self.stats.bytes_written.saturating_add(bytes);
            self.stats.damage_pixels = self.stats.damage_pixels.saturating_add(pixels);
            self.stats.render_time = self.stats.render_time.saturating_add(render_time);
        }

        let surface = self.window.wl_surface();
        surface.damage_buffer(rect.0 as i32, rect.1 as i32, rect.2 as i32, rect.3 as i32);
        surface.frame(qh, FrameCallbackData(surface.clone()));
        self.buffers[index].buffer.attach_to(surface)?;
        self.window.commit();
        self.frame_ready = false;
        self.frame_number = self.frame_number.wrapping_add(1);
        Ok(())
    }
}

fn synthetic_indexed_scene() -> (Vec<u8>, [[u8; 3]; 256]) {
    // Model a 640x480 indexed source with a centered 4:3-to-16:9 cover crop.
    // The displayed 640x360 buffer samples source rows 60..420 one-to-one.
    let mut source = vec![0_u8; 640 * 480];
    for y in 0..480_u32 {
        for x in 0..640_u32 {
            let output_y = y.saturating_sub(60);
            let in_visible_crop = (60..420).contains(&y);
            let in_animated_region = in_visible_crop
                && (ANIMATED_RECT.0..ANIMATED_RECT.0 + ANIMATED_RECT.2).contains(&x)
                && (ANIMATED_RECT.1..ANIMATED_RECT.1 + ANIMATED_RECT.3).contains(&output_y);
            let index = if in_animated_region {
                64 + ((x / 10 + output_y / 8) % 32) as u8
            } else {
                ((x / 16 + y / 12) % 64) as u8
            };
            source[(y * 640 + x) as usize] = index;
        }
    }

    let mut output_indices = vec![0_u8; (BUFFER_WIDTH * BUFFER_HEIGHT) as usize];
    for y in 0..BUFFER_HEIGHT {
        let source_y = y + 60;
        let source_start = (source_y * 640) as usize;
        let output_start = (y * BUFFER_WIDTH) as usize;
        output_indices[output_start..output_start + BUFFER_WIDTH as usize]
            .copy_from_slice(&source[source_start..source_start + BUFFER_WIDTH as usize]);
    }

    let mut palette = [[0_u8; 3]; 256];
    for (index, color) in palette.iter_mut().enumerate() {
        let index = index as u32;
        *color = [
            ((index * 37 + 29) % 256) as u8,
            ((index * 73 + 61) % 256) as u8,
            ((index * 109 + 103) % 256) as u8,
        ];
    }
    (output_indices, palette)
}

fn animated_palette(base: &[[u8; 3]; 256], frame: u64) -> [[u8; 3]; 256] {
    let mut palette = *base;
    let rotation = ((frame / 3) % 32) as usize;
    for index in 0..32 {
        palette[64 + index] = base[64 + (index + rotation) % 32];
    }
    palette
}

fn render_region(
    canvas: &mut [u8],
    rect: (u32, u32, u32, u32),
    indices: &[u8],
    palette: &[[u8; 3]; 256],
) {
    let (x0, y0, width, height) = rect;
    for y in y0..y0 + height {
        for x in x0..x0 + width {
            let pixel = (y * BUFFER_WIDTH + x) as usize;
            let color = palette[indices[pixel] as usize];
            let offset = pixel * 4;
            // XRGB8888 is 0xXXRRGGBB; this host's little-endian SHM byte order
            // is B, G, R, X. X is set to 0xff for predictable opaque content.
            canvas[offset] = color[2];
            canvas[offset + 1] = color[1];
            canvas[offset + 2] = color[0];
            canvas[offset + 3] = 0xff;
        }
    }
}

fn cover_source(
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
) -> (f64, f64, f64, f64) {
    let source_aspect = f64::from(source_width) / f64::from(source_height);
    let destination_aspect = f64::from(destination_width) / f64::from(destination_height);
    if source_aspect > destination_aspect {
        let visible_width = f64::from(source_height) * destination_aspect;
        (
            (f64::from(source_width) - visible_width) / 2.0,
            0.0,
            visible_width,
            f64::from(source_height),
        )
    } else {
        let visible_height = f64::from(source_width) / destination_aspect;
        (
            0.0,
            (f64::from(source_height) - visible_height) / 2.0,
            f64::from(source_width),
            visible_height,
        )
    }
}

impl Drop for Demo {
    fn drop(&mut self) {
        self.viewport.destroy();
    }
}

impl CompositorHandler for Demo {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.frame_ready = true;
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Demo {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl WindowHandler for Demo {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &Window) {
        self.exit_requested = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let width = configure.new_size.0.map_or(960, |value| value.get());
        let height = configure.new_size.1.map_or(540, |value| value.get());
        self.logical_size = (width, height);
        self.viewport.set_destination(width as i32, height as i32);
        let source = cover_source(BUFFER_WIDTH, BUFFER_HEIGHT, width, height);
        self.viewport
            .set_source(source.0, source.1, source.2, source.3);
        self.configured = true;
        self.redraw_pending = true;
        self.frame_ready = true;
        println!(
            "wlshm-bench configured buffer={}x{} viewport={}x{}",
            BUFFER_WIDTH, BUFFER_HEIGHT, width, height
        );
    }
}

impl ShmHandler for Demo {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(Demo);

impl ProvidesRegistryState for Demo {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

wayland_client::delegate_noop!(Demo: WpViewporter);

impl Dispatch<WpViewport, ()> for Demo {
    fn event(
        _state: &mut Self,
        _proxy: &WpViewport,
        _event: wayland_protocols::wp::viewporter::client::wp_viewport::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        unreachable!("wp_viewport has no events in version 1");
    }
}

smithay_client_toolkit::delegate_dispatch2!(Demo);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_viewport_preserves_aspect_and_center_crops() {
        let exact = cover_source(640, 360, 1920, 1080);
        assert_eq!(exact, (0.0, 0.0, 640.0, 360.0));

        let wide = cover_source(640, 360, 1876, 950);
        assert_eq!(wide.0, 0.0);
        assert_eq!(wide.2, 640.0);
        assert!((wide.1 - (360.0 - wide.3) / 2.0).abs() < 1e-9);
        assert!((wide.2 / wide.3 - 1876.0 / 950.0).abs() < 1e-9);
    }

    #[test]
    fn animation_changes_only_marked_palette_entries() {
        let (_, base) = synthetic_indexed_scene();
        let animated = animated_palette(&base, 6);
        assert_eq!(&animated[..64], &base[..64]);
        assert_ne!(&animated[64..96], &base[64..96]);
        assert_eq!(&animated[96..], &base[96..]);
    }

    #[test]
    fn xrgb_shm_bytes_match_palette_lookup() {
        let mut indices = vec![0_u8; (BUFFER_WIDTH * BUFFER_HEIGHT) as usize];
        indices[0] = 17;
        let mut palette = [[0_u8; 3]; 256];
        palette[17] = [0x12, 0x34, 0x56];
        let mut canvas = vec![0_u8; (BUFFER_WIDTH * BUFFER_HEIGHT * 4) as usize];
        render_region(&mut canvas, (0, 0, 1, 1), &indices, &palette);
        assert_eq!(&canvas[..4], &[0x56, 0x34, 0x12, 0xff]);
    }
}
