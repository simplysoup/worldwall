//! Single-output Wayland layer-shell ownership and S02 presentation loop.
//!
//! SCTK owns protocol dispatch and layer configuration. Once a nonzero
//! configure is acknowledged, the retained layer surface is handed to the
//! renderer for one event-driven bootstrap frame and later resize frames.

use std::{
    error::Error,
    fmt,
    time::{Duration, Instant},
};

use calloop::{
    EventLoop,
    timer::{TimeoutAction, Timer},
};
use calloop_wayland_source::WaylandSource;

#[cfg(test)]
use std::cmp::Ordering;

use crate::{
    renderer::{RenderOutcome, Renderer},
    scene::Scene,
    scene_registry::{
        CommitOutcome, EnvironmentSnapshot, LoadedScene, RegistryError, SceneLoadToken,
        SceneManifest, SceneRegistry, SceneTransaction, SelectionController, SelectionRequest,
        SelectionTrigger,
    },
    scheduler::{Fps, FrameScheduler},
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region},
    delegate_registry,
    output::{OutputHandler, OutputInfo, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_surface},
};

/// Run the S01 layer-only client until the compositor or selected output asks
/// it to stop.
pub fn run() -> Result<(), Box<dyn Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init(&connection)?;
    let queue_handle = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &queue_handle)
        .map_err(|error| format!("required wl_compositor global is unavailable: {error}"))?;
    let layer_shell = LayerShell::bind(&globals, &queue_handle).map_err(|error| {
        format!(
            "required zwlr_layer_shell_v1 global is unavailable; a layer-shell compositor is required: {error}"
        )
    })?;

    let registry_state = RegistryState::new(&globals);
    let output_state = OutputState::new(&globals, &queue_handle);
    let mut client = WaylandClient::new(registry_state, output_state);

    // OutputState has bound the advertised outputs, but their metadata arrives
    // as events. This roundtrip is finite and establishes the deterministic
    // selection input before a layer role is created.
    event_queue.roundtrip(&mut client)?;
    let (selected_output, selected_info) = client
        .choose_output()
        .ok_or_else(|| "no output with complete metadata was advertised".to_owned())?;

    println!(
        "trace globals-ready selected-output={} name={} logical-size={:?} scale={} transform={:?}",
        selected_info.id,
        selected_info.name.as_deref().unwrap_or("<unnamed>"),
        selected_info.logical_size,
        selected_info.scale_factor,
        selected_info.transform
    );

    let surface = compositor.create_surface(&queue_handle);
    let layer = layer_shell.create_layer_surface(
        &queue_handle,
        surface,
        Layer::Background,
        Some("worldwall"),
        Some(&selected_output),
    );

    // Full-screen background: all anchors, client-chosen size, no exclusive
    // input focus, and an explicit -1 exclusive zone. The latter is deliberate
    // S01 policy and remains visible in the trace for compositor validation.
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_size(0, 0);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);

    // Passing None would restore the default infinite input region. An empty
    // wl_region is therefore required for a wallpaper that must not intercept
    // pointer or touch input.
    let empty_input_region = Region::new(&compositor)?;
    layer.set_input_region(Some(empty_input_region.wl_region()));
    layer.commit();

    client.install_layer(layer.wl_surface().clone(), selected_output, selected_info);
    println!(
        "trace role-settings layer=background anchors=top|bottom|left|right size=0x0 keyboard=none input-region=empty exclusive-zone=-1"
    );
    println!("trace initial-commit buffer=none awaiting-configure=true");

    // The second roundtrip is the explicit configure/ack barrier. No GPU
    // surface is created until SCTK has recorded a nonzero physical extent.
    event_queue.roundtrip(&mut client)?;
    let extent = client.physical_extent().ok_or_else(|| {
        "compositor did not provide a nonzero physical configure extent".to_owned()
    })?;

    // Preserve the accepted S01 layer-only path as an explicit diagnostic
    // baseline. Normal operation always takes the GPU path below.
    if std::env::var_os("WORLDWALL_LAYER_ONLY").is_some() {
        println!("trace diagnostic layer-only=true gpu=false");
        loop {
            match event_queue.blocking_dispatch(&mut client) {
                Ok(_) if client.should_exit() => break,
                Ok(_) => {}
                Err(error) => {
                    client.mark_disconnected();
                    eprintln!("trace compositor-disconnect state=disconnected error={error}");
                    break;
                }
            }
        }
        drop(layer);
        client.shutdown_owners();
        println!("trace shutdown state={:?}", client.lifecycle.state());
        return Ok(());
    }

    // Keep the validated selection transaction alive with the renderer. S05
    // has no public control source yet, so only startup is invoked here; a
    // later explicit request must use the begin/complete handoff below rather
    // than letting output/config/weather callbacks select implicitly.
    let (scene, _registry_selection) = load_scene()?;
    let mut renderer = Renderer::new_with_scene(&connection, layer.wl_surface(), extent, &scene)?;
    println!(
        "trace scene-upload-count={} steady-state-reuploads=false",
        renderer.scene_upload_count()
    );

    let fps = configured_fps();
    let mut scheduler = FrameScheduler::new(fps);
    let clock_origin = Instant::now();
    #[cfg(debug_assertions)]
    let test_exit_deadline = test_exit_deadline(clock_origin);

    // wl_surface.frame is queued before wgpu's present, which performs the
    // actual commit. There is no speculative duplicate wl_surface.commit.
    client.request_frame(&queue_handle);
    match render_renderer(&connection, &mut renderer, extent, 0.0)? {
        RenderOutcome::Presented => {
            let _ = scheduler.submit_if_due(0);
            println!(
                "trace animation-bootstrap fps={} phase-seconds=0 animation-uniform-updates={}",
                match fps {
                    Fps::Fifteen => 15,
                    Fps::Thirty => 30,
                },
                renderer.animation_uniform_update_count(),
            );
        }
        RenderOutcome::Timeout => eprintln!("trace gpu-frame skipped reason=timeout bounded=true"),
        RenderOutcome::Occluded => {
            eprintln!("trace gpu-frame skipped reason=occluded bounded=true")
        }
        RenderOutcome::Reconfigure | RenderOutcome::Fatal => {
            unreachable!("render_renderer classifies these")
        }
    }

    let mut event_loop: EventLoop<WaylandClient> = EventLoop::try_new()?;
    // The timer is a deadline wakeup, not a frame loop: the scheduler still
    // requires a compositor callback before every steady-state submission.
    let interval = Duration::from_nanos(fps.interval_nanos());
    event_loop
        .handle()
        .insert_source(Timer::from_duration(interval), move |_, _, client| {
            client.mark_timer_fired();
            TimeoutAction::ToDuration(interval)
        })?;
    WaylandSource::new(connection.clone(), event_queue).insert(event_loop.handle())?;

    let mut seen_reconfigure_generation = client.reconfigure_generation;
    loop {
        if let Err(error) = event_loop.dispatch(None, &mut client) {
            client.mark_disconnected();
            eprintln!("trace compositor-disconnect state=disconnected error={error}");
            break;
        }
        if client.should_exit() {
            break;
        }
        #[cfg(debug_assertions)]
        if test_exit_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            println!("trace test-exit clean=true reason=deadline");
            break;
        }
        let now_ns = clock_origin.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        if client.callback_ready {
            scheduler.on_callback();
            client.callback_ready = false;
        }
        if client.reconfigure_generation != seen_reconfigure_generation {
            scheduler.on_reconfigure();
            seen_reconfigure_generation = client.reconfigure_generation;
        }
        client.timer_fired = false;
        if client.frame_pending || !scheduler.is_due(now_ns) {
            continue;
        }
        let Some(extent) = client.physical_extent() else {
            continue;
        };
        client.request_frame(&queue_handle);
        let animation_time_seconds = (clock_origin.elapsed().as_secs_f64() % 1024.0) as f32;
        match render_renderer(&connection, &mut renderer, extent, animation_time_seconds)? {
            RenderOutcome::Presented => {
                let _ = scheduler.submit_if_due(now_ns);
                println!(
                    "trace animation-frame fps={} phase-seconds={:.6} scene-upload-count={} animation-uniform-updates={}",
                    match fps {
                        Fps::Fifteen => 15,
                        Fps::Thirty => 30,
                    },
                    animation_time_seconds,
                    renderer.scene_upload_count(),
                    renderer.animation_uniform_update_count(),
                );
            }
            RenderOutcome::Timeout => {
                eprintln!("trace gpu-frame skipped reason=timeout bounded=true")
            }
            RenderOutcome::Occluded => {
                eprintln!("trace gpu-frame skipped reason=occluded bounded=true")
            }
            RenderOutcome::Reconfigure | RenderOutcome::Fatal => {
                unreachable!("render_renderer classifies these")
            }
        }
    }

    // Local drop order is intentional: renderer (and any acquired GPU frame)
    // first, then the LayerSurface owner, then the remaining protocol state.
    drop(event_loop);
    drop(renderer);
    drop(layer);
    client.shutdown_owners();
    println!("trace shutdown state={:?}", client.lifecycle.state());
    Ok(())
}

