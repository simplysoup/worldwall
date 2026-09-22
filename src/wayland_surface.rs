//! Single-output Wayland layer-shell ownership and S02 presentation loop.
//!
//! SCTK owns protocol dispatch and layer configuration. Once a nonzero
//! configure is acknowledged, the retained layer surface is handed to the
//! renderer for one event-driven bootstrap frame and later resize frames.
#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    time::{Duration, Instant},
};

use calloop::{
    EventLoop, RegistrationToken,
    channel::Event as ChannelEvent,
    timer::{TimeoutAction, Timer},
};
use calloop_wayland_source::WaylandSource;

#[cfg(test)]
use std::cmp::Ordering;

use serde_json::{Value, json};

use crate::{
    config::AppConfig,
    environment::{
        EnvironmentProvider, Location, NoaaAstronomy, SystemWallClock, UtcOffsetSchedule,
        WallClock, Weather,
    },
    ipc::{self, Command, ControlRequest, IpcServer},
    power::{PausedAnimationClock, PowerAction, PowerEvent, PowerEventSource, PowerStateMachine},
    renderer::{AllocationCounters, OutputRenderer, RenderOutcome, Renderer, SharedRenderer},
    scene::Scene,
    scene_registry::{
        CommitOutcome, EnvironmentSnapshot, LoadedScene, RegistryError, SceneLoadToken,
        SceneManifest, SceneRegistry, SceneTransaction, SelectionController, SelectionRequest,
        SelectionTrigger,
    },
    scheduler::{Fps, FrameScheduler},
    weather::{
        CacheStore, Freshness, OpenMeteoProvider, RefreshKind, WeatherConfig, WeatherService,
        WeatherState,
    },
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region},
    delegate_registry,
    output::{OutputHandler, OutputInfo, OutputState},
    registry::{ProvidesRegistryState, RegistryState, SimpleGlobal},
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
    Connection, Dispatch, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_surface},
};
use wayland_protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
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

    // Weather is optional and starts only after explicit provider/location
    // configuration. Its startup request is asynchronous; selection uses the
    // cache snapshot available at this boundary and never waits on network I/O.
    let mut weather = WeatherRuntime::startup()?;
    let civil_time = CivilTimeRuntime::startup()?;
    // Keep the validated selection transaction alive with the renderer. S05
    // has no public control source yet, so only startup is invoked here; a
    // later explicit request must use the begin/complete handoff below rather
    // than letting output/config/weather callbacks select implicitly.
    let config = AppConfig::from_environment()?;
    let (scene, mut registry_selection) = load_scene(
        civil_time.snapshot(weather.weather()),
        config.registry_dir.as_deref(),
    )?;
    let mut renderer = Renderer::new_with_scene(&connection, layer.wl_surface(), extent, &scene)?;
    println!(
        "trace scene-upload-count={} steady-state-reuploads=false",
        renderer.scene_upload_count()
    );

    let fps = config.fps;
    let mut scheduler = FrameScheduler::new(fps);
    let clock_origin = Instant::now();
    let mut animation_clock = PausedAnimationClock::new(0);
    let mut power = PowerStateMachine::new();
    #[cfg(debug_assertions)]
    let test_exit_deadline = test_exit_deadline(clock_origin);

    // wl_surface.frame is queued before wgpu's present, which performs the
    // actual commit. There is no speculative duplicate wl_surface.commit.
    client.request_frame(&queue_handle);
    match render_renderer(
        &connection,
        &mut renderer,
        extent,
        animation_clock.seconds(0),
        civil_time.seconds(),
    )? {
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
    let mut frame_timer = Some(insert_frame_timer(&event_loop, interval)?);
    // This low-frequency wakeup is separate from the animation timer. Weather
    // completion/deadline checks are therefore not performed per frame.
    let weather_poll_interval = Duration::from_secs(1);
    let mut weather_timer = Some(insert_weather_timer(&event_loop, weather_poll_interval)?);
    let power_events = PowerEventSource::start().into_event_source();
    event_loop
        .handle()
        .insert_source(power_events, move |event, _, client| match event {
            ChannelEvent::Msg(event) => client.mark_power_event(event),
            ChannelEvent::Closed => client.mark_power_event(PowerEvent::Disconnected),
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
        for event in client.take_power_events() {
            let action = power.on_event(event);
            println!(
                "trace power-event event={event:?} action={action:?} generation={} service={:?} phase={:?}",
                power.generation(),
                power.service(),
                power.phase()
            );
            match action {
                PowerAction::Pause { generation } => {
                    scheduler.pause();
                    animation_clock.pause(now_ns);
                    client.pause_rendering();
                    if let Some(token) = frame_timer.take() {
                        event_loop.handle().remove(token);
                    }
                    if let Some(token) = weather_timer.take() {
                        event_loop.handle().remove(token);
                    }
                    println!(
                        "trace suspend state=paused generation={generation} deadlines=false weather-refresh=false"
                    );
                }
                PowerAction::BeginResume { generation } => {
                    println!(
                        "trace resume state=pending generation={generation} output-ready={}",
                        client.physical_extent().is_some()
                    );
                }
                PowerAction::ServiceDegraded => {
                    eprintln!(
                        "trace power-service state=degraded resume-support=false manual-refresh=true"
                    );
                }
                PowerAction::ServiceRestored => {
                    println!("trace power-service state=available resume-support=true");
                }
                PowerAction::Ignored => {}
            }
        }
        if let Some(generation) = power.begin_resume_if_ready(client.physical_extent().is_some()) {
            weather.refresh_for_resume();
            let snapshot = civil_time.snapshot(weather.weather());
            if let Some(selection) = registry_selection.as_mut() {
                match selection.begin_explicit_request(
                    snapshot,
                    generation,
                    SelectionTrigger::Resume,
                ) {
                    Ok((request, token, manifest)) => {
                        let candidate = manifest.load_scene();
                        match selection.complete_loaded(&mut renderer, &request, token, candidate) {
                            Ok(outcome) => println!(
                                "trace resume-selection generation={generation} outcome={outcome:?}"
                            ),
                            Err(error) => eprintln!(
                                "trace resume-selection generation={generation} outcome=failed error={error}"
                            ),
                        }
                    }
                    Err(error) => eprintln!(
                        "trace resume-selection generation={generation} outcome=retained error={error}"
                    ),
                }
            } else {
                println!(
                    "trace resume-selection generation={generation} outcome=retained fixed-scene=true"
                );
            }
            scheduler.resume();
            animation_clock.resume(now_ns);
            client.resume_rendering();
            if frame_timer.is_none() {
                frame_timer = Some(insert_frame_timer(&event_loop, interval)?);
            }
            if weather_timer.is_none() {
                weather_timer = Some(insert_weather_timer(&event_loop, weather_poll_interval)?);
            }
            client.request_frame(&queue_handle);
            let Some(extent) = client.physical_extent() else {
                continue;
            };
            let animation_time_seconds = animation_clock.seconds(now_ns);
            match render_renderer(
                &connection,
                &mut renderer,
                extent,
                animation_time_seconds,
                civil_time.seconds(),
            )? {
                RenderOutcome::Presented => {
                    let _ = scheduler.submit_if_due(now_ns);
                    println!(
                        "trace resume-presented generation={generation} phase-seconds={animation_time_seconds:.6} scene-upload-count={}",
                        renderer.scene_upload_count()
                    );
                }
                RenderOutcome::Timeout | RenderOutcome::Occluded => eprintln!(
                    "trace resume-presented=false generation={generation} reason=surface-not-ready"
                ),
                RenderOutcome::Reconfigure | RenderOutcome::Fatal => {
                    unreachable!("render_renderer classifies these")
                }
            }
            let _ = power.complete_resume(generation);
            continue;
        }
        if client.callback_ready && !scheduler.is_paused() {
            scheduler.on_callback();
            client.callback_ready = false;
        }
        if client.reconfigure_generation != seen_reconfigure_generation {
            scheduler.on_reconfigure();
            seen_reconfigure_generation = client.reconfigure_generation;
        }
        if client.weather_timer_fired {
            client.weather_timer_fired = false;
            weather.poll();
        }
        client.timer_fired = false;
        if client.render_paused || client.frame_pending || !scheduler.is_due(now_ns) {
            continue;
        }
        let Some(extent) = client.physical_extent() else {
            continue;
        };
        client.request_frame(&queue_handle);
        let animation_time_seconds = animation_clock.seconds(now_ns);
        match render_renderer(
            &connection,
            &mut renderer,
            extent,
            animation_time_seconds,
            civil_time.seconds(),
        )? {
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

struct CivilTimeRuntime {
    offset_minutes: Option<i32>,
    location: Option<Location>,
    clock: SystemWallClock,
}

impl CivilTimeRuntime {
    fn startup() -> Result<Self, Box<dyn Error>> {
        let offset_minutes = match std::env::var("WORLDWALL_UTC_OFFSET_MINUTES") {
            Ok(value) => Some(value.parse::<i32>().map_err(
                |_| "WORLDWALL_UTC_OFFSET_MINUTES must be a finite whole-minute offset",
            )?),
            Err(std::env::VarError::NotPresent) => None,
            Err(error) => {
                return Err(format!("reading WORLDWALL_UTC_OFFSET_MINUTES failed: {error}").into());
            }
        };
        if let Some(offset) = offset_minutes {
            if !(-14 * 60..=14 * 60).contains(&offset) {
                return Err("WORLDWALL_UTC_OFFSET_MINUTES must be between -840 and 840".into());
            }
        }
        let location = configured_location()?;
        println!(
            "trace civil-time configured={} astronomy-location-configured={}",
            offset_minutes.is_some(),
            location.is_some()
        );
        Ok(Self {
            offset_minutes,
            location,
            clock: SystemWallClock,
        })
    }

    fn snapshot(&self, weather: Weather) -> EnvironmentSnapshot {
        let (Some(offset_minutes), Some(location)) = (self.offset_minutes, self.location) else {
            return EnvironmentSnapshot {
                time: crate::scene_registry::TimeOfDay::Unknown,
                weather,
                season: crate::scene_registry::Season::Unknown,
            };
        };
        let Ok(timezone) = UtcOffsetSchedule::fixed(offset_minutes) else {
            return EnvironmentSnapshot {
                time: crate::scene_registry::TimeOfDay::Unknown,
                weather,
                season: crate::scene_registry::Season::Unknown,
            };
        };
        let Ok(utc_seconds) = self.clock.now_utc_seconds() else {
            return EnvironmentSnapshot {
                time: crate::scene_registry::TimeOfDay::Unknown,
                weather,
                season: crate::scene_registry::Season::Unknown,
            };
        };
        EnvironmentProvider {
            location: Some(location),
            timezone: Some(timezone),
            astronomy: NoaaAstronomy,
        }
        .snapshot_at(utc_seconds, weather)
    }

    fn seconds(&self) -> u32 {
        let Ok(utc_seconds) = self.clock.now_utc_seconds() else {
            return 0;
        };
        let Some(offset_minutes) = self.offset_minutes else {
            return 0;
        };
        let offset = i64::from(offset_minutes) * 60;
        utc_seconds
            .saturating_add(offset)
            .rem_euclid(crate::scene::CIVIL_SECONDS_PER_DAY as i64) as u32
    }
}

struct WeatherRuntime {
    service: Option<WeatherService>,
    clock: SystemWallClock,
    reported_epoch: u64,
}

impl WeatherRuntime {
    fn startup() -> Result<Self, Box<dyn Error>> {
        let clock = SystemWallClock;
        let Some(provider) = std::env::var_os("WORLDWALL_WEATHER") else {
            return Ok(Self {
                service: None,
                clock,
                reported_epoch: 0,
            });
        };
        let provider = provider
            .to_str()
            .ok_or("WORLDWALL_WEATHER must be valid UTF-8 and equal open-meteo when enabled")?;
        if provider != "open-meteo" {
            return Err(format!(
                "unsupported WORLDWALL_WEATHER provider {provider:?}; expected open-meteo"
            )
            .into());
        }
        let latitude = required_weather_coordinate("WORLDWALL_WEATHER_LATITUDE")?;
        let longitude = required_weather_coordinate("WORLDWALL_WEATHER_LONGITUDE")?;
        let location = Location::new(latitude, longitude)
            .map_err(|_| "configured weather coordinates are invalid")?;
        let config = WeatherConfig::open_meteo(location)?;
        let request_deadline = config.request_deadline;
        let mut service = WeatherService::new(config, OpenMeteoProvider::new(request_deadline)?)?;
        let now_utc = clock.now_utc_seconds()?;
        if let Ok(path) = CacheStore::default_path() {
            service.attach_cache_best_effort(CacheStore::new(path), now_utc)?;
            if service.snapshot(now_utc).state == crate::weather::WeatherState::Error {
                eprintln!("trace weather-cache state=error fallback=unknown");
            }
        }
        let refresh = service.request_refresh(now_utc, Instant::now(), RefreshKind::Startup);
        println!("trace weather-enabled provider=open-meteo refresh={refresh:?}");
        Ok(Self {
            service: Some(service),
            clock,
            reported_epoch: 0,
        })
    }

    fn weather(&self) -> Weather {
        self.snapshot().weather
    }

    fn snapshot(&self) -> EnvironmentSnapshot {
        let Some(service) = &self.service else {
            return EnvironmentSnapshot::unknown();
        };
        let Ok(now_utc) = self.clock.now_utc_seconds() else {
            return EnvironmentSnapshot::unknown();
        };
        let weather = service.snapshot(now_utc).weather();
        EnvironmentSnapshot {
            time: crate::scene_registry::TimeOfDay::Unknown,
            weather,
            season: crate::scene_registry::Season::Unknown,
        }
    }

    fn refresh_for_resume(&mut self) {
        let Some(service) = &mut self.service else {
            println!("trace weather-resume state=disabled");
            return;
        };
        let Ok(now_utc) = self.clock.now_utc_seconds() else {
            eprintln!("trace weather-resume state=unknown reason=clock");
            return;
        };
        let now_mono = Instant::now();
        service.poll(now_utc, now_mono);
        let refresh = service.request_refresh(now_utc, now_mono, RefreshKind::Resume);
        println!("trace weather-resume refresh={refresh:?} bounded=true");
    }

    fn poll(&mut self) {
        let Some(service) = &mut self.service else {
            return;
        };
        let Ok(now_utc) = self.clock.now_utc_seconds() else {
            return;
        };
        service.poll(now_utc, Instant::now());
        let epoch = service.cache_epoch();
        if epoch != self.reported_epoch {
            self.reported_epoch = epoch;
            let state = service.snapshot(now_utc).state;
            println!("trace weather-cache epoch={epoch} state={state:?}");
        }
    }

    fn status_value(&self) -> Value {
        let Some(service) = &self.service else {
            return json!({"state": "disabled", "freshness": "unknown", "cache_epoch": 0});
        };
        let Ok(now_utc) = self.clock.now_utc_seconds() else {
            return json!({"state": "error", "freshness": "unknown", "cache_epoch": service.cache_epoch(), "error": "clock unavailable"});
        };
        let snapshot = service.snapshot(now_utc);
        let state = match snapshot.state {
            WeatherState::Disabled => "disabled",
            WeatherState::Unknown => "unknown",
            WeatherState::Fresh => "fresh",
            WeatherState::Stale => "stale",
            WeatherState::Error => "error",
        };
        let freshness = match snapshot.freshness {
            Freshness::Unknown => "unknown",
            Freshness::Fresh => "fresh",
            Freshness::Stale => "stale",
        };
        let mut value = json!({
            "state": state,
            "freshness": freshness,
            "cache_epoch": service.cache_epoch(),
        });
        if let Some(error) = snapshot.last_error {
            // Provider/cache diagnostics are intentionally bounded and never
            // include the configured coordinates.
            value["error"] = Value::String(error);
        }
        value
    }
}

fn required_weather_coordinate(name: &str) -> Result<f64, Box<dyn Error>> {
    let value = std::env::var(name)
        .map_err(|_| format!("{name} is required when WORLDWALL_WEATHER=open-meteo"))?;
    value
        .parse::<f64>()
        .map_err(|_| format!("{name} is not a valid finite decimal coordinate").into())
}

fn configured_location() -> Result<Option<Location>, Box<dyn Error>> {
    let latitude = std::env::var("WORLDWALL_LOCATION_LATITUDE");
    let longitude = std::env::var("WORLDWALL_LOCATION_LONGITUDE");
    match (latitude, longitude) {
        (Ok(latitude), Ok(longitude)) => {
            let latitude = latitude
                .parse::<f64>()
                .map_err(|_| "WORLDWALL_LOCATION_LATITUDE is not a valid finite decimal")?;
            let longitude = longitude
                .parse::<f64>()
                .map_err(|_| "WORLDWALL_LOCATION_LONGITUDE is not a valid finite decimal")?;
            Ok(Some(Location::new(latitude, longitude).map_err(
                |_| "WORLDWALL_LOCATION coordinates are outside the permitted ranges",
            )?))
        }
        (Err(std::env::VarError::NotPresent), Err(std::env::VarError::NotPresent)) => Ok(None),
        _ => Err(
            "WORLDWALL_LOCATION_LATITUDE and WORLDWALL_LOCATION_LONGITUDE must be set together"
                .into(),
        ),
    }
}

struct RegistrySelection {
    controller: SelectionController,
    transaction: SceneTransaction,
}

impl RegistrySelection {
    fn from_registry(registry: SceneRegistry) -> Self {
        Self {
            controller: SelectionController::new(registry),
            transaction: SceneTransaction::default(),
        }
    }

    fn startup(
        root: std::path::PathBuf,
        snapshot: EnvironmentSnapshot,
    ) -> Result<(Self, Scene), Box<dyn Error>> {
        let registry = SceneRegistry::load(root)?;
        let mut selection = Self::from_registry(registry);
        let request = selection
            .controller
            .request(snapshot, 0, SelectionTrigger::Startup)
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

    /// Shared-renderer variant used by the S09 event loop. A scene swap keeps
    /// the device and textures authoritative, then rebuilds only the output
    /// bind groups so old per-output dimensions cannot survive the swap.
    #[allow(dead_code)]
    fn replace_registry(&mut self, registry: SceneRegistry) {
        self.controller.replace_registry(registry);
    }

    fn disable_registry(&mut self) {
        self.controller.replace_registry(SceneRegistry::empty());
    }

    fn complete_loaded_shared<'wayland>(
        &mut self,
        renderer: &mut SharedRenderer,
        presenters: &mut BTreeMap<u32, OutputRenderer<'wayland>>,
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
            for presenter in presenters.values_mut() {
                presenter.refresh_scene_binding(renderer);
            }
        }
        println!(
            "trace scene-selection generation={} id={} trigger={:?} outcome={outcome:?} shared=true",
            request.generation, request.id, request.trigger
        );
        Ok(outcome)
    }
}

fn load_scene(
    snapshot: EnvironmentSnapshot,
    registry_dir: Option<&std::path::Path>,
) -> Result<(Scene, Option<RegistrySelection>), Box<dyn Error>> {
    if let Some(root) = registry_dir {
        let (selection, scene) =
            RegistrySelection::startup(std::path::PathBuf::from(root), snapshot)?;
        return Ok((scene, Some(selection)));
    }

    let scene = match std::env::var_os("WORLDWALL_SCENE_DIR") {
        Some(directory) => {
            let directory = std::path::PathBuf::from(directory);
            let animation_path = directory.join("animation.toml");
            let animation_path = animation_path.is_file().then_some(animation_path);
            let time_of_day_path = directory.join("time_of_day.toml");
            let time_of_day_path = time_of_day_path.is_file().then_some(time_of_day_path);
            let scene = crate::scene::load_png_pair_with_resources(
                directory.join("scene.png"),
                directory.join("palette.png"),
                animation_path,
                time_of_day_path,
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

fn insert_frame_timer(
    event_loop: &EventLoop<WaylandClient>,
    interval: Duration,
) -> Result<RegistrationToken, Box<dyn Error>> {
    Ok(event_loop
        .handle()
        .insert_source(Timer::from_duration(interval), move |_, _, client| {
            client.mark_timer_fired();
            TimeoutAction::ToDuration(interval)
        })?)
}

fn insert_weather_timer(
    event_loop: &EventLoop<WaylandClient>,
    interval: Duration,
) -> Result<RegistrationToken, Box<dyn Error>> {
    Ok(event_loop
        .handle()
        .insert_source(Timer::from_duration(interval), move |_, _, client| {
            client.mark_weather_timer_fired();
            TimeoutAction::ToDuration(interval)
        })?)
}

fn render_renderer<'wayland>(
    connection: &'wayland Connection,
    renderer: &mut Renderer<'wayland>,
    extent: (u32, u32),
    animation_time_seconds: f32,
    civil_seconds: u32,
) -> Result<RenderOutcome, Box<dyn Error>> {
    renderer.reconfigure(extent)?;
    let outcome = renderer.render_at_with_civil_seconds(animation_time_seconds, civil_seconds)?;
    match outcome {
        RenderOutcome::Presented | RenderOutcome::Timeout | RenderOutcome::Occluded => Ok(outcome),
        RenderOutcome::Reconfigure => {
            eprintln!("trace gpu-frame recovery=surface-recreate attempt=1");
            renderer.recreate_surface(connection)?;
            renderer.reconfigure(extent)?;
            match renderer.render_at_with_civil_seconds(animation_time_seconds, civil_seconds)? {
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
    fractional_scale_enabled: bool,
    preferred_scale_120: Option<u32>,
    last_configure: Option<ConfigureRecord>,
}

impl OutputLifecycle {
    fn new(scale_factor: i32, fallback_logical_size: Option<(i32, i32)>) -> Self {
        Self::new_with_fractional(scale_factor, fallback_logical_size, false)
    }

    fn new_with_fractional(
        scale_factor: i32,
        fallback_logical_size: Option<(i32, i32)>,
        fractional_scale_enabled: bool,
    ) -> Self {
        Self {
            state: LifecycleState::Discovered,
            // wl_output scale is positive. Keep invalid metadata visible but
            // prevent it from becoming a zero physical extent.
            scale_factor: scale_factor.max(1) as u32,
            fallback_logical_size: positive_size(fallback_logical_size),
            fractional_scale_enabled,
            preferred_scale_120: None,
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
        let physical_extent = logical_width
            .zip(logical_height)
            .and_then(|(width, height)| self.physical_extent(width, height));

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
            if !self.fractional_scale_enabled {
                self.recompute_physical_extent();
            }
        }
    }

    fn preferred_scale_changed(&mut self, scale_120: u32) {
        if self.fractional_scale_enabled && scale_120 > 0 {
            self.preferred_scale_120 = Some(scale_120);
            self.recompute_physical_extent();
        }
    }

    fn physical_extent(&self, logical_width: u32, logical_height: u32) -> Option<PhysicalExtent> {
        if self.fractional_scale_enabled {
            let scale_120 = self.preferred_scale_120?;
            Some(PhysicalExtent {
                width: scaled_dimension(logical_width, scale_120)?,
                height: scaled_dimension(logical_height, scale_120)?,
            })
        } else {
            logical_width
                .checked_mul(self.scale_factor)
                .zip(logical_height.checked_mul(self.scale_factor))
                .map(|(width, height)| PhysicalExtent { width, height })
        }
    }

    fn recompute_physical_extent(&mut self) {
        let Some(configure) = self.last_configure else {
            return;
        };
        let physical_extent =
            self.physical_extent(configure.logical_width, configure.logical_height);
        if let Some(current) = &mut self.last_configure {
            current.physical_extent = physical_extent;
        }
        self.state = if physical_extent.is_some() {
            LifecycleState::Ready
        } else {
            LifecycleState::AwaitingConfigure
        };
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

fn scaled_dimension(logical: u32, scale_120: u32) -> Option<u32> {
    let numerator = u64::from(logical).checked_mul(u64::from(scale_120))?;
    u32::try_from(numerator.div_ceil(120))
        .ok()
        .filter(|value| *value > 0)
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
    weather_timer_fired: bool,
    reconfigure_generation: u64,
    power_events: Vec<PowerEvent>,
    render_paused: bool,
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
            weather_timer_fired: false,
            reconfigure_generation: 0,
            power_events: Vec::new(),
            render_paused: false,
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

    fn mark_weather_timer_fired(&mut self) {
        self.weather_timer_fired = true;
    }

    fn mark_power_event(&mut self, event: PowerEvent) {
        self.power_events.push(event);
    }

    fn take_power_events(&mut self) -> Vec<PowerEvent> {
        std::mem::take(&mut self.power_events)
    }

    fn pause_rendering(&mut self) {
        self.render_paused = true;
        self.render_requested = false;
        self.frame_pending = false;
        self.callback_ready = false;
        self.timer_fired = false;
        self.weather_timer_fired = false;
    }

    fn resume_rendering(&mut self) {
        self.render_paused = false;
        self.render_requested = self.lifecycle.state() == LifecycleState::Ready;
        self.frame_pending = false;
        self.callback_ready = false;
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
            if self.render_paused {
                self.frame_pending = false;
                self.callback_ready = false;
                println!("trace frame-callback ignored=true reason=suspended");
                return;
            }
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

/// Optional protocol globals used to turn logical layer sizes into fractional
/// buffer sizes. Integer wl_output scaling remains the fallback when either
/// protocol is unavailable.
struct FractionalGlobals {
    viewporter: Option<SimpleGlobal<WpViewporter, 1>>,
    fractional_scale_manager: Option<SimpleGlobal<WpFractionalScaleManagerV1, 1>>,
}

impl FractionalGlobals {
    fn supported(&self) -> bool {
        self.viewporter.is_some() && self.fractional_scale_manager.is_some()
    }
}

/// S09 keyed multi-output client. It stores only protocol proxies and output
/// state; the owning LayerSurface values remain in `run_multi`, so output GPU
/// presenters can be dropped before those owners during every topology rebuild.
struct MultiOutputRecord {
    output: wl_output::WlOutput,
    info: OutputInfo,
    lifecycle: OutputLifecycle,
    surface_id: Option<u32>,
    frame_pending: bool,
    callback_ready: bool,
    reconfigure_generation: u64,
    viewport: Option<WpViewport>,
    fractional_scale: Option<WpFractionalScaleV1>,
}

struct MultiWaylandClient {
    registry_state: RegistryState,
    output_state: OutputState,
    outputs: BTreeMap<u32, MultiOutputRecord>,
    topology_dirty: bool,
    timer_fired: bool,
    power_events: Vec<PowerEvent>,
    weather_timer_fired: bool,
    ipc_requests: Vec<ControlRequest>,
    render_paused: bool,
    /// Outputs that failed capability validation remain degraded until their
    /// Wayland identity is removed or rediscovered. This prevents a missing
    /// compatible presenter from causing a retry loop while other outputs
    /// continue rendering.
    degraded_outputs: BTreeSet<u32>,
    fractional_globals: FractionalGlobals,
    #[cfg(debug_assertions)]
    withhold_output_callbacks: Option<u32>,
}

impl MultiWaylandClient {
    fn new(
        registry_state: RegistryState,
        output_state: OutputState,
        fractional_globals: FractionalGlobals,
    ) -> Self {
        Self {
            registry_state,
            output_state,
            outputs: BTreeMap::new(),
            topology_dirty: false,
            timer_fired: false,
            power_events: Vec::new(),
            weather_timer_fired: false,
            ipc_requests: Vec::new(),
            render_paused: false,
            degraded_outputs: BTreeSet::new(),
            fractional_globals,
            #[cfg(debug_assertions)]
            withhold_output_callbacks: std::env::var("WORLDWALL_TEST_WITHHOLD_OUTPUT")
                .ok()
                .and_then(|value| value.parse::<u32>().ok()),
        }
    }

    fn output_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.outputs.keys().copied()
    }

    fn missing_outputs(&self) -> Vec<(wl_output::WlOutput, OutputInfo)> {
        let mut outputs = self
            .output_state
            .outputs()
            .filter_map(|output| {
                let info = self.output_state.info(&output)?;
                let record = self.outputs.get(&info.id);
                (record.is_none_or(|record| {
                    record.surface_id.is_none()
                        && !matches!(
                            record.lifecycle.state(),
                            LifecycleState::Removed | LifecycleState::Closing
                        )
                }))
                .then_some((output, info))
            })
            .collect::<Vec<_>>();
        outputs.sort_by(|(_, left), (_, right)| {
            output_sort_key(left.id, left.location, left.name.as_deref()).cmp(&output_sort_key(
                right.id,
                right.location,
                right.name.as_deref(),
            ))
        });
        outputs
    }

    fn install_layer(
        &mut self,
        surface: &LayerSurface,
        output: wl_output::WlOutput,
        info: OutputInfo,
        viewport: Option<WpViewport>,
        fractional_scale: Option<WpFractionalScaleV1>,
    ) {
        let id = info.id;
        let fractional_supported = self.fractional_globals.supported();
        let record = self.outputs.entry(id).or_insert_with(|| MultiOutputRecord {
            output: output.clone(),
            info: info.clone(),
            lifecycle: OutputLifecycle::new_with_fractional(
                info.scale_factor,
                info.logical_size,
                fractional_supported,
            ),
            surface_id: None,
            frame_pending: false,
            callback_ready: false,
            reconfigure_generation: 0,
            viewport: None,
            fractional_scale: None,
        });
        record.output = output;
        record.info = info.clone();
        record.lifecycle = OutputLifecycle::new_with_fractional(
            info.scale_factor,
            info.logical_size,
            fractional_supported,
        );
        record.lifecycle.role_created();
        record.lifecycle.initial_commit();
        record.surface_id = Some(surface.wl_surface().id().protocol_id());
        record.frame_pending = false;
        record.callback_ready = false;
        record.viewport = viewport;
        record.fractional_scale = fractional_scale;
        self.topology_dirty = true;
    }

    fn record_for_surface(&self, surface: &wl_surface::WlSurface) -> Option<u32> {
        let id = surface.id().protocol_id();
        self.outputs
            .iter()
            .find_map(|(output_id, record)| (record.surface_id == Some(id)).then_some(*output_id))
    }

    fn physical_extent(&self, output_id: u32) -> Option<(u32, u32)> {
        self.outputs
            .get(&output_id)
            .and_then(|record| record.lifecycle.last_configure)
            .and_then(|configure| configure.physical_extent)
            .map(|extent| (extent.width, extent.height))
    }

    fn update_viewport(output_id: u32, record: &MultiOutputRecord) {
        let Some(viewport) = record.viewport.as_ref() else {
            return;
        };
        let Some(configure) = record.lifecycle.last_configure else {
            return;
        };
        viewport.set_destination(
            configure.logical_width as i32,
            configure.logical_height as i32,
        );
        if let Some(physical) = configure.physical_extent {
            viewport.set_source(0.0, 0.0, physical.width as f64, physical.height as f64);
            println!(
                "trace viewport-configured output={} source={}x{} destination={}x{}",
                output_id,
                physical.width,
                physical.height,
                configure.logical_width,
                configure.logical_height,
            );
        }
    }

    fn ready_output_ids(&self) -> Vec<u32> {
        self.outputs
            .iter()
            .filter_map(|(id, record)| {
                (record.lifecycle.state() == LifecycleState::Ready).then_some(*id)
            })
            .collect()
    }

    fn frame_pending(&self, output_id: u32) -> bool {
        self.outputs
            .get(&output_id)
            .is_some_and(|record| record.frame_pending)
    }

    fn callback_ready(&mut self, output_id: u32) -> bool {
        self.outputs.get_mut(&output_id).is_some_and(|record| {
            let ready = record.callback_ready;
            record.callback_ready = false;
            ready
        })
    }

    fn request_frame_for_surface(
        &mut self,
        output_id: u32,
        surface: &wl_surface::WlSurface,
        queue_handle: &QueueHandle<Self>,
    ) {
        let Some(record) = self.outputs.get_mut(&output_id) else {
            return;
        };
        if record.surface_id == Some(surface.id().protocol_id()) && !record.frame_pending {
            surface.frame(queue_handle, FrameCallbackData(surface.clone()));
            record.frame_pending = true;
            record.callback_ready = false;
            println!(
                "trace frame-requested output={} before-present=true",
                output_id
            );
        }
    }

    fn topology_dirty(&self) -> bool {
        self.topology_dirty
    }

    fn clear_topology_dirty(&mut self) {
        self.topology_dirty = false;
    }

    fn removed_output_ids(&self) -> Vec<u32> {
        self.outputs
            .iter()
            .filter_map(|(id, record)| {
                matches!(
                    record.lifecycle.state(),
                    LifecycleState::Removed | LifecycleState::Closing
                )
                .then_some(*id)
            })
            .collect()
    }

    fn forget_output(&mut self, output_id: u32) {
        self.outputs.remove(&output_id);
        self.degraded_outputs.remove(&output_id);
    }

    fn mark_timer_fired(&mut self) {
        self.timer_fired = true;
    }

    fn mark_power_event(&mut self, event: PowerEvent) {
        self.power_events.push(event);
    }

    fn mark_weather_timer_fired(&mut self) {
        self.weather_timer_fired = true;
    }

    fn mark_ipc_request(&mut self, request: ControlRequest) {
        self.ipc_requests.push(request);
    }

    fn take_ipc_requests(&mut self) -> Vec<ControlRequest> {
        std::mem::take(&mut self.ipc_requests)
    }

    fn take_power_events(&mut self) -> Vec<PowerEvent> {
        std::mem::take(&mut self.power_events)
    }

    fn pause_rendering(&mut self) {
        self.render_paused = true;
        self.timer_fired = false;
        for record in self.outputs.values_mut() {
            record.frame_pending = false;
            record.callback_ready = false;
        }
    }

    fn resume_rendering(&mut self) {
        self.render_paused = false;
        self.timer_fired = false;
        for record in self.outputs.values_mut() {
            record.frame_pending = false;
            record.callback_ready = false;
        }
    }
}

impl CompositorHandler for MultiWaylandClient {
    fn scale_factor_changed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        if let Some(output_id) = self.record_for_surface(surface) {
            if let Some(record) = self.outputs.get_mut(&output_id) {
                record.lifecycle.scale_changed(new_factor);
                record.reconfigure_generation = record.reconfigure_generation.saturating_add(1);
                println!(
                    "trace output-scale output={} factor={} state={:?}",
                    output_id,
                    new_factor,
                    record.lifecycle.state()
                );
            }
        }
    }

    fn transform_changed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_transform: wl_output::Transform,
    ) {
        if let Some(output_id) = self.record_for_surface(surface) {
            if let Some(record) = self.outputs.get_mut(&output_id) {
                record.info.transform = new_transform;
                record.reconfigure_generation = record.reconfigure_generation.saturating_add(1);
            }
            println!(
                "trace output-transform output={} transform={new_transform:?}",
                output_id
            );
        }
    }

    fn frame(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        time: u32,
    ) {
        let Some(output_id) = self.record_for_surface(surface) else {
            return;
        };
        if let Some(record) = self.outputs.get_mut(&output_id) {
            if matches!(
                record.lifecycle.state(),
                LifecycleState::Closing | LifecycleState::Removed | LifecycleState::Disconnected
            ) {
                return;
            }
            #[cfg(debug_assertions)]
            if self.withhold_output_callbacks == Some(output_id) {
                // Debug-only evidence seam: keep one output's readiness gate
                // closed while other output schedulers continue independently.
                record.frame_pending = true;
                record.callback_ready = false;
                println!(
                    "trace frame-callback output={} withheld=true time={time}",
                    output_id
                );
                return;
            }
            record.frame_pending = false;
            record.callback_ready = true;
            println!(
                "trace frame-callback output={} ready=true time={time}",
                output_id
            );
        }
    }

    fn surface_enter(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if let Some(output_id) = self.record_for_surface(surface) {
            println!(
                "trace surface-enter output={} entered-output={}",
                output_id,
                output.id()
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
        if let Some(output_id) = self.record_for_surface(surface) {
            println!(
                "trace surface-leave output={} left-output={}",
                output_id,
                output.id()
            );
        }
    }
}

impl OutputHandler for MultiWaylandClient {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.topology_dirty = true;
        println!(
            "trace output-added protocol-id={}",
            output.id().protocol_id()
        );
    }

    fn update_output(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let Some(info) = self.output_state.info(&output) else {
            return;
        };
        let id = info.id;
        let fractional_supported = self.fractional_globals.supported();
        let record = self.outputs.entry(id).or_insert_with(|| MultiOutputRecord {
            output: output.clone(),
            info: info.clone(),
            lifecycle: OutputLifecycle::new_with_fractional(
                info.scale_factor,
                info.logical_size,
                fractional_supported,
            ),
            surface_id: None,
            frame_pending: false,
            callback_ready: false,
            reconfigure_generation: 0,
            viewport: None,
            fractional_scale: None,
        });
        record.output = output;
        record.info = info.clone();
        if record.surface_id.is_none() {
            record.lifecycle = OutputLifecycle::new_with_fractional(
                info.scale_factor,
                info.logical_size,
                fractional_supported,
            );
            self.degraded_outputs.remove(&id);
            self.topology_dirty = true;
        }
        println!(
            "trace output-updated id={} logical-size={:?} scale={}",
            id, info.logical_size, info.scale_factor
        );
    }

    fn output_destroyed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if let Some((id, record)) = self
            .outputs
            .iter_mut()
            .find(|(_, record)| record.output == output)
        {
            record.lifecycle.output_removed();
            record.frame_pending = false;
            record.callback_ready = false;
            self.degraded_outputs.remove(id);
            self.topology_dirty = true;
            println!("trace output-removed id={} state=removed", id);
        }
    }
}

impl LayerShellHandler for MultiWaylandClient {
    fn closed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        layer: &LayerSurface,
    ) {
        if let Some(output_id) = self.record_for_surface(layer.wl_surface()) {
            if let Some(record) = self.outputs.get_mut(&output_id) {
                record.lifecycle.close();
                record.frame_pending = false;
                record.callback_ready = false;
                self.topology_dirty = true;
                println!("trace layer-closed output={} state=closing", output_id);
            }
        }
    }

    fn configure(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        serial: u32,
    ) {
        if let Some(output_id) = self.record_for_surface(layer.wl_surface()) {
            if let Some(record) = self.outputs.get_mut(&output_id) {
                record.lifecycle.configured(serial, configure.new_size);
                record.reconfigure_generation = record.reconfigure_generation.saturating_add(1);
                Self::update_viewport(output_id, record);
                let current = record.lifecycle.last_configure;
                println!(
                    "trace configure output={} serial={} suggested={}x{} physical={:?} preferred-scale-120={:?} state={:?} ack=toolkit-before-handler",
                    output_id,
                    serial,
                    configure.new_size.0,
                    configure.new_size.1,
                    current.and_then(|value| value.physical_extent),
                    record.lifecycle.preferred_scale_120,
                    record.lifecycle.state(),
                );
            }
        }
    }
}

wayland_client::delegate_noop!(MultiWaylandClient: WpViewporter);
wayland_client::delegate_noop!(MultiWaylandClient: WpFractionalScaleManagerV1);
wayland_client::delegate_noop!(MultiWaylandClient: WpViewport);

impl Dispatch<WpFractionalScaleV1, u32> for MultiWaylandClient {
    fn event(
        client: &mut Self,
        _proxy: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        output_id: &u32,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else {
            return;
        };
        if let Some(record) = client.outputs.get_mut(output_id) {
            record.lifecycle.preferred_scale_changed(scale);
            record.reconfigure_generation = record.reconfigure_generation.saturating_add(1);
            Self::update_viewport(*output_id, record);
            println!(
                "trace fractional-scale output={} preferred-scale-120={} physical={:?} state={:?}",
                output_id,
                scale,
                record
                    .lifecycle
                    .last_configure
                    .and_then(|configure| configure.physical_extent),
                record.lifecycle.state(),
            );
        }
    }
}

delegate_registry!(MultiWaylandClient);

impl ProvidesRegistryState for MultiWaylandClient {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}

smithay_client_toolkit::delegate_dispatch2!(MultiWaylandClient);

struct CreatedMultiLayer {
    layer: LayerSurface,
    viewport: Option<WpViewport>,
    fractional_scale: Option<WpFractionalScaleV1>,
}

fn create_multi_layer(
    compositor: &CompositorState,
    layer_shell: &LayerShell,
    queue_handle: &QueueHandle<MultiWaylandClient>,
    output: &wl_output::WlOutput,
    output_id: u32,
    fractional_globals: &FractionalGlobals,
) -> Result<CreatedMultiLayer, Box<dyn Error>> {
    let surface = compositor.create_surface(queue_handle);
    let layer = layer_shell.create_layer_surface(
        queue_handle,
        surface,
        Layer::Background,
        Some("worldwall"),
        Some(output),
    );
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_size(0, 0);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    if fractional_globals.supported() {
        // Fractional-scale v1 requires buffer scale 1; wp_viewport maps the
        // physical buffer back to the logical layer destination.
        layer.wl_surface().set_buffer_scale(1);
    }
    let empty_input_region = Region::new(compositor)?;
    layer.set_input_region(Some(empty_input_region.wl_region()));
    let viewport = fractional_globals
        .viewporter
        .as_ref()
        .and_then(|global| global.get().ok())
        .map(|manager| manager.get_viewport(layer.wl_surface(), queue_handle, ()));
    let fractional_scale = fractional_globals
        .fractional_scale_manager
        .as_ref()
        .and_then(|global| global.get().ok())
        .map(|manager| manager.get_fractional_scale(layer.wl_surface(), queue_handle, output_id));
    layer.commit();
    Ok(CreatedMultiLayer {
        layer,
        viewport,
        fractional_scale,
    })
}

fn install_missing_multi_layers(
    client: &mut MultiWaylandClient,
    compositor: &CompositorState,
    layer_shell: &LayerShell,
    queue_handle: &QueueHandle<MultiWaylandClient>,
    layers: &mut Vec<(u32, LayerSurface)>,
) -> Result<(), Box<dyn Error>> {
    for (output, info) in client.missing_outputs() {
        let id = info.id;
        let CreatedMultiLayer {
            layer,
            viewport,
            fractional_scale,
        } = create_multi_layer(
            compositor,
            layer_shell,
            queue_handle,
            &output,
            id,
            &client.fractional_globals,
        )?;
        client.install_layer(&layer, output, info, viewport, fractional_scale);
        layers.push((id, layer));
        println!(
            "trace role-settings output={} layer=background anchors=all size=0x0 keyboard=none input-region=empty exclusive-zone=-1",
            id
        );
    }
    Ok(())
}

fn trace_allocation_counters(counters: AllocationCounters) {
    println!(
        "trace allocation-counters scene-textures={} scene-buffers={} scene-layouts={} scene-upload-bind-groups={} pipelines={} output-uniform-buffers={} output-bind-groups={} output-initial-surface-configurations={}",
        counters.scene_texture_creations,
        counters.scene_buffer_creations,
        counters.scene_bind_group_layout_creations,
        counters.scene_upload_bind_group_creations,
        counters.pipeline_creations,
        counters.output_uniform_buffer_creations,
        counters.output_bind_group_creations,
        counters.output_initial_surface_configurations,
    );
}

fn rebuild_multi_presenters<'wayland>(
    shared: &mut SharedRenderer,
    connection: &'wayland Connection,
    client: &mut MultiWaylandClient,
    layers: &'wayland [(u32, LayerSurface)],
) -> BTreeMap<u32, OutputRenderer<'wayland>> {
    let mut presenters = BTreeMap::new();
    for output_id in client.ready_output_ids() {
        let Some(extent) = client.physical_extent(output_id) else {
            continue;
        };
        let Some((_, layer)) = layers.iter().find(|(id, _)| *id == output_id) else {
            continue;
        };
        if client.degraded_outputs.contains(&output_id) {
            continue;
        }
        match shared.add_output(connection, layer.wl_surface(), extent) {
            Ok(presenter) => {
                println!(
                    "trace output-presenter-added output={} shared-device=true format={:?}",
                    output_id,
                    presenter.format()
                );
                presenters.insert(output_id, presenter);
            }
            Err(error) => {
                eprintln!(
                    "trace output-degraded output={} shared-device=true retry=on-rediscovery reason={error}",
                    output_id
                );
                client.degraded_outputs.insert(output_id);
            }
        }
    }
    presenters
}

fn insert_multi_frame_timer(
    event_loop: &EventLoop<MultiWaylandClient>,
    interval: Duration,
) -> Result<RegistrationToken, Box<dyn Error>> {
    Ok(event_loop
        .handle()
        .insert_source(Timer::from_duration(interval), move |_, _, client| {
            client.mark_timer_fired();
            TimeoutAction::ToDuration(interval)
        })?)
}

fn insert_multi_weather_timer(
    event_loop: &EventLoop<MultiWaylandClient>,
    interval: Duration,
) -> Result<RegistrationToken, Box<dyn Error>> {
    Ok(event_loop
        .handle()
        .insert_source(Timer::from_duration(interval), move |_, _, client| {
            client.mark_weather_timer_fired();
            TimeoutAction::ToDuration(interval)
        })?)
}

fn render_multi_presenter<'wayland>(
    connection: &'wayland Connection,
    presenter: &mut OutputRenderer<'wayland>,
    extent: (u32, u32),
    animation_time_seconds: f32,
    civil_seconds: u32,
) -> Result<RenderOutcome, Box<dyn Error>> {
    presenter.reconfigure(extent)?;
    match presenter.render_at_with_civil_seconds(animation_time_seconds, civil_seconds)? {
        RenderOutcome::Reconfigure => {
            presenter.recreate_surface(connection)?;
            presenter.reconfigure(extent)?;
            presenter.render_at_with_civil_seconds(animation_time_seconds, civil_seconds)
        }
        outcome => Ok(outcome),
    }
}

struct DiagnosticRateLimiter {
    last_emitted: BTreeMap<String, (Instant, u32)>,
    interval: Duration,
}

impl DiagnosticRateLimiter {
    fn new() -> Self {
        Self {
            last_emitted: BTreeMap::new(),
            interval: Duration::from_secs(1),
        }
    }

    fn record(&mut self, code: &str) {
        let now = Instant::now();
        let Some(entry) = self.last_emitted.get_mut(code) else {
            eprintln!("trace ipc-diagnostic code={} suppressed=0", code);
            self.last_emitted.insert(code.to_owned(), (now, 0));
            return;
        };
        if now.duration_since(entry.0) >= self.interval {
            eprintln!("trace ipc-diagnostic code={} suppressed={}", code, entry.1);
            *entry = (now, 0);
        } else {
            entry.1 = entry.1.saturating_add(1);
        }
    }
}

fn record_ipc_failure(diagnostics: &mut DiagnosticRateLimiter, response: &Value) {
    if response.get("ok").and_then(Value::as_bool) == Some(false)
        && let Some(code) = response
            .get("error")
            .and_then(Value::as_object)
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str)
    {
        diagnostics.record(code);
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_ipc_requests<'wayland>(
    client: &mut MultiWaylandClient,
    shared: &mut Option<SharedRenderer>,
    presenters: &mut BTreeMap<u32, OutputRenderer<'wayland>>,
    selection: &mut Option<RegistrySelection>,
    config: &mut AppConfig,
    schedulers: &mut BTreeMap<u32, FrameScheduler>,
    civil_time: &CivilTimeRuntime,
    weather: &WeatherRuntime,
    diagnostics: &mut DiagnosticRateLimiter,
) -> Option<Fps> {
    let mut applied_fps = None;
    for request in client.take_ipc_requests() {
        let command_name = match &request.command {
            Command::Status => "status",
            Command::Reload => "reload",
            Command::ReloadConfig => "reload-config",
            Command::Scene(_) => "scene",
            Command::Next => "next",
            Command::Weather => "weather",
        };
        let command = request.command.clone();
        let response = match command {
            Command::Status => {
                let outputs = client.ready_output_ids();
                ipc::success_response(
                    command_name,
                    json!({
                        "outputs": outputs,
                        "scene": selection
                            .as_ref()
                            .and_then(|value| value.transaction.active().map(|scene| scene.id.as_str()))
                            .or_else(|| selection.as_ref().and_then(|value| value.controller.active_id()))
                            .unwrap_or("synthetic-reference"),
                        "weather": weather.status_value(),
                        "fps": match config.fps { Fps::Fifteen => 15, Fps::Thirty => 30 },
                        "backend": "vulkan",
                        "degraded_outputs": client.degraded_outputs.iter().copied().collect::<Vec<_>>(),
                        "selection_generation": selection.as_ref().map_or(0, |value| value.controller.generation()),
                    }),
                )
            }
            Command::Weather => ipc::success_response(command_name, weather.status_value()),
            Command::ReloadConfig => match config.reload_path().and_then(AppConfig::load) {
                Ok(candidate) => {
                    let registry_changed = candidate.registry_dir != config.registry_dir;
                    let replacement = candidate
                        .registry_dir
                        .as_ref()
                        .map(SceneRegistry::load)
                        .transpose();
                    match replacement {
                        Ok(replacement) if registry_changed => {
                            match replacement {
                                Some(registry) => {
                                    if let Some(active) = selection.as_mut() {
                                        active.replace_registry(registry);
                                    } else {
                                        *selection =
                                            Some(RegistrySelection::from_registry(registry));
                                    }
                                }
                                None => {
                                    // Keep the resident transaction alive while
                                    // making future registry selections
                                    // unavailable. This preserves status for
                                    // the current scene across a path change.
                                    if let Some(active) = selection.as_mut() {
                                        active.disable_registry();
                                    }
                                }
                            }
                            for scheduler in schedulers.values_mut() {
                                scheduler.set_fps(candidate.fps, 0);
                            }
                            let fps = match candidate.fps {
                                Fps::Fifteen => 15,
                                Fps::Thirty => 30,
                            };
                            *config = candidate;
                            applied_fps = Some(config.fps);
                            ipc::success_response(
                                command_name,
                                json!({"applied": true, "reselected": false, "fps": fps}),
                            )
                        }
                        Ok(_) => {
                            for scheduler in schedulers.values_mut() {
                                scheduler.set_fps(candidate.fps, 0);
                            }
                            let fps = match candidate.fps {
                                Fps::Fifteen => 15,
                                Fps::Thirty => 30,
                            };
                            *config = candidate;
                            applied_fps = Some(config.fps);
                            ipc::success_response(
                                command_name,
                                json!({"applied": true, "reselected": false, "fps": fps}),
                            )
                        }
                        Err(error) => ipc::error_response("invalid_config", error.to_string()),
                    }
                }
                Err(error) => ipc::error_response("invalid_config", error.to_string()),
            },
            trigger => {
                let trigger = match trigger {
                    Command::Reload => SelectionTrigger::Reload,
                    Command::Next => SelectionTrigger::Next,
                    Command::Scene(id) => SelectionTrigger::ManualId(id),
                    _ => unreachable!(),
                };
                match (selection.as_mut(), shared.as_mut()) {
                    (None, _) => ipc::error_response(
                        "selection_unavailable",
                        "the active scene has no validated registry selection",
                    ),
                    (_, None) => ipc::error_response(
                        "outputs_unavailable",
                        "no configured output is available",
                    ),
                    (Some(active), Some(_renderer)) if active.controller.registry().is_empty() => {
                        ipc::error_response(
                            "selection_unavailable",
                            "the active scene has no validated registry selection",
                        )
                    }
                    (Some(active), Some(renderer)) => {
                        let snapshot = civil_time.snapshot(weather.weather());
                        let seed = u64::from(civil_time.seconds());
                        match active.begin_explicit_request(snapshot, seed, trigger) {
                            Ok((request, token, manifest)) => {
                                let candidate = manifest.load_scene();
                                match active.complete_loaded_shared(
                                    renderer, presenters, &request, token, candidate,
                                ) {
                                    Ok(outcome) => match outcome {
                                        CommitOutcome::Applied { ref id, .. }
                                        | CommitOutcome::Unchanged { ref id, .. } => {
                                            ipc::success_response(
                                                command_name,
                                                json!({"applied": true, "scene": id, "outcome": format!("{outcome:?}")}),
                                            )
                                        }
                                        CommitOutcome::Failed { ref reason, .. } => {
                                            ipc::error_response("selection_failed", reason.clone())
                                        }
                                        CommitOutcome::Stale { .. } => ipc::error_response(
                                            "stale_request",
                                            "selection generation is stale",
                                        ),
                                    },
                                    Err(error) => {
                                        ipc::error_response("selection_failed", error.to_string())
                                    }
                                }
                            }
                            Err(error) => {
                                ipc::error_response("selection_rejected", error.to_string())
                            }
                        }
                    }
                }
            }
        };
        record_ipc_failure(diagnostics, &response);
        request.respond(response);
    }
    applied_fps
}

#[allow(clippy::too_many_arguments)]
fn run_multi_presenting<'wayland>(
    event_loop: &mut EventLoop<MultiWaylandClient>,
    client: &mut MultiWaylandClient,
    mut presenters: BTreeMap<u32, OutputRenderer<'wayland>>,
    shared: &mut Option<SharedRenderer>,
    selection: &mut Option<RegistrySelection>,
    config: &mut AppConfig,
    connection: &'wayland Connection,
    queue_handle: &QueueHandle<MultiWaylandClient>,
    layers: &'wayland [(u32, LayerSurface)],
    schedulers: &mut BTreeMap<u32, FrameScheduler>,
    seen_generations: &mut BTreeMap<u32, u64>,
    civil_time: &CivilTimeRuntime,
    weather: &mut WeatherRuntime,
    clock_origin: Instant,
    animation_clock: &mut PausedAnimationClock,
    power: &mut PowerStateMachine,
    frame_timer: &mut Option<RegistrationToken>,
    weather_timer: &mut Option<RegistrationToken>,
    fps: Fps,
    shared_scene_uploads: u64,
    interval: Duration,
    #[cfg(debug_assertions)] test_exit_deadline: Option<Instant>,
    diagnostics: &mut DiagnosticRateLimiter,
) -> Result<bool, Box<dyn Error>> {
    let mut fps = fps;
    let mut interval = interval;
    // Bootstrap each newly-created presenter only after its callback request
    // is queued. The next steady-state draw is callback/deadline gated below.
    if !client.render_paused {
        for (&output_id, presenter) in &mut presenters {
            let Some((_, layer)) = layers.iter().find(|(id, _)| *id == output_id) else {
                continue;
            };
            client.request_frame_for_surface(output_id, layer.wl_surface(), queue_handle);
            match presenter.render_at_with_civil_seconds(0.0, civil_time.seconds())? {
                RenderOutcome::Presented => {
                    if let Some(scheduler) = schedulers.get_mut(&output_id) {
                        let _ = scheduler.submit_if_due(0);
                    }
                }
                RenderOutcome::Timeout | RenderOutcome::Occluded => {
                    eprintln!(
                        "trace gpu-frame output={} skipped reason=surface-not-ready bounded=true",
                        output_id
                    );
                }
                RenderOutcome::Reconfigure | RenderOutcome::Fatal => {}
            }
        }
    }

    loop {
        event_loop.dispatch(None, client)?;
        if client.weather_timer_fired {
            client.weather_timer_fired = false;
            weather.poll();
        }
        let now_ns = clock_origin.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        for event in client.take_power_events() {
            match power.on_event(event) {
                PowerAction::Pause { generation } => {
                    client.pause_rendering();
                    animation_clock.pause(now_ns);
                    for scheduler in schedulers.values_mut() {
                        scheduler.pause();
                    }
                    if let Some(token) = frame_timer.take() {
                        event_loop.handle().remove(token);
                    }
                    if let Some(token) = weather_timer.take() {
                        event_loop.handle().remove(token);
                    }
                    println!("trace suspend state=paused generation={generation} deadlines=false");
                }
                PowerAction::BeginResume { generation } => {
                    println!(
                        "trace resume state=pending generation={generation} output-ready={}",
                        !client.ready_output_ids().is_empty()
                    );
                }
                PowerAction::ServiceDegraded => {
                    eprintln!("trace power-service state=degraded resume-support=false");
                }
                PowerAction::ServiceRestored => {
                    println!("trace power-service state=available resume-support=true");
                }
                PowerAction::Ignored => {}
            }
        }
        if let Some(applied_fps) = handle_ipc_requests(
            client,
            shared,
            &mut presenters,
            selection,
            config,
            schedulers,
            civil_time,
            weather,
            diagnostics,
        ) {
            if applied_fps != fps {
                fps = applied_fps;
                interval = Duration::from_nanos(fps.interval_nanos());
                if let Some(token) = frame_timer.take() {
                    event_loop.handle().remove(token);
                }
                if !client.render_paused && !presenters.is_empty() {
                    *frame_timer = Some(insert_multi_frame_timer(event_loop, interval)?);
                }
                println!("trace config-fps-reconfigured fps={:?}", fps);
            }
        }
        if let Some(generation) = power.begin_resume_if_ready(!client.ready_output_ids().is_empty())
        {
            client.resume_rendering();
            animation_clock.resume(now_ns);
            for scheduler in schedulers.values_mut() {
                scheduler.resume();
            }
            if frame_timer.is_none() && !presenters.is_empty() {
                *frame_timer = Some(insert_multi_frame_timer(event_loop, interval)?);
            }
            if weather_timer.is_none() {
                *weather_timer = Some(insert_multi_weather_timer(
                    event_loop,
                    Duration::from_secs(1),
                )?);
            }
            let _ = power.complete_resume(generation);
            println!(
                "trace resume state=active generation={generation} shared-scene-retained=true"
            );
            println!(
                "trace resume-presented generation={generation} output-ready={}",
                !presenters.is_empty()
            );
        }
        // A configure can make a newly-created output ready after this
        // topology pass has already built presenters for the outputs that
        // were ready at its start. Rebuild once for that output, but do not
        // retry an output already placed in the bounded degraded set.
        let missing_ready_presenter = client.ready_output_ids().into_iter().any(|output_id| {
            !presenters.contains_key(&output_id) && !client.degraded_outputs.contains(&output_id)
        });
        if client.topology_dirty() || missing_ready_presenter {
            return Ok(true);
        }
        #[cfg(debug_assertions)]
        if test_exit_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            println!("trace test-exit clean=true reason=deadline");
            return Ok(false);
        }
        if !client.timer_fired {
            continue;
        }
        client.timer_fired = false;
        let now_ns = clock_origin.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        if client.render_paused {
            continue;
        }
        let animation_time_seconds = animation_clock.seconds(now_ns);
        for output_id in client.ready_output_ids() {
            let Some(presenter) = presenters.get_mut(&output_id) else {
                continue;
            };
            if client.callback_ready(output_id) {
                if let Some(scheduler) = schedulers.get_mut(&output_id) {
                    scheduler.on_callback();
                }
            }
            let generation = client
                .outputs
                .get(&output_id)
                .map_or(0, |record| record.reconfigure_generation);
            if seen_generations.get(&output_id).copied() != Some(generation) {
                if let Some(scheduler) = schedulers.get_mut(&output_id) {
                    scheduler.on_reconfigure();
                }
                seen_generations.insert(output_id, generation);
            }
            let Some(extent) = client.physical_extent(output_id) else {
                continue;
            };
            let Some(scheduler) = schedulers.get_mut(&output_id) else {
                continue;
            };
            if client.frame_pending(output_id) || !scheduler.is_due(now_ns) {
                continue;
            }
            let Some((_, layer)) = layers.iter().find(|(id, _)| *id == output_id) else {
                continue;
            };
            client.request_frame_for_surface(output_id, layer.wl_surface(), queue_handle);
            match render_multi_presenter(
                connection,
                presenter,
                extent,
                animation_time_seconds,
                civil_time.seconds(),
            )? {
                RenderOutcome::Presented => {
                    let _ = scheduler.submit_if_due(now_ns);
                    println!(
                        "trace animation-frame output={} fps={} phase-seconds={animation_time_seconds:.6} shared-scene-uploads={} animation-uniform-updates={}",
                        output_id,
                        match fps {
                            Fps::Fifteen => 15,
                            Fps::Thirty => 30,
                        },
                        shared_scene_uploads,
                        presenter.animation_uniform_update_count(),
                    );
                }
                RenderOutcome::Timeout => eprintln!(
                    "trace gpu-frame output={} skipped reason=timeout bounded=true",
                    output_id
                ),
                RenderOutcome::Occluded => eprintln!(
                    "trace gpu-frame output={} skipped reason=occluded bounded=true",
                    output_id
                ),
                RenderOutcome::Reconfigure | RenderOutcome::Fatal => {}
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_multi_session<'owners>(
    event_loop: &mut EventLoop<MultiWaylandClient>,
    client: &mut MultiWaylandClient,
    shared: &mut Option<SharedRenderer>,
    selection: &mut Option<RegistrySelection>,
    config: &mut AppConfig,
    connection: &'owners Connection,
    queue_handle: &QueueHandle<MultiWaylandClient>,
    compositor: &CompositorState,
    layer_shell: &LayerShell,
    layers: &'owners mut Vec<(u32, LayerSurface)>,
    schedulers: &mut BTreeMap<u32, FrameScheduler>,
    seen_generations: &mut BTreeMap<u32, u64>,
    scene: &Scene,
    civil_time: &CivilTimeRuntime,
    weather: &mut WeatherRuntime,
    clock_origin: Instant,
    animation_clock: &mut PausedAnimationClock,
    power: &mut PowerStateMachine,
    _fps: Fps,
    frame_timer: &mut Option<RegistrationToken>,
    weather_timer: &mut Option<RegistrationToken>,
    _interval: Duration,
    #[cfg(debug_assertions)] test_exit_deadline: Option<Instant>,
) -> Result<(), Box<dyn Error>> {
    let mut diagnostics = DiagnosticRateLimiter::new();
    loop {
        let configured_fps = config.fps;
        let configured_interval = Duration::from_nanos(configured_fps.interval_nanos());
        if shared.is_none() {
            if let Some(first_ready) = client.ready_output_ids().into_iter().next() {
                let first_layer = layers
                    .iter()
                    .find(|(id, _)| *id == first_ready)
                    .ok_or_else(|| "ready output has no retained layer owner".to_owned())?;
                *shared = Some(SharedRenderer::new_with_scene(
                    connection,
                    first_layer.1.wl_surface(),
                    scene,
                )?);
                println!("trace outputs-restored shared-device-created=true");
            }
        }
        let presenters = shared
            .as_mut()
            .map(|renderer| rebuild_multi_presenters(renderer, connection, client, layers))
            .unwrap_or_default();
        if let Some(renderer) = shared.as_ref() {
            trace_allocation_counters(renderer.allocation_counters());
        }
        if presenters.is_empty() || client.render_paused {
            if let Some(token) = frame_timer.take() {
                event_loop.handle().remove(token);
            }
            println!("trace outputs-zero frame-wakeup=false dispatch-idle=true");
        } else if frame_timer.is_none() {
            *frame_timer = Some(insert_multi_frame_timer(event_loop, configured_interval)?);
        }
        client.clear_topology_dirty();
        let topology_changed = run_multi_presenting(
            event_loop,
            client,
            presenters,
            shared,
            selection,
            config,
            connection,
            queue_handle,
            layers,
            schedulers,
            seen_generations,
            civil_time,
            weather,
            clock_origin,
            animation_clock,
            power,
            frame_timer,
            weather_timer,
            configured_fps,
            shared
                .as_ref()
                .map_or(0, SharedRenderer::scene_upload_count),
            configured_interval,
            #[cfg(debug_assertions)]
            test_exit_deadline,
            &mut diagnostics,
        )?;
        if !topology_changed {
            return Ok(());
        }

        // `presenters` was moved into run_multi_presenting and dropped there. This
        // is the lifetime barrier before any retained LayerSurface is removed.
        let removed = client.removed_output_ids();
        layers.retain(|(id, _)| !removed.contains(id));
        for id in removed {
            schedulers.remove(&id);
            seen_generations.remove(&id);
            client.forget_output(id);
        }
        install_missing_multi_layers(client, compositor, layer_shell, queue_handle, layers)?;
        for id in client.output_ids() {
            schedulers
                .entry(id)
                .or_insert_with(|| FrameScheduler::new(configured_fps));
            seen_generations.entry(id).or_insert(0);
        }
        continue;
    }
}

/// S09 event loop: every layer, presenter, callback, configuration, and
/// scheduler is keyed by wl_output identity. Output changes only rebuild
/// presentation; the selected scene transaction is never touched.
pub fn run_multi() -> Result<(), Box<dyn Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init(&connection)?;
    let queue_handle = event_queue.handle();
    let compositor = CompositorState::bind(&globals, &queue_handle)
        .map_err(|error| format!("required wl_compositor global is unavailable: {error}"))?;
    let layer_shell = LayerShell::bind(&globals, &queue_handle).map_err(|error| {
        format!("required zwlr_layer_shell_v1 global is unavailable; a layer-shell compositor is required: {error}")
    })?;
    let registry_state = RegistryState::new(&globals);
    let output_state = OutputState::new(&globals, &queue_handle);
    let fractional_globals = FractionalGlobals {
        viewporter: SimpleGlobal::<WpViewporter, 1>::bind(&globals, &queue_handle).ok(),
        fractional_scale_manager: SimpleGlobal::<WpFractionalScaleManagerV1, 1>::bind(
            &globals,
            &queue_handle,
        )
        .ok(),
    };
    println!(
        "trace fractional-scale protocol viewporter={} manager={} enabled={}",
        fractional_globals.viewporter.is_some(),
        fractional_globals.fractional_scale_manager.is_some(),
        fractional_globals.supported(),
    );
    let mut client = MultiWaylandClient::new(registry_state, output_state, fractional_globals);
    event_queue.roundtrip(&mut client)?;
    let mut layers = Vec::new();
    install_missing_multi_layers(
        &mut client,
        &compositor,
        &layer_shell,
        &queue_handle,
        &mut layers,
    )?;
    event_queue.roundtrip(&mut client)?;
    let civil_time = CivilTimeRuntime::startup()?;
    let mut weather = WeatherRuntime::startup()?;
    let mut config = AppConfig::from_environment()?;
    let (scene, mut selection) = load_scene(
        civil_time.snapshot(weather.weather()),
        config.registry_dir.as_deref(),
    )?;
    // A compositor may start with no complete outputs. Keep dispatching with
    // no timer and create the shared device only when the first layer reaches
    // a nonzero configure; a later output arrival reuses this device/scene.
    let mut shared: Option<SharedRenderer> = None;
    let fps = config.fps;
    let mut schedulers = client
        .output_ids()
        .map(|id| (id, FrameScheduler::new(fps)))
        .collect::<BTreeMap<_, _>>();
    let mut seen_generations = client
        .output_ids()
        .map(|id| (id, 0_u64))
        .collect::<BTreeMap<_, _>>();
    let clock_origin = Instant::now();
    #[cfg(debug_assertions)]
    let test_exit_deadline = test_exit_deadline(clock_origin);

    let mut event_loop: EventLoop<MultiWaylandClient> = EventLoop::try_new()?;
    let interval = Duration::from_nanos(fps.interval_nanos());
    WaylandSource::new(connection.clone(), event_queue).insert(event_loop.handle())?;
    let ipc_server = match IpcServer::start_default() {
        Ok((server, source)) => {
            event_loop
                .handle()
                .insert_source(source, |event, _, client| match event {
                    ChannelEvent::Msg(request) => client.mark_ipc_request(request),
                    ChannelEvent::Closed => {}
                })?;
            Some(server)
        }
        Err(error) => {
            eprintln!("trace ipc state=degraded reason={error}");
            None
        }
    };
    let power_events = PowerEventSource::start().into_event_source();
    event_loop
        .handle()
        .insert_source(power_events, move |event, _, client| match event {
            ChannelEvent::Msg(event) => client.mark_power_event(event),
            ChannelEvent::Closed => client.mark_power_event(PowerEvent::Disconnected),
        })?;
    let weather_timer_interval = Duration::from_secs(1);
    let mut weather_timer = Some(insert_multi_weather_timer(
        &event_loop,
        weather_timer_interval,
    )?);
    let mut power = PowerStateMachine::new();
    let mut animation_clock = PausedAnimationClock::new(0);
    let mut frame_timer = None;
    run_multi_session(
        &mut event_loop,
        &mut client,
        &mut shared,
        &mut selection,
        &mut config,
        &connection,
        &queue_handle,
        &compositor,
        &layer_shell,
        &mut layers,
        &mut schedulers,
        &mut seen_generations,
        &scene,
        &civil_time,
        &mut weather,
        clock_origin,
        &mut animation_clock,
        &mut power,
        fps,
        &mut frame_timer,
        &mut weather_timer,
        interval,
        #[cfg(debug_assertions)]
        test_exit_deadline,
    )?;

    drop(event_loop);
    drop(shared);
    drop(ipc_server);
    layers.clear();
    client.outputs.clear();
    println!("trace shutdown multi-output=true");
    Ok(())
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
    fn fractional_scale_waits_for_preferred_scale_and_uses_viewport_extent() {
        let mut lifecycle = OutputLifecycle::new_with_fractional(2, Some((1536, 864)), true);
        lifecycle.role_created();
        lifecycle.initial_commit();
        lifecycle.configured(4, (1536, 864));
        assert_eq!(lifecycle.state(), LifecycleState::AwaitingConfigure);
        assert_eq!(lifecycle.last_configure.unwrap().physical_extent, None);

        lifecycle.preferred_scale_changed(150);
        assert_eq!(lifecycle.state(), LifecycleState::Ready);
        assert_eq!(
            lifecycle.last_configure.unwrap().physical_extent,
            Some(PhysicalExtent {
                width: 1920,
                height: 1080
            })
        );
        assert_eq!(scaled_dimension(101, 125), Some(106));
    }

    #[test]
    fn fractional_preferred_scale_can_arrive_before_configure() {
        let mut lifecycle = OutputLifecycle::new_with_fractional(1, None, true);
        lifecycle.role_created();
        lifecycle.initial_commit();
        lifecycle.preferred_scale_changed(150);
        lifecycle.configured(5, (100, 50));

        assert_eq!(lifecycle.state(), LifecycleState::Ready);
        assert_eq!(
            lifecycle.last_configure.unwrap().physical_extent,
            Some(PhysicalExtent {
                width: 125,
                height: 63
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

    #[test]
    fn ipc_diagnostics_rate_limit_releases_suppressed_count() {
        let mut limiter = DiagnosticRateLimiter::new();
        limiter.record("selection_failed");
        limiter.record("selection_failed");
        assert_eq!(limiter.last_emitted.get("selection_failed").unwrap().1, 1);
        limiter.last_emitted.get_mut("selection_failed").unwrap().0 =
            Instant::now() - Duration::from_secs(2);
        limiter.record("selection_failed");
        assert_eq!(limiter.last_emitted.get("selection_failed").unwrap().1, 0);
    }
}