struct RegistrySelection {
    controller: SelectionController,
    transaction: SceneTransaction,
}

impl RegistrySelection {
    fn startup(root: std::path::PathBuf) -> Result<(Self, Scene), Box<dyn Error>> {
        let registry = SceneRegistry::load(root)?;
        let mut selection = Self {
            controller: SelectionController::new(registry),
            transaction: SceneTransaction::default(),
        };
        let request = selection
            .controller
            .request(EnvironmentSnapshot::unknown(), 0, SelectionTrigger::Startup)
            .map_err(|result| format!("startup registry selection failed: {result:?}"))?;
        let token = selection.transaction.begin(&request);
        let manifest = selection
            .controller
            .registry()
            .get(&request.id)
            .ok_or_else(|| format!("validated registry lost selected scene {:?}", request.id))?;
        let candidate = manifest.load_scene()?;
        let selected_id = candidate.id.clone();
        let outcome = selection.transaction.complete(token, Ok(candidate));
        if !matches!(outcome, CommitOutcome::Applied { .. }) {
            return Err(format!("startup registry transaction failed: {outcome:?}").into());
        }
        selection.controller.mark_active(&selected_id)?;
        let scene = selection
            .transaction
            .active()
            .ok_or_else(|| "startup registry transaction produced no active scene".to_owned())?
            .scene
            .clone();
        println!(
            "trace scene-source=registry id={} trigger=startup generation={} rationale={:?} image={}x{}",
            selected_id,
            request.generation,
            request.rationale,
            scene.image.width,
            scene.image.height
        );
        Ok((selection, scene))
    }

    /// Begin one explicit request and return a cloneable manifest for loading
    /// outside the Wayland dispatch thread. The returned token must be paired
    /// with `complete_loaded`; newer requests make older tokens stale.
    #[allow(dead_code)]
    fn begin_explicit_request(
        &mut self,
        snapshot: EnvironmentSnapshot,
        seed: u64,
        trigger: SelectionTrigger,
    ) -> Result<(SelectionRequest, SceneLoadToken, SceneManifest), Box<dyn Error>> {
        let request = self
            .controller
            .request(snapshot, seed, trigger)
            .map_err(|result| format!("explicit scene request was not accepted: {result:?}"))?;
        let token = self.transaction.begin(&request);
        let manifest = self
            .controller
            .registry()
            .get(&request.id)
            .cloned()
            .ok_or_else(|| format!("validated registry lost requested scene {:?}", request.id))?;
        Ok((request, token, manifest))
    }

    /// Finish an explicit request at the renderer-safe boundary. Asset
    /// decoding is supplied as a completed result, so this method performs no
    /// filesystem work while Wayland dispatch is active.
    #[allow(dead_code)]
    fn complete_loaded<'wayland>(
        &mut self,
        renderer: &mut Renderer<'wayland>,
        request: &SelectionRequest,
        token: SceneLoadToken,
        candidate: Result<LoadedScene, RegistryError>,
    ) -> Result<CommitOutcome, Box<dyn Error>> {
        let outcome = self
            .transaction
            .complete_with_swap(token, candidate, |scene| {
                renderer.upload_and_swap_scene(scene)
            });
        if let CommitOutcome::Applied { ref id, .. } = outcome {
            self.controller.mark_active(id)?;
        }
        println!(
            "trace scene-selection generation={} id={} trigger={:?} outcome={outcome:?}",
            request.generation, request.id, request.trigger
        );
        Ok(outcome)
    }
}

fn load_scene() -> Result<(Scene, Option<RegistrySelection>), Box<dyn Error>> {
    if let Some(root) = std::env::var_os("WORLDWALL_REGISTRY_DIR") {
        let (selection, scene) = RegistrySelection::startup(std::path::PathBuf::from(root))?;
        return Ok((scene, Some(selection)));
    }

    let scene = match std::env::var_os("WORLDWALL_SCENE_DIR") {
        Some(directory) => {
            let directory = std::path::PathBuf::from(directory);
            let animation_path = directory.join("animation.toml");
            let animation_path = animation_path.is_file().then_some(animation_path);
            let scene = crate::scene::load_png_pair_with_animation(
                directory.join("scene.png"),
                directory.join("palette.png"),
                animation_path,
            )?;
            println!(
                "trace scene-source=package image={}x{}",
                scene.image.width, scene.image.height
            );
            scene
        }
        None => {
            println!("trace scene-source=synthetic-reference");
            Scene::reference()
        }
    };
    Ok((scene, None))
}

#[cfg(debug_assertions)]
fn test_exit_deadline(origin: Instant) -> Option<Instant> {
    let milliseconds = std::env::var("WORLDWALL_TEST_EXIT_AFTER_MS")
        .ok()?
        .parse::<u64>()
        .ok()?;
    Some(origin + Duration::from_millis(milliseconds))
}

fn configured_fps() -> Fps {
    match std::env::var("WORLDWALL_FPS").ok().as_deref() {
        Some("15") => Fps::Fifteen,
        Some("30") | None => Fps::Thirty,
        Some(value) => {
            eprintln!("worldwall: unsupported WORLDWALL_FPS={value:?}; using 30");
            Fps::Thirty
        }
    }
}

fn render_renderer<'wayland>(
    connection: &'wayland Connection,
    renderer: &mut Renderer<'wayland>,
    extent: (u32, u32),
    animation_time_seconds: f32,
) -> Result<RenderOutcome, Box<dyn Error>> {
    renderer.reconfigure(extent)?;
    let outcome = renderer.render_at(animation_time_seconds)?;
    match outcome {
        RenderOutcome::Presented | RenderOutcome::Timeout | RenderOutcome::Occluded => Ok(outcome),
        RenderOutcome::Reconfigure => {
            eprintln!("trace gpu-frame recovery=surface-recreate attempt=1");
            renderer.recreate_surface(connection)?;
            renderer.reconfigure(extent)?;
            match renderer.render_at(animation_time_seconds)? {
                RenderOutcome::Presented => Ok(RenderOutcome::Presented),
                outcome => Err(format!(
                    "surface recovery did not present one bounded retry: {outcome:?}"
                )
                .into()),
            }
        }
        RenderOutcome::Fatal => Err("fatal wgpu surface validation failure".into()),
    }
}

/// Lifecycle states owned by one selected output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleState {
    Discovered,
    RoleCreated,
    AwaitingConfigure,
    Ready,
    Closing,
    Removed,
    Disconnected,
}

/// A physical extent derived from logical layer coordinates and an integer
/// output scale. It is deliberately separate from the logical configure size;
/// S02 will use it to reject zero GPU extents before surface creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhysicalExtent {
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConfigureRecord {
    serial: u32,
    logical_width: u32,
    logical_height: u32,
    physical_extent: Option<PhysicalExtent>,
}

#[derive(Debug)]
struct OutputLifecycle {
    state: LifecycleState,
    scale_factor: u32,
    fallback_logical_size: Option<(u32, u32)>,
    last_configure: Option<ConfigureRecord>,
}

impl OutputLifecycle {
    fn new(scale_factor: i32, fallback_logical_size: Option<(i32, i32)>) -> Self {
        Self {
            state: LifecycleState::Discovered,
            // wl_output scale is positive. Keep invalid metadata visible but
            // prevent it from becoming a zero physical extent.
            scale_factor: scale_factor.max(1) as u32,
            fallback_logical_size: positive_size(fallback_logical_size),
            last_configure: None,
        }
    }

    fn state(&self) -> LifecycleState {
        self.state
    }

    fn role_created(&mut self) {
        if self.state == LifecycleState::Discovered {
            self.state = LifecycleState::RoleCreated;
        }
    }

    fn initial_commit(&mut self) {
        if self.state == LifecycleState::RoleCreated {
            self.state = LifecycleState::AwaitingConfigure;
        }
    }

    /// Record the configure after SCTK has acknowledged its serial.
    ///
    /// `smithay-client-toolkit` acknowledges the layer configure in its
    /// dispatch implementation before invoking `LayerShellHandler::configure`.
    /// Keeping the acknowledgement in that one toolkit path avoids a double
    /// ack and makes this handler a state/storage boundary only.
    fn configured(&mut self, serial: u32, suggested_size: (u32, u32)) {
        if matches!(
            self.state,
            LifecycleState::Closing | LifecycleState::Removed | LifecycleState::Disconnected
        ) {
            return;
        }

        let logical_width = nonzero_or(
            suggested_size.0,
            self.fallback_logical_size.map(|size| size.0),
        );
        let logical_height = nonzero_or(
            suggested_size.1,
            self.fallback_logical_size.map(|size| size.1),
        );
        let physical_extent = match (logical_width, logical_height) {
            (Some(width), Some(height)) => width
                .checked_mul(self.scale_factor)
                .zip(height.checked_mul(self.scale_factor))
                .map(|(width, height)| PhysicalExtent { width, height }),
            _ => None,
        };

        self.last_configure = Some(ConfigureRecord {
            serial,
            logical_width: logical_width.unwrap_or(0),
            logical_height: logical_height.unwrap_or(0),
            physical_extent,
        });
        self.state = if physical_extent.is_some() {
            LifecycleState::Ready
        } else {
            LifecycleState::AwaitingConfigure
        };
    }

    fn scale_changed(&mut self, scale_factor: i32) {
        if scale_factor > 0 {
            self.scale_factor = scale_factor as u32;
            if let Some(configure) = &mut self.last_configure {
                configure.physical_extent = configure
                    .logical_width
                    .checked_mul(self.scale_factor)
                    .zip(configure.logical_height.checked_mul(self.scale_factor))
                    .map(|(width, height)| PhysicalExtent { width, height });
                if configure.physical_extent.is_none() {
                    self.state = LifecycleState::AwaitingConfigure;
                } else if self.state == LifecycleState::AwaitingConfigure {
                    self.state = LifecycleState::Ready;
                }
            }
        }
    }

    fn close(&mut self) {
        if !matches!(
            self.state,
            LifecycleState::Removed | LifecycleState::Disconnected
        ) {
            self.state = LifecycleState::Closing;
        }
    }

    fn output_removed(&mut self) {
        if self.state != LifecycleState::Disconnected {
            self.state = LifecycleState::Removed;
        }
    }

    fn disconnected(&mut self) {
        self.state = LifecycleState::Disconnected;
    }
}

fn positive_size(size: Option<(i32, i32)>) -> Option<(u32, u32)> {
    size.and_then(|(width, height)| {
        u32::try_from(width)
            .ok()
            .zip(u32::try_from(height).ok())
            .filter(|&(width, height)| width > 0 && height > 0)
    })
}

fn nonzero_or(value: u32, fallback: Option<u32>) -> Option<u32> {
    (value > 0)
        .then_some(value)
        .or(fallback.filter(|value| *value > 0))
}

/// Application state. Wayland protocol owners are kept together so that the
/// layer role and its wl_surface are dropped before the registry/output owners
/// during normal scope teardown. No dependent GPU surface exists in S01.
struct WaylandClient {
    registry_state: RegistryState,
    output_state: OutputState,
    selected_output: Option<wl_output::WlOutput>,
    surface: Option<wl_surface::WlSurface>,
    lifecycle: OutputLifecycle,
    render_requested: bool,
    frame_pending: bool,
    callback_ready: bool,
    timer_fired: bool,
    reconfigure_generation: u64,
    #[cfg(debug_assertions)]
    withhold_callbacks: bool,
}

impl WaylandClient {
    fn new(registry_state: RegistryState, output_state: OutputState) -> Self {
        Self {
            registry_state,
            output_state,
            selected_output: None,
            surface: None,
            lifecycle: OutputLifecycle::new(1, None),
            render_requested: false,
            frame_pending: false,
            callback_ready: false,
            timer_fired: false,
            reconfigure_generation: 0,
            #[cfg(debug_assertions)]
            withhold_callbacks: std::env::var_os("WORLDWALL_TEST_WITHHOLD_CALLBACK").is_some(),
        }
    }

    fn choose_output(&self) -> Option<(wl_output::WlOutput, OutputInfo)> {
        let mut candidates: Vec<_> = self
            .output_state
            .outputs()
            .filter_map(|output| self.output_state.info(&output).map(|info| (output, info)))
            .collect();

        candidates.sort_by(|(_, left), (_, right)| {
            output_sort_key(left.id, left.location, left.name.as_deref()).cmp(&output_sort_key(
                right.id,
                right.location,
                right.name.as_deref(),
            ))
        });
        candidates.into_iter().next()
    }

    fn install_layer(
        &mut self,
        surface: wl_surface::WlSurface,
        selected_output: wl_output::WlOutput,
        selected_info: OutputInfo,
    ) {
        self.selected_output = Some(selected_output);
        self.lifecycle =
            OutputLifecycle::new(selected_info.scale_factor, selected_info.logical_size);
        self.lifecycle.role_created();
        self.lifecycle.initial_commit();
        self.surface = Some(surface);
    }

    fn physical_extent(&self) -> Option<(u32, u32)> {
        self.lifecycle
            .last_configure
            .and_then(|configure| configure.physical_extent)
            .map(|extent| (extent.width, extent.height))
    }

    fn request_frame(&mut self, queue_handle: &QueueHandle<Self>) {
        if !self.frame_pending {
            if let Some(surface) = &self.surface {
                surface.frame(queue_handle, FrameCallbackData(surface.clone()));
                self.frame_pending = true;
                self.callback_ready = false;
                println!("trace frame-requested before-present=true");
            }
        }
    }

    fn should_exit(&self) -> bool {
        matches!(
            self.lifecycle.state(),
            LifecycleState::Closing | LifecycleState::Removed | LifecycleState::Disconnected
        )
    }

    fn mark_disconnected(&mut self) {
        self.lifecycle.disconnected();
    }

    /// Drop the role before the selected output and registry owners. This is
    /// called after the dispatch loop has stopped, and is also safe after a
    /// compositor disconnect because the protocol proxies become inert.
    fn shutdown_owners(&mut self) {
        self.surface.take();
        self.selected_output.take();
        println!("trace owners-dropped gpu-before-layer-before-selected-output=true");
    }

    fn mark_timer_fired(&mut self) {
        self.timer_fired = true;
    }
}

fn output_sort_key(id: u32, location: (i32, i32), name: Option<&str>) -> (i32, i32, String, u32) {
    (
        location.1,
        location.0,
        name.unwrap_or_default().to_owned(),
        id,
    )
}

impl CompositorHandler for WaylandClient {
    fn scale_factor_changed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        if self
            .surface
            .as_ref()
            .is_some_and(|layer_surface| layer_surface == surface)
        {
            self.lifecycle.scale_changed(new_factor);
            self.render_requested = self.lifecycle.state() == LifecycleState::Ready;
            self.reconfigure_generation = self.reconfigure_generation.saturating_add(1);
            println!(
                "trace surface-scale factor={} state={:?}",
                new_factor,
                self.lifecycle.state()
            );
        }
    }

    fn transform_changed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_transform: wl_output::Transform,
    ) {
        if self
            .surface
            .as_ref()
            .is_some_and(|layer_surface| layer_surface == surface)
        {
            println!("trace surface-transform transform={new_transform:?}");
        }
    }

    fn frame(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        if self
            .surface
            .as_ref()
            .is_some_and(|layer_surface| layer_surface == surface)
        {
            #[cfg(debug_assertions)]
            if self.withhold_callbacks {
                // Debug-only evidence seam: retain the pending gate even after
                // the compositor reports a callback, so deadline wakeups can
                // prove that no frame is submitted without readiness.
                self.frame_pending = true;
                self.callback_ready = false;
                println!("trace frame-callback withheld=true time={_time}");
                return;
            }
            self.frame_pending = false;
            self.callback_ready = true;
            println!("trace frame-callback ready=true time={_time}");
        }
    }

    fn surface_enter(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if self
            .surface
            .as_ref()
            .is_some_and(|layer_surface| layer_surface == surface)
        {
            println!(
                "trace surface-enter selected={}",
                self.selected_output.as_ref() == Some(output)
            );
        }
    }

    fn surface_leave(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if self
            .surface
            .as_ref()
            .is_some_and(|layer_surface| layer_surface == surface)
        {
            println!(
                "trace surface-leave selected={}",
                self.selected_output.as_ref() == Some(output)
            );
        }
    }
}

impl OutputHandler for WaylandClient {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        println!("trace output-added id={}", output.id());
    }

    fn update_output(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if self.selected_output.as_ref() == Some(&output) {
            println!("trace selected-output-updated id={}", output.id());
        }
    }

    fn output_destroyed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if self.selected_output.as_ref() == Some(&output) {
            self.lifecycle.output_removed();
            // Stop work immediately. The owning LayerSurface is local to run
            // and is dropped after the GPU renderer and before this output.
            self.render_requested = false;
            self.selected_output.take();
            println!(
                "trace selected-output-removed id={} state=removed owners-dropped=true",
                output.id()
            );
        }
    }
}

impl LayerShellHandler for WaylandClient {
    fn closed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _layer: &LayerSurface,
    ) {
        self.lifecycle.close();
        // The callback argument keeps the role alive for this callback. The
        // owning LayerSurface is dropped by run after GPU resources.
        self.render_requested = false;
        println!("trace layer-closed state=closing owners-dropped=true");
    }

    fn configure(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        serial: u32,
    ) {
        self.lifecycle.configured(serial, configure.new_size);
        self.render_requested = self.lifecycle.state() == LifecycleState::Ready;
        self.reconfigure_generation = self.reconfigure_generation.saturating_add(1);
        let record = self.lifecycle.last_configure;
        println!(
            "trace configure serial={} suggested={}x{} logical={}x{} physical={:?} state={:?} ack=toolkit-before-handler",
            serial,
            configure.new_size.0,
            configure.new_size.1,
            record.map_or(0, |record| record.logical_width),
            record.map_or(0, |record| record.logical_height),
            record.and_then(|record| record.physical_extent),
            self.lifecycle.state(),
        );
    }
}

delegate_registry!(WaylandClient);

impl ProvidesRegistryState for WaylandClient {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}

smithay_client_toolkit::delegate_dispatch2!(WaylandClient);

impl fmt::Display for LifecycleState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_configure_uses_known_output_size_and_integer_scale() {
        let mut lifecycle = OutputLifecycle::new(2, Some((1920, 1080)));
        lifecycle.role_created();
        lifecycle.initial_commit();

        lifecycle.configured(7, (0, 0));

        assert_eq!(lifecycle.state(), LifecycleState::Ready);
        assert_eq!(
            lifecycle.last_configure,
            Some(ConfigureRecord {
                serial: 7,
                logical_width: 1920,
                logical_height: 1080,
                physical_extent: Some(PhysicalExtent {
                    width: 3840,
                    height: 2160
                }),
            })
        );
    }

    #[test]
    fn unresolved_zero_configure_never_becomes_ready() {
        let mut lifecycle = OutputLifecycle::new(1, None);
        lifecycle.role_created();
        lifecycle.initial_commit();

        lifecycle.configured(8, (0, 1080));

        assert_eq!(lifecycle.state(), LifecycleState::AwaitingConfigure);
        assert_eq!(lifecycle.last_configure.unwrap().physical_extent, None);
    }

    #[test]
    fn resize_replaces_logical_and_physical_extent() {
        let mut lifecycle = OutputLifecycle::new(1, Some((1280, 720)));
        lifecycle.role_created();
        lifecycle.initial_commit();
        lifecycle.configured(1, (1280, 720));
        lifecycle.configured(2, (2560, 1440));

        assert_eq!(lifecycle.state(), LifecycleState::Ready);
        assert_eq!(lifecycle.last_configure.unwrap().serial, 2);
        assert_eq!(
            lifecycle.last_configure.unwrap().physical_extent,
            Some(PhysicalExtent {
                width: 2560,
                height: 1440
            })
        );
    }

    #[test]
    fn scale_change_recomputes_last_extent_without_reselecting() {
        let mut lifecycle = OutputLifecycle::new(1, Some((1920, 1080)));
        lifecycle.role_created();
        lifecycle.initial_commit();
        lifecycle.configured(3, (1920, 1080));
        lifecycle.scale_changed(2);

        assert_eq!(lifecycle.state(), LifecycleState::Ready);
        assert_eq!(
            lifecycle.last_configure.unwrap().physical_extent,
            Some(PhysicalExtent {
                width: 3840,
                height: 2160
            })
        );
    }

    #[test]
    fn close_removal_and_disconnect_are_terminal_for_work() {
        let mut closing = OutputLifecycle::new(1, None);
        closing.role_created();
        closing.initial_commit();
        closing.close();
        closing.configured(4, (1920, 1080));
        assert_eq!(closing.state(), LifecycleState::Closing);

        let mut removed = OutputLifecycle::new(1, None);
        removed.output_removed();
        removed.configured(5, (1920, 1080));
        assert_eq!(removed.state(), LifecycleState::Removed);

        let mut disconnected = OutputLifecycle::new(1, None);
        disconnected.disconnected();
        disconnected.close();
        assert_eq!(disconnected.state(), LifecycleState::Disconnected);
    }

    #[test]
    fn output_selection_key_is_stable_for_same_layout() {
        let first = output_sort_key(4, (0, 0), Some("DP-1"));
        let second = output_sort_key(3, (0, 0), Some("DP-1"));

        assert_eq!(first.cmp(&second), Ordering::Greater);
    }
}
