//! Minimal Vulkan presentation for the retained S01 Wayland surface.
//!
//! The renderer owns only wgpu resources. The caller owns the Wayland
//! connection and layer surface and must keep them alive for this renderer's
//! lifetime. The raw-handle boundary is intentionally one small function so
//! its validity and drop-order argument can be reviewed independently.
#![allow(dead_code)]

use std::{error::Error, fmt, num::NonZeroU64, ptr::NonNull, sync::Arc};

use crate::scene::Scene;
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use wayland_client::{Connection, Proxy, protocol::wl_surface};

const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.035,
    g: 0.075,
    b: 0.16,
    a: 1.0,
};

#[derive(Debug)]
pub struct Renderer<'wayland> {
    // Field order is deliberate: Rust drops fields in declaration order.
    // GPU resources are released before the surface, and the surface before
    // the instance. The whole renderer is dropped before its Wayland borrow.
    // Scene resources and the indexed pipeline are declared before the
    // presentation surface so they are dropped before it.
    pipeline: wgpu::RenderPipeline,
    solid_pipeline: wgpu::RenderPipeline,
    // One complete scene bundle is swapped as a single owner. Uploading a
    // candidate never mutates this field until every resource and bind group
    // has been created successfully.
    scene: UploadedScene,
    source_extent: (u32, u32),
    queue: wgpu::Queue,
    device: wgpu::Device,
    surface: wgpu::Surface<'wayland>,
    instance: wgpu::Instance,
    format: wgpu::TextureFormat,
    config: wgpu::SurfaceConfiguration,
    wayland_surface: &'wayland wl_surface::WlSurface,
    scene_upload_count: u64,
    animation_uniform_update_count: u64,
    diagnostic_solid_color: bool,
    #[cfg(debug_assertions)]
    test_surface_error: Option<RenderOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOutcome {
    Presented,
    Timeout,
    Occluded,
    Reconfigure,
    Fatal,
}

/// Cumulative renderer resource-creation events for S09 diagnostics.
///
/// These counters describe the application's explicit wgpu resource creation
/// sites; they are not driver allocator or VRAM statistics. They make shared
/// scene reuse and output-local resource growth visible without pretending to
/// measure the backend allocator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AllocationCounters {
    pub scene_texture_creations: u64,
    pub scene_buffer_creations: u64,
    pub scene_bind_group_layout_creations: u64,
    pub scene_upload_bind_group_creations: u64,
    pub pipeline_creations: u64,
    pub output_uniform_buffer_creations: u64,
    pub output_bind_group_creations: u64,
    pub output_initial_surface_configurations: u64,
}

impl AllocationCounters {
    fn record_scene_upload(&mut self, creates_layout: bool) {
        // index texture, palette texture, and the resident time-of-day atlas
        self.scene_texture_creations = self.scene_texture_creations.saturating_add(3);
        // mapping, animation metadata, cycle indices, time-of-day keyframes,
        // and animation clock buffers are created by the upload helper.
        self.scene_buffer_creations = self.scene_buffer_creations.saturating_add(5);
        if creates_layout {
            self.scene_bind_group_layout_creations =
                self.scene_bind_group_layout_creations.saturating_add(1);
        }
        self.scene_upload_bind_group_creations =
            self.scene_upload_bind_group_creations.saturating_add(1);
    }

    fn record_output(&mut self) {
        self.output_uniform_buffer_creations =
            self.output_uniform_buffer_creations.saturating_add(2);
        self.output_bind_group_creations = self.output_bind_group_creations.saturating_add(1);
        self.output_initial_surface_configurations =
            self.output_initial_surface_configurations.saturating_add(1);
    }

    fn record_pipeline_pair(&mut self) {
        self.pipeline_creations = self.pipeline_creations.saturating_add(2);
    }
}

fn classify_surface_texture(acquisition: &wgpu::CurrentSurfaceTexture) -> RenderOutcome {
    match acquisition {
        wgpu::CurrentSurfaceTexture::Success(_) | wgpu::CurrentSurfaceTexture::Suboptimal(_) => {
            RenderOutcome::Presented
        }
        wgpu::CurrentSurfaceTexture::Timeout => RenderOutcome::Timeout,
        wgpu::CurrentSurfaceTexture::Occluded => RenderOutcome::Occluded,
        wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
            RenderOutcome::Reconfigure
        }
        wgpu::CurrentSurfaceTexture::Validation => RenderOutcome::Fatal,
    }
}

#[derive(Debug)]
struct RendererError(String);

impl fmt::Display for RendererError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for RendererError {}

impl<'wayland> Renderer<'wayland> {
    /// Create a Vulkan renderer for a configured, nonzero Wayland layer.
    ///
    /// The adapter is requested against this exact surface. Configuration is
    /// derived from its capabilities; FIFO is required rather than silently
    /// falling back to an unpaced presentation mode.
    pub fn new_with_scene(
        connection: &'wayland Connection,
        wayland_surface: &'wayland wl_surface::WlSurface,
        extent: (u32, u32),
        scene: &Scene,
    ) -> Result<Self, Box<dyn Error>> {
        validate_extent(extent)?;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = create_wayland_surface(&instance, connection, wayland_surface)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))
        .map_err(|error| {
            RendererError(format!(
                "no Vulkan adapter can present on Wayland surface: {error}"
            ))
        })?;

        let info = adapter.get_info();
        println!(
            "trace gpu-adapter backend={:?} name={:?} device={:?}",
            info.backend, info.name, info.device
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("worldwall-device"),
            ..wgpu::DeviceDescriptor::default()
        }))
        .map_err(|error| RendererError(format!("Vulkan device request failed: {error}")))?;
        device.on_uncaptured_error(Arc::new(|error| {
            eprintln!("trace gpu-uncaptured-error error={error:?}");
        }));
        device.set_device_lost_callback(|reason, message| {
            eprintln!("trace gpu-device-lost reason={reason:?} message={message}");
        });

        let capabilities = surface.get_capabilities(&adapter);
        let format = choose_format(&capabilities.formats).ok_or_else(|| {
            RendererError(
                "Wayland surface advertises no supported sRGB render format for indexed scenes"
                    .to_owned(),
            )
        })?;
        if !capabilities
            .present_modes
            .contains(&wgpu::PresentMode::Fifo)
        {
            return Err(Box::new(RendererError(
                "Wayland surface does not advertise required FIFO presentation".to_owned(),
            )));
        }
        let alpha_mode =
            capabilities.alpha_modes.first().copied().ok_or_else(|| {
                RendererError("Wayland surface advertises no alpha mode".to_owned())
            })?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: extent.0,
            height: extent.1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: Vec::new(),
        };
        surface.configure(&device, &config);

        let uploaded = upload_scene(&device, &queue, scene, extent)?;
        let indexed_layout = &uploaded.indexed_layout;
        let indexed_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("worldwall-indexed-scene-shader"),
            source: wgpu::ShaderSource::Wgsl(INDEXED_SCENE_SHADER.into()),
        });
        let indexed_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("worldwall-indexed-scene-layout"),
                bind_group_layouts: &[Some(indexed_layout)],
                immediate_size: 0,
            });
        let pipeline = create_pipeline(
            &device,
            format,
            &indexed_pipeline_layout,
            &indexed_shader,
            "worldwall-indexed-scene-pipeline",
        );

        // Keep S02's solid-color diagnostic available without making it the
        // normal path. It is selected only by an explicit environment flag.
        let solid_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("worldwall-solid-color-shader"),
            source: wgpu::ShaderSource::Wgsl(SOLID_COLOR_SHADER.into()),
        });
        let solid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("worldwall-solid-color-layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let solid_pipeline = create_pipeline(
            &device,
            format,
            &solid_layout,
            &solid_shader,
            "worldwall-solid-color-pipeline",
        );

        println!(
            "trace gpu-surface configured extent={}x{} format={format:?} alpha={alpha_mode:?} present-mode=fifo max-frame-latency=2",
            extent.0, extent.1
        );

        println!(
            "trace scene-upload generation=1 image={}x{} index-bytes={} palette-bytes={} uploads=1",
            scene.image.width,
            scene.image.height,
            scene.image.indices.len(),
            scene.palette.rgba.len()
        );

        Ok(Self {
            instance,
            surface,
            device,
            queue,
            pipeline,
            solid_pipeline,
            scene: uploaded,
            source_extent: (scene.image.width, scene.image.height),
            format,
            config,
            wayland_surface,
            scene_upload_count: 1,
            animation_uniform_update_count: 0,
            diagnostic_solid_color: std::env::var_os("WORLDWALL_SOLID_COLOR").is_some(),
            #[cfg(debug_assertions)]
            test_surface_error: requested_test_surface_error(),
        })
    }

    pub fn reconfigure(&mut self, extent: (u32, u32)) -> Result<(), Box<dyn Error>> {
        validate_extent(extent)?;
        if self.config.width != extent.0 || self.config.height != extent.1 {
            self.config.width = extent.0;
            self.config.height = extent.1;
            self.surface.configure(&self.device, &self.config);
            self.update_mapping(extent);
            println!(
                "trace gpu-surface reconfigured extent={}x{} scene-upload=false",
                extent.0, extent.1
            );
        }
        Ok(())
    }

    /// Upload a complete candidate and atomically make it active.
    ///
    /// This is a stage-local seam until an explicit reload/manual trigger is
    /// wired by the event dispatcher. Call this between frame submissions,
    /// after the scene registry has validated and prepared the candidate. The
    /// old bundle remains active if any resource creation/upload step fails;
    /// assignment is the only point
    /// at which the renderer-visible scene changes. wgpu retains resources
    /// referenced by already-submitted work, so dropping the old bundle here
    /// is safe and does not require blocking the Wayland thread.
    #[allow(dead_code)]
    pub fn upload_and_swap_scene(&mut self, scene: &Scene) -> Result<(), Box<dyn Error>> {
        let candidate = upload_scene_with_layout(
            &self.device,
            &self.queue,
            scene,
            (self.config.width, self.config.height),
            Some(&self.scene.indexed_layout),
        )?;
        self.scene = candidate;
        self.source_extent = (scene.image.width, scene.image.height);
        self.scene_upload_count = self.scene_upload_count.saturating_add(1);
        println!(
            "trace scene-upload swapped=true image={}x{} uploads={}",
            scene.image.width, scene.image.height, self.scene_upload_count
        );
        Ok(())
    }

    /// Render with a bounded monotonic animation phase in seconds.  Only the
    /// 16-byte clock buffer changes per frame; scene textures and metadata are
    /// immutable after upload.
    #[allow(dead_code)]
    pub fn render_at(
        &mut self,
        animation_time_seconds: f32,
    ) -> Result<RenderOutcome, Box<dyn Error>> {
        self.render_at_with_civil_seconds(animation_time_seconds, 0)
    }

    /// Render with independent bounded monotonic animation time and local
    /// civil seconds. The latter only affects an optional scene palette atlas;
    /// it never participates in scene selection.
    pub fn render_at_with_civil_seconds(
        &mut self,
        animation_time_seconds: f32,
        civil_seconds: u32,
    ) -> Result<RenderOutcome, Box<dyn Error>> {
        self.render_at_with_palette_sample(animation_time_seconds, civil_seconds, None)
    }

    pub fn render_at_with_palette_sample(
        &mut self,
        animation_time_seconds: f32,
        civil_seconds: u32,
        solar_sample: Option<crate::scene::ResolvedPaletteSample>,
    ) -> Result<RenderOutcome, Box<dyn Error>> {
        #[cfg(debug_assertions)]
        if let Some(outcome) = self.test_surface_error.take() {
            println!("trace test-surface-error injected={outcome:?}");
            return Ok(outcome);
        }

        self.queue.write_buffer(
            &self.scene.animation_clock_buffer,
            0,
            &animation_clock_bytes(
                animation_time_seconds,
                civil_seconds.min(crate::scene::CIVIL_SECONDS_PER_DAY - 1),
                self.scene.time_of_day_keyframe_count,
                self.scene.time_of_day_enabled,
                solar_sample,
            ),
        );
        self.animation_uniform_update_count = self.animation_uniform_update_count.saturating_add(1);

        let acquisition = self.surface.get_current_texture();
        let acquisition_outcome = classify_surface_texture(&acquisition);
        let frame = match acquisition {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            _ => return Ok(acquisition_outcome),
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("worldwall-solid-color-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("worldwall-solid-color-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if self.diagnostic_solid_color {
                pass.set_pipeline(&self.solid_pipeline);
            } else {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.scene.bind_group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        println!(
            "trace gpu-frame presented extent={}x{} format={:?}",
            self.config.width, self.config.height, self.format
        );
        Ok(RenderOutcome::Presented)
    }

    pub fn scene_upload_count(&self) -> u64 {
        // Keep the texture owners explicit: the bind group references their
        // views, while these fields document the renderer's scene lifetime.
        let _scene_resources_alive = (
            &self.scene.index_texture,
            &self.scene.palette_texture,
            &self.scene.animation_metadata_buffer,
            &self.scene.cycle_indices_buffer,
            &self.scene.time_of_day_atlas,
            &self.scene.time_of_day_keyframes,
        );
        self.scene_upload_count
    }

    pub fn animation_uniform_update_count(&self) -> u64 {
        self.animation_uniform_update_count
    }

    fn update_mapping(&self, extent: (u32, u32)) {
        self.queue.write_buffer(
            &self.scene.mapping_buffer,
            0,
            &mapping_bytes(self.source_extent, extent),
        );
    }

    pub fn recreate_surface(
        &mut self,
        connection: &'wayland Connection,
    ) -> Result<(), Box<dyn Error>> {
        let surface = create_wayland_surface(&self.instance, connection, self.wayland_surface)?;
        self.surface = surface;
        self.surface.configure(&self.device, &self.config);
        println!("trace gpu-surface recreated reason=lost");
        Ok(())
    }
}

/// Device, queue, scene textures, metadata, and pipelines shared by every
/// output presenter. Output-local dimensions never live in this type.
pub struct SharedRenderer {
    scene: SharedScene,
    source_extent: (u32, u32),
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter: wgpu::Adapter,
    instance: wgpu::Instance,
    pipelines: Vec<PipelineSet>,
    scene_upload_count: u64,
    allocation_counters: AllocationCounters,
    diagnostic_solid_color: bool,
}

/// Presentation state for one keyed output. The wgpu handles are cloned from
/// the shared device/queue; the scene textures are not. This makes it possible
/// to rebuild an output binding or surface while the shared scene remains
/// resident, and lets the event loop drop these values before LayerSurface
/// owners during topology changes.
pub struct OutputRenderer<'wayland> {
    pipeline: wgpu::RenderPipeline,
    solid_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    mapping_buffer: wgpu::Buffer,
    animation_clock_buffer: wgpu::Buffer,
    queue: wgpu::Queue,
    device: wgpu::Device,
    surface: wgpu::Surface<'wayland>,
    instance: wgpu::Instance,
    wayland_surface: &'wayland wl_surface::WlSurface,
    format: wgpu::TextureFormat,
    config: wgpu::SurfaceConfiguration,
    source_extent: (u32, u32),
    time_of_day_keyframe_count: u32,
    time_of_day_enabled: bool,
    animation_uniform_update_count: u64,
    diagnostic_solid_color: bool,
    #[cfg(debug_assertions)]
    test_surface_error: Option<RenderOutcome>,
}

struct PipelineSet {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    solid_pipeline: wgpu::RenderPipeline,
}

struct SharedScene {
    index_texture: wgpu::Texture,
    palette_texture: wgpu::Texture,
    mapping_layout: wgpu::BindGroupLayout,
    animation_metadata_buffer: wgpu::Buffer,
    cycle_indices_buffer: wgpu::Buffer,
    time_of_day_atlas: wgpu::Texture,
    time_of_day_keyframes: wgpu::Buffer,
    time_of_day_keyframe_count: u32,
    time_of_day_enabled: bool,
}

impl SharedScene {
    fn from_uploaded(uploaded: UploadedScene) -> Self {
        let UploadedScene {
            index_texture,
            palette_texture,
            animation_metadata_buffer,
            cycle_indices_buffer,
            time_of_day_atlas,
            time_of_day_keyframes,
            time_of_day_keyframe_count,
            time_of_day_enabled,
            indexed_layout,
            ..
        } = uploaded;
        Self {
            index_texture,
            palette_texture,
            mapping_layout: indexed_layout,
            animation_metadata_buffer,
            cycle_indices_buffer,
            time_of_day_atlas,
            time_of_day_keyframes,
            time_of_day_keyframe_count,
            time_of_day_enabled,
        }
    }
}

impl SharedRenderer {
    /// Create one Vulkan device/queue and upload the scene once. The supplied
    /// surface is used only to choose the adapter and validate the first
    /// output's presentation matrix; actual presenters are added separately.
    pub fn new_with_scene(
        connection: &'_ Connection,
        wayland_surface: &'_ wl_surface::WlSurface,
        scene: &Scene,
    ) -> Result<Self, Box<dyn Error>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let bootstrap_surface = create_wayland_surface(&instance, connection, wayland_surface)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&bootstrap_surface),
            apply_limit_buckets: false,
        }))
        .map_err(|error| {
            RendererError(format!(
                "no Vulkan adapter can present on Wayland surface: {error}"
            ))
        })?;
        let info = adapter.get_info();
        println!(
            "trace gpu-shared-adapter backend={:?} name={:?} device={:?}",
            info.backend, info.name, info.device
        );
        let first_capabilities = bootstrap_surface.get_capabilities(&adapter);
        validate_output_capabilities(&first_capabilities)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("worldwall-shared-device"),
            ..wgpu::DeviceDescriptor::default()
        }))
        .map_err(|error| RendererError(format!("Vulkan device request failed: {error}")))?;
        device.on_uncaptured_error(Arc::new(|error| {
            eprintln!("trace gpu-uncaptured-error error={error:?}");
        }));
        device.set_device_lost_callback(|reason, message| {
            eprintln!("trace gpu-device-lost reason={reason:?} message={message}");
        });

        let uploaded = upload_scene(&device, &queue, scene, (1, 1))?;
        let shared_scene = SharedScene::from_uploaded(uploaded);
        let mut renderer = Self {
            scene: shared_scene,
            source_extent: (scene.image.width, scene.image.height),
            device,
            queue,
            adapter,
            instance,
            pipelines: Vec::new(),
            scene_upload_count: 1,
            allocation_counters: AllocationCounters::default(),
            diagnostic_solid_color: std::env::var_os("WORLDWALL_SOLID_COLOR").is_some(),
        };
        renderer.allocation_counters.record_scene_upload(true);
        let format = choose_format(&first_capabilities.formats).ok_or_else(|| {
            RendererError(
                "Wayland surface advertises no supported sRGB render format for indexed scenes"
                    .to_owned(),
            )
        })?;
        renderer.ensure_pipeline(format);
        println!(
            "trace gpu-shared-scene image={}x{} index-bytes={} palette-bytes={} uploads=1",
            scene.image.width,
            scene.image.height,
            scene.image.indices.len(),
            scene.palette.rgba.len()
        );
        Ok(renderer)
    }

    /// Validate a new output against the existing adapter and prepare its
    /// format-specific pipeline without creating a second device.
    pub fn ensure_output_format(
        &mut self,
        connection: &Connection,
        wayland_surface: &wl_surface::WlSurface,
    ) -> Result<wgpu::TextureFormat, Box<dyn Error>> {
        let surface = create_wayland_surface(&self.instance, connection, wayland_surface)?;
        let capabilities = surface.get_capabilities(&self.adapter);
        validate_output_capabilities(&capabilities)?;
        let format = choose_format(&capabilities.formats).ok_or_else(|| {
            RendererError(format!(
                "output {} has no compatible sRGB format; retained shared device",
                wayland_surface.id()
            ))
        })?;
        self.ensure_pipeline(format);
        Ok(format)
    }

    /// Add an output after `ensure_output_format` has validated it. The
    /// returned presenter owns only output-local GPU state and the borrowed
    /// Wayland surface; scene textures stay in `SharedRenderer`.
    pub fn add_output<'wayland>(
        &mut self,
        connection: &'wayland Connection,
        wayland_surface: &'wayland wl_surface::WlSurface,
        extent: (u32, u32),
    ) -> Result<OutputRenderer<'wayland>, Box<dyn Error>> {
        validate_extent(extent)?;
        let surface = create_wayland_surface(&self.instance, connection, wayland_surface)?;
        let capabilities = surface.get_capabilities(&self.adapter);
        validate_output_capabilities(&capabilities)?;
        let format = choose_format(&capabilities.formats).ok_or_else(|| {
            RendererError("output has no compatible sRGB render format".to_owned())
        })?;
        self.ensure_pipeline(format);
        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .ok_or_else(|| RendererError("output advertises no alpha mode".to_owned()))?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: extent.0,
            height: extent.1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: Vec::new(),
        };
        surface.configure(&self.device, &config);
        self.allocation_counters.record_output();
        let (pipeline, solid_pipeline) = self
            .pipelines
            .iter()
            .find(|pipeline| pipeline.format == format)
            .map(|pipeline| (pipeline.pipeline.clone(), pipeline.solid_pipeline.clone()))
            .ok_or_else(|| RendererError("output pipeline was not prepared".to_owned()))?;
        let mapping_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("worldwall-output-cover-mapping"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(
            &mapping_buffer,
            0,
            &mapping_bytes(self.source_extent, extent),
        );
        let animation_clock_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("worldwall-output-animation-clock"),
            size: 32,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(
            &animation_clock_buffer,
            0,
            &animation_clock_bytes(
                0.0,
                0,
                self.scene.time_of_day_keyframe_count,
                self.scene.time_of_day_enabled,
                None,
            ),
        );
        let bind_group = create_output_bind_group(
            &self.device,
            &self.scene,
            &mapping_buffer,
            &animation_clock_buffer,
        );
        Ok(OutputRenderer {
            pipeline,
            solid_pipeline,
            bind_group,
            mapping_buffer,
            animation_clock_buffer,
            queue: self.queue.clone(),
            device: self.device.clone(),
            surface,
            instance: self.instance.clone(),
            wayland_surface,
            format,
            config,
            source_extent: self.source_extent,
            time_of_day_keyframe_count: self.scene.time_of_day_keyframe_count,
            time_of_day_enabled: self.scene.time_of_day_enabled,
            animation_uniform_update_count: 0,
            diagnostic_solid_color: self.diagnostic_solid_color,
            #[cfg(debug_assertions)]
            test_surface_error: requested_test_surface_error(),
        })
    }

    /// Replace only the shared scene bundle. Callers must refresh each live
    /// output binding immediately after this succeeds, before its next draw.
    pub fn upload_and_swap_scene(&mut self, scene: &Scene) -> Result<(), Box<dyn Error>> {
        let uploaded = upload_scene_with_layout(
            &self.device,
            &self.queue,
            scene,
            (1, 1),
            Some(&self.scene.mapping_layout),
        )?;
        self.scene = SharedScene::from_uploaded(uploaded);
        self.source_extent = (scene.image.width, scene.image.height);
        self.scene_upload_count = self.scene_upload_count.saturating_add(1);
        self.allocation_counters.record_scene_upload(false);
        println!(
            "trace scene-upload shared=true image={}x{} uploads={}",
            scene.image.width, scene.image.height, self.scene_upload_count
        );
        Ok(())
    }

    pub fn scene_upload_count(&self) -> u64 {
        self.scene_upload_count
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    fn ensure_pipeline(&mut self, format: wgpu::TextureFormat) {
        if self
            .pipelines
            .iter()
            .any(|pipeline| pipeline.format == format)
        {
            return;
        }
        let indexed_shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("worldwall-shared-indexed-scene-shader"),
                source: wgpu::ShaderSource::Wgsl(INDEXED_SCENE_SHADER.into()),
            });
        let indexed_pipeline_layout =
            self.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("worldwall-shared-indexed-scene-layout"),
                    bind_group_layouts: &[Some(&self.scene.mapping_layout)],
                    immediate_size: 0,
                });
        let pipeline = create_pipeline(
            &self.device,
            format,
            &indexed_pipeline_layout,
            &indexed_shader,
            "worldwall-shared-indexed-scene-pipeline",
        );
        let solid_shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("worldwall-shared-solid-color-shader"),
                source: wgpu::ShaderSource::Wgsl(SOLID_COLOR_SHADER.into()),
            });
        let solid_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("worldwall-shared-solid-color-layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            });
        let solid_pipeline = create_pipeline(
            &self.device,
            format,
            &solid_layout,
            &solid_shader,
            "worldwall-shared-solid-color-pipeline",
        );
        self.pipelines.push(PipelineSet {
            format,
            pipeline,
            solid_pipeline,
        });
        self.allocation_counters.record_pipeline_pair();
    }

    pub fn allocation_counters(&self) -> AllocationCounters {
        self.allocation_counters
    }
}

impl<'wayland> OutputRenderer<'wayland> {
    /// Rebuild only this output's bind group after a shared scene swap.
    pub fn refresh_scene_binding(&mut self, shared: &mut SharedRenderer) {
        self.bind_group = create_output_bind_group(
            &self.device,
            &shared.scene,
            &self.mapping_buffer,
            &self.animation_clock_buffer,
        );
        shared.allocation_counters.output_bind_group_creations = shared
            .allocation_counters
            .output_bind_group_creations
            .saturating_add(1);
        self.source_extent = shared.source_extent;
        self.time_of_day_keyframe_count = shared.scene.time_of_day_keyframe_count;
        self.time_of_day_enabled = shared.scene.time_of_day_enabled;
        self.queue.write_buffer(
            &self.mapping_buffer,
            0,
            &mapping_bytes(self.source_extent, (self.config.width, self.config.height)),
        );
    }

    pub fn reconfigure(&mut self, extent: (u32, u32)) -> Result<(), Box<dyn Error>> {
        validate_extent(extent)?;
        if self.config.width != extent.0 || self.config.height != extent.1 {
            self.config.width = extent.0;
            self.config.height = extent.1;
            self.surface.configure(&self.device, &self.config);
            self.queue.write_buffer(
                &self.mapping_buffer,
                0,
                &mapping_bytes(self.source_extent, extent),
            );
            println!(
                "trace gpu-output reconfigured extent={}x{} scene-upload=false",
                extent.0, extent.1
            );
        }
        Ok(())
    }

    pub fn render_at_with_civil_seconds(
        &mut self,
        animation_time_seconds: f32,
        civil_seconds: u32,
    ) -> Result<RenderOutcome, Box<dyn Error>> {
        self.render_at_with_palette_sample(animation_time_seconds, civil_seconds, None)
    }

    pub fn render_at_with_palette_sample(
        &mut self,
        animation_time_seconds: f32,
        civil_seconds: u32,
        solar_sample: Option<crate::scene::ResolvedPaletteSample>,
    ) -> Result<RenderOutcome, Box<dyn Error>> {
        #[cfg(debug_assertions)]
        if let Some(outcome) = self.test_surface_error.take() {
            println!("trace test-output-surface-error injected={outcome:?}");
            return Ok(outcome);
        }
        self.queue.write_buffer(
            &self.animation_clock_buffer,
            0,
            &animation_clock_bytes(
                animation_time_seconds,
                civil_seconds.min(crate::scene::CIVIL_SECONDS_PER_DAY - 1),
                self.time_of_day_keyframe_count,
                self.time_of_day_enabled,
                solar_sample,
            ),
        );
        self.animation_uniform_update_count = self.animation_uniform_update_count.saturating_add(1);
        let acquisition = self.surface.get_current_texture();
        let acquisition_outcome = classify_surface_texture(&acquisition);
        let frame = match acquisition {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            _ => return Ok(acquisition_outcome),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("worldwall-output-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("worldwall-output-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if self.diagnostic_solid_color {
                pass.set_pipeline(&self.solid_pipeline);
            } else {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        println!(
            "trace gpu-output-frame presented extent={}x{} format={:?}",
            self.config.width, self.config.height, self.format
        );
        println!(
            "trace gpu-frame presented output-format={:?} extent={}x{}",
            self.format, self.config.width, self.config.height
        );
        Ok(RenderOutcome::Presented)
    }

    pub fn animation_uniform_update_count(&self) -> u64 {
        self.animation_uniform_update_count
    }

    pub fn recreate_surface(
        &mut self,
        connection: &'wayland Connection,
    ) -> Result<(), Box<dyn Error>> {
        let surface = create_wayland_surface(&self.instance, connection, self.wayland_surface)?;
        self.surface = surface;
        self.surface.configure(&self.device, &self.config);
        println!("trace gpu-output-surface recreated reason=lost");
        Ok(())
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
}

fn create_output_bind_group(
    device: &wgpu::Device,
    scene: &SharedScene,
    mapping_buffer: &wgpu::Buffer,
    animation_clock_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let index_view = scene
        .index_texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let palette_view = scene
        .palette_texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let time_of_day_view = scene
        .time_of_day_atlas
        .create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("worldwall-output-indexed-bind-group"),
        layout: &scene.mapping_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&index_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&palette_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: mapping_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: scene.animation_metadata_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: scene.cycle_indices_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: animation_clock_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(&time_of_day_view),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: scene.time_of_day_keyframes.as_entire_binding(),
            },
        ],
    })
}

fn validate_output_capabilities(
    capabilities: &wgpu::SurfaceCapabilities,
) -> Result<(), Box<dyn Error>> {
    if choose_format(&capabilities.formats).is_none() {
        return Err(Box::new(RendererError(
            "output advertises no supported sRGB render format; output is degraded without a second device"
                .to_owned(),
        )));
    }
    if !capabilities
        .present_modes
        .contains(&wgpu::PresentMode::Fifo)
    {
        return Err(Box::new(RendererError(
            "output does not advertise required FIFO presentation; output is degraded without a second device"
                .to_owned(),
        )));
    }
    if capabilities.alpha_modes.is_empty() {
        return Err(Box::new(RendererError(
            "output advertises no alpha mode; output is degraded without a second device"
                .to_owned(),
        )));
    }
    Ok(())
}

fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    label: &'static str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn mapping_bytes(source: (u32, u32), output: (u32, u32)) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&source.0.to_ne_bytes());
    bytes[4..8].copy_from_slice(&source.1.to_ne_bytes());
    bytes[8..12].copy_from_slice(&output.0.to_ne_bytes());
    bytes[12..16].copy_from_slice(&output.1.to_ne_bytes());
    bytes
}

fn animation_clock_bytes(
    seconds: f32,
    civil_seconds: u32,
    keyframe_count: u32,
    enabled: bool,
    solar_sample: Option<crate::scene::ResolvedPaletteSample>,
) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[0..4].copy_from_slice(&seconds.to_ne_bytes());
    bytes[4..8].copy_from_slice(&(civil_seconds as f32).to_ne_bytes());
    bytes[8..12].copy_from_slice(&(keyframe_count as f32).to_ne_bytes());
    bytes[12..16].copy_from_slice(&(if enabled { 1.0 } else { 0.0_f32 }).to_ne_bytes());
    let (lower_row, upper_row, amount, mode): (f32, f32, f32, f32) = match solar_sample {
        None => (0.0, 0.0, 0.0, 0.0),
        Some(crate::scene::ResolvedPaletteSample::Base) => (0.0, 0.0, 0.0, 2.0),
        Some(crate::scene::ResolvedPaletteSample::Rows {
            lower_row,
            upper_row,
            amount,
        }) => (lower_row as f32, upper_row as f32, amount, 1.0),
    };
    bytes[16..20].copy_from_slice(&lower_row.to_ne_bytes());
    bytes[20..24].copy_from_slice(&upper_row.to_ne_bytes());
    bytes[24..28].copy_from_slice(&amount.to_ne_bytes());
    bytes[28..32].copy_from_slice(&mode.to_ne_bytes());
    bytes
}

#[derive(Debug)]
struct UploadedScene {
    index_texture: wgpu::Texture,
    palette_texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    mapping_buffer: wgpu::Buffer,
    animation_metadata_buffer: wgpu::Buffer,
    cycle_indices_buffer: wgpu::Buffer,
    time_of_day_atlas: wgpu::Texture,
    time_of_day_keyframes: wgpu::Buffer,
    time_of_day_keyframe_count: u32,
    time_of_day_enabled: bool,
    animation_clock_buffer: wgpu::Buffer,
    indexed_layout: wgpu::BindGroupLayout,
}

fn upload_scene(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &Scene,
    output_extent: (u32, u32),
) -> Result<UploadedScene, Box<dyn Error>> {
    upload_scene_with_layout(device, queue, scene, output_extent, None)
}

fn upload_scene_with_layout(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &Scene,
    output_extent: (u32, u32),
    existing_layout: Option<&wgpu::BindGroupLayout>,
) -> Result<UploadedScene, Box<dyn Error>> {
    // Keep the CPU reference mapping beside the shader mapping. Numeric
    // vectors in scene tests exercise this same centered-cover contract.
    let _reference_corner = crate::scene::cover_source_coord(
        (scene.image.width, scene.image.height),
        output_extent,
        (0, 0),
    );

    let index_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("worldwall-index-texture"),
        size: wgpu::Extent3d {
            width: scene.image.width,
            height: scene.image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Uint,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &index_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &scene.image.indices,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            // write_texture accepts tightly packed rows; unlike a buffer copy,
            // this path does not require 256-byte row padding.
            bytes_per_row: Some(scene.image.width),
            rows_per_image: Some(scene.image.height),
        },
        wgpu::Extent3d {
            width: scene.image.width,
            height: scene.image.height,
            depth_or_array_layers: 1,
        },
    );

    let palette_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("worldwall-authoritative-palette"),
        size: wgpu::Extent3d {
            width: 256,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // Palette bytes are authored sRGB. The shader performs the explicit
        // sRGB-to-linear conversion before the sRGB surface encodes output.
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &palette_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &scene.palette.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256 * 4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 256,
            height: 1,
            depth_or_array_layers: 1,
        },
    );

    let mapping_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("worldwall-cover-mapping-uniform"),
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    queue.write_buffer(
        &mapping_buffer,
        0,
        &mapping_bytes((scene.image.width, scene.image.height), output_extent),
    );

    let metadata_bytes = scene.animation.metadata_bytes();
    let animation_metadata_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("worldwall-animation-metadata"),
        size: metadata_bytes.len() as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    queue.write_buffer(&animation_metadata_buffer, 0, &metadata_bytes);

    let cycle_index_bytes = scene.animation.cycle_index_bytes();
    let cycle_indices_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("worldwall-palette-cycle-indices"),
        size: cycle_index_bytes.len() as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    queue.write_buffer(&cycle_indices_buffer, 0, &cycle_index_bytes);

    let (
        time_of_day_rows,
        time_of_day_rgba,
        time_of_day_keyframe_bytes,
        time_of_day_keyframe_count,
        time_of_day_enabled,
    ) = match scene.time_of_day.as_ref() {
        Some(timeline) => (
            timeline.rows,
            timeline.rgba.clone(),
            timeline.keyframe_bytes(),
            timeline.keyframe_count() as u32,
            true,
        ),
        None => (1, scene.palette.rgba.to_vec(), vec![0u8; 32], 0, false),
    };
    let time_of_day_atlas = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("worldwall-time-of-day-atlas"),
        size: wgpu::Extent3d {
            width: 256,
            height: time_of_day_rows,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &time_of_day_atlas,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &time_of_day_rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256 * 4),
            rows_per_image: Some(time_of_day_rows),
        },
        wgpu::Extent3d {
            width: 256,
            height: time_of_day_rows,
            depth_or_array_layers: 1,
        },
    );
    let time_of_day_keyframes = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("worldwall-time-of-day-keyframes"),
        size: time_of_day_keyframe_bytes.len() as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    queue.write_buffer(&time_of_day_keyframes, 0, &time_of_day_keyframe_bytes);

    let animation_clock_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("worldwall-animation-clock"),
        size: 32,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    queue.write_buffer(
        &animation_clock_buffer,
        0,
        &animation_clock_bytes(
            0.0,
            0,
            time_of_day_keyframe_count,
            time_of_day_enabled,
            None,
        ),
    );

    // Reuse the renderer's layout during a replacement. A standalone upload
    // creates its own layout so the readback tests can construct a pipeline
    // from the returned bundle.
    let layout = existing_layout.cloned().unwrap_or_else(|| {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("worldwall-indexed-bindings-upload"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(16),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(256 * 32),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(256 * 4),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(32),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(32),
                    },
                    count: None,
                },
            ],
        })
    });
    let index_view = index_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let palette_view = palette_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let time_of_day_view = time_of_day_atlas.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("worldwall-indexed-bind-group-upload"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&index_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&palette_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: mapping_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: animation_metadata_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: cycle_indices_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: animation_clock_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(&time_of_day_view),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: time_of_day_keyframes.as_entire_binding(),
            },
        ],
    });
    Ok(UploadedScene {
        index_texture,
        palette_texture,
        bind_group,
        mapping_buffer,
        animation_metadata_buffer,
        cycle_indices_buffer,
        time_of_day_atlas,
        time_of_day_keyframes,
        time_of_day_keyframe_count,
        time_of_day_enabled,
        animation_clock_buffer,
        indexed_layout: layout,
    })
}

/// Debug-only, one-shot acquisition seam for the S02 fault-policy review.
/// It is absent from release builds and never changes the default wgpu path.
#[cfg(debug_assertions)]
fn requested_test_surface_error() -> Option<RenderOutcome> {
    match std::env::var("WORLDWALL_TEST_SURFACE_ERROR").ok()?.as_str() {
        "timeout" => Some(RenderOutcome::Timeout),
        "occluded" => Some(RenderOutcome::Occluded),
        "outdated" => Some(RenderOutcome::Reconfigure),
        "lost" => Some(RenderOutcome::Reconfigure),
        "validation" => Some(RenderOutcome::Fatal),
        _ => None,
    }
}

fn validate_extent(extent: (u32, u32)) -> Result<(), Box<dyn Error>> {
    if extent.0 == 0 || extent.1 == 0 {
        return Err(Box::new(RendererError(format!(
            "refusing zero GPU extent {}x{}",
            extent.0, extent.1
        ))));
    }
    Ok(())
}

fn choose_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    formats.iter().copied().find(|format| {
        matches!(
            format,
            wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Rgba8UnormSrgb
        )
    })
}

/// Construct a surface from handles borrowed from the retained SCTK owners.
///
/// # Safety argument
///
/// `Connection::backend().display_ptr()` is the display connected by SCTK and
/// remains valid while `connection` is alive. `WlSurface::id().as_ptr()` is the
/// proxy for the retained layer surface and remains valid while the caller's
/// `LayerSurface` owner is alive. The returned lifetime is tied to both borrows;
/// the caller therefore cannot drop either Wayland owner before this renderer.
/// All GPU and Wayland calls stay on the event-loop thread. The renderer is
/// dropped before those owners, so wgpu cannot access either raw handle during
/// or after owner teardown.
#[allow(unsafe_code)]
fn create_wayland_surface<'wayland>(
    instance: &wgpu::Instance,
    connection: &'wayland Connection,
    wayland_surface: &'wayland wl_surface::WlSurface,
) -> Result<wgpu::Surface<'wayland>, Box<dyn Error>> {
    let display = NonNull::new(connection.backend().display_ptr().cast()).ok_or_else(|| {
        RendererError("Wayland backend returned a null display handle".to_owned())
    })?;
    let window = NonNull::new(wayland_surface.id().as_ptr().cast()).ok_or_else(|| {
        RendererError("Wayland surface proxy no longer has a live raw handle".to_owned())
    })?;
    let raw_display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display));
    let raw_window = RawWindowHandle::Wayland(WaylandWindowHandle::new(window));

    // SAFETY: The raw handles are valid above, and both their owners are
    // borrowed for exactly the returned Surface lifetime. See the function
    // safety argument and Renderer drop ordering.
    unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display),
                raw_window_handle: raw_window,
            })
            .map_err(|error| Box::new(error) as Box<dyn Error>)
    }
}

const INDEXED_SCENE_SHADER: &str = r#"
struct CoverMapping {
    source_size: vec2<u32>,
    output_size: vec2<u32>,
};

@group(0) @binding(0) var scene_indices: texture_2d<u32>;
@group(0) @binding(1) var palette: texture_2d<f32>;
@group(0) @binding(2) var<uniform> mapping: CoverMapping;

struct PaletteMetadata {
    effect: u32,
    group_offset: u32,
    member_count: u32,
    ordinal: u32,
    speed: f32,
    amplitude: f32,
    phase: f32,
    reserved: f32,
};

struct AnimationClock {
    values: vec4f,
    solar: vec4f,
};

@group(0) @binding(3) var<storage, read> animation_metadata: array<PaletteMetadata>;
@group(0) @binding(4) var<storage, read> cycle_indices: array<u32>;
@group(0) @binding(5) var<uniform> animation_clock: AnimationClock;

struct TimeOfDayKeyframe {
    civil_seconds: u32,
    row: u32,
    reserved_a: u32,
    reserved_b: u32,
};

@group(0) @binding(6) var time_of_day_atlas: texture_2d<f32>;
@group(0) @binding(7) var<storage, read> time_of_day_keyframes: array<TimeOfDayKeyframe>;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4f {
    let positions = array<vec2f, 3>(
        vec2f(-1.0, -1.0),
        vec2f( 3.0, -1.0),
        vec2f(-1.0,  3.0),
    );
    return vec4f(positions[vertex_index], 0.0, 1.0);
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

fn palette_linear(index: u32) -> vec3f {
    let authored = textureLoad(palette, vec2<i32>(i32(index), 0), 0).rgb;
    return vec3f(
        srgb_to_linear(authored.r),
        srgb_to_linear(authored.g),
        srgb_to_linear(authored.b),
    );
}

fn palette_bytes(index: u32) -> vec3f {
    // The palette texture is authoritative RGBA8. Reconstruct the authored
    // bytes before BlendShift interpolation; the half-unit compensates for
    // normalized texture sampling.
    return floor(textureLoad(palette, vec2<i32>(i32(index), 0), 0).rgb * 255.0 + vec3f(0.5));
}

fn time_of_day_bytes(index: u32) -> vec3f {
    if animation_clock.values.w < 0.5 {
        return palette_bytes(index);
    }
    let count = u32(animation_clock.values.z);
    if count < 2u {
        return palette_bytes(index);
    }
    if animation_clock.solar.w > 1.5 {
        return palette_bytes(index);
    }
    if animation_clock.solar.w > 0.5 {
        let lower_color = floor(textureLoad(
            time_of_day_atlas,
            vec2<i32>(i32(index), i32(u32(animation_clock.solar.x))),
            0,
        ).rgb * 255.0 + vec3f(0.5));
        let upper_color = floor(textureLoad(
            time_of_day_atlas,
            vec2<i32>(i32(index), i32(u32(animation_clock.solar.y))),
            0,
        ).rgb * 255.0 + vec3f(0.5));
        return floor(lower_color + (upper_color - lower_color) * vec3f(animation_clock.solar.z));
    }
    let civil = min(animation_clock.values.y, 86399.0);
    let civil_u = u32(civil);
    var lower = count - 1u;
    for (var i = 0u; i < 256u; i = i + 1u) {
        if i >= count {
            break;
        }
        if time_of_day_keyframes[i].civil_seconds <= civil_u {
            lower = i;
        }
    }
    let upper = (lower + 1u) % count;
    let lower_seconds = time_of_day_keyframes[lower].civil_seconds;
    var upper_seconds = time_of_day_keyframes[upper].civil_seconds;
    var sample_seconds = civil_u;
    if upper_seconds <= lower_seconds {
        upper_seconds = upper_seconds + 86400u;
    }
    if sample_seconds < lower_seconds {
        sample_seconds = sample_seconds + 86400u;
    }
    let duration = f32(upper_seconds - lower_seconds);
    let amount = clamp(f32(sample_seconds - lower_seconds) / duration, 0.0, 1.0);
    let lower_color = floor(textureLoad(
        time_of_day_atlas,
        vec2<i32>(i32(index), i32(time_of_day_keyframes[lower].row)),
        0,
    ).rgb * 255.0 + vec3f(0.5));
    let upper_color = floor(textureLoad(
        time_of_day_atlas,
        vec2<i32>(i32(index), i32(time_of_day_keyframes[upper].row)),
        0,
    ).rgb * 255.0 + vec3f(0.5));
    return floor(lower_color + (upper_color - lower_color) * vec3f(amount));
}

fn palette_at_time(index: u32) -> vec3f {
    if animation_clock.values.w < 0.5 {
        return palette_linear(index);
    }
    return byte_space_to_linear(time_of_day_bytes(index));
}

fn byte_space_to_linear(color: vec3f) -> vec3f {
    let normalized = color / vec3f(255.0);
    return vec3f(
        srgb_to_linear(normalized.r),
        srgb_to_linear(normalized.g),
        srgb_to_linear(normalized.b),
    );
}

fn rgb_to_hsv(color: vec3f) -> vec3f {
    let maximum = max(color.r, max(color.g, color.b));
    let minimum = min(color.r, min(color.g, color.b));
    let delta = maximum - minimum;
    var hue = 0.0;
    if delta != 0.0 {
        if maximum == color.r {
            hue = ((color.g - color.b) / delta / 6.0);
        } else if maximum == color.g {
            hue = ((color.b - color.r) / delta + 2.0) / 6.0;
        } else {
            hue = ((color.r - color.g) / delta + 4.0) / 6.0;
        }
    }
    if hue < 0.0 {
        hue = hue + 1.0;
    }
    var saturation = 0.0;
    if maximum != 0.0 {
        saturation = delta / maximum;
    }
    return vec3f(hue, saturation, maximum);
}

fn hsv_to_rgb(hsv: vec3f) -> vec3f {
    let scaled = fract(hsv.x) * 6.0;
    let sector = u32(floor(scaled)) % 6u;
    let fraction = fract(scaled);
    let p = hsv.z * (1.0 - hsv.y);
    let q = hsv.z * (1.0 - hsv.y * fraction);
    let t = hsv.z * (1.0 - hsv.y * (1.0 - fraction));
    switch sector {
        case 0u: { return vec3f(hsv.z, t, p); }
        case 1u: { return vec3f(q, hsv.z, p); }
        case 2u: { return vec3f(p, hsv.z, t); }
        case 3u: { return vec3f(p, q, hsv.z); }
        case 4u: { return vec3f(t, p, hsv.z); }
        default: { return vec3f(hsv.z, p, q); }
    }
}

fn animated_palette(index: u32) -> vec3f {
    let metadata = animation_metadata[index];
    let base = palette_at_time(index);
    let phase = fract(metadata.phase + animation_clock.values.x * metadata.speed);
    switch metadata.effect {
        case 1u: {
            let hsv = rgb_to_hsv(base);
            return hsv_to_rgb(vec3f(fract(hsv.x + metadata.amplitude * phase), hsv.y, hsv.z));
        }
        case 2u: {
            let factor = clamp(1.0 + metadata.amplitude * sin(6.28318530718 * phase), 0.0, 2.0);
            return base * factor;
        }
        case 3u: {
            let position = f32(metadata.ordinal) + phase;
            let lower = u32(floor(position)) % metadata.member_count;
            let upper = (lower + 1u) % metadata.member_count;
            let fraction = fract(position);
            let lower_color = palette_at_time(cycle_indices[metadata.group_offset + lower]);
            let upper_color = palette_at_time(cycle_indices[metadata.group_offset + upper]);
            return mix(lower_color, upper_color, vec3f(fraction));
        }
        case 4u: {
            // Match the reference Palette.blendShiftColors implementation:
            // integer rotation toward the preceding member, then a
            // 1/100-precision floor fade in authored byte space.
            let amount = phase * f32(metadata.member_count);
            let shift = u32(floor(amount)) % metadata.member_count;
            let fraction = floor(fract(amount) * 100.0) / 100.0;
            let current = (metadata.ordinal + metadata.member_count - shift) % metadata.member_count;
            let previous = (current + metadata.member_count - 1u) % metadata.member_count;
            let current_color = time_of_day_bytes(cycle_indices[metadata.group_offset + current]);
            let previous_color = time_of_day_bytes(cycle_indices[metadata.group_offset + previous]);
            let blended = floor(current_color + (previous_color - current_color) * vec3f(fraction));
            return byte_space_to_linear(blended);
        }
        default: { return base; }
    }
}

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let output_size = vec2f(mapping.output_size);
    let source_size = vec2f(mapping.source_size);
    let scale = max(
        output_size.x / source_size.x,
        output_size.y / source_size.y,
    );
    let crop = (source_size * scale - output_size) * 0.5;
    // Fragment coordinates use the render-target origin; scene PNG rows are
    // authored top-to-bottom, so mirror Y before applying the CPU contract.
    // `position` already denotes the pixel center; do not add another 0.5.
    let output_position = vec2f(position.x, output_size.y - position.y);
    let source_coordinate = floor((output_position + crop) / scale);
    let source_max = mapping.source_size - vec2u(1u);
    let clamped_coordinate = clamp(vec2u(source_coordinate), vec2u(0u), source_max);
    let source_coordinate_u = vec2u(
        clamped_coordinate.x,
        source_max.y - clamped_coordinate.y,
    );
    let index = textureLoad(scene_indices, vec2<i32>(source_coordinate_u), 0).r;
    return vec4f(animated_palette(index), 1.0);
}
"#;

const SOLID_COLOR_SHADER: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4f {
    let positions = array<vec2f, 3>(
        vec2f(-1.0, -1.0),
        vec2f( 3.0, -1.0),
        vec2f(-1.0,  3.0),
    );
    return vec4f(positions[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4f {
    return vec4f(0.035, 0.075, 0.16, 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_mapping_uniforms_are_independent_values() {
        let first = mapping_bytes((3, 2), (1920, 1080));
        let second = mapping_bytes((3, 2), (1280, 720));
        assert_ne!(first, second);
        assert_eq!(&first[..8], &second[..8]);
        assert_ne!(&first[8..], &second[8..]);
    }

    #[test]
    fn zero_extent_is_rejected_before_surface_configuration() {
        assert!(validate_extent((0, 1080)).is_err());
        assert!(validate_extent((1920, 0)).is_err());
    }

    #[test]
    fn actual_surface_error_variants_have_bounded_outcomes() {
        assert_eq!(
            classify_surface_texture(&wgpu::CurrentSurfaceTexture::Timeout),
            RenderOutcome::Timeout
        );
        assert_eq!(
            classify_surface_texture(&wgpu::CurrentSurfaceTexture::Occluded),
            RenderOutcome::Occluded
        );
        assert_eq!(
            classify_surface_texture(&wgpu::CurrentSurfaceTexture::Outdated),
            RenderOutcome::Reconfigure
        );
        assert_eq!(
            classify_surface_texture(&wgpu::CurrentSurfaceTexture::Lost),
            RenderOutcome::Reconfigure
        );
        assert_eq!(
            classify_surface_texture(&wgpu::CurrentSurfaceTexture::Validation),
            RenderOutcome::Fatal
        );
    }

    #[test]
    fn format_selection_requires_srgb_output() {
        assert_eq!(
            choose_format(&[
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Bgra8UnormSrgb,
            ]),
            Some(wgpu::TextureFormat::Bgra8UnormSrgb)
        );
        assert_eq!(choose_format(&[wgpu::TextureFormat::Rgba16Float]), None);
        assert_eq!(choose_format(&[]), None);
    }

    #[test]
    fn allocation_counters_distinguish_shared_and_output_resources() {
        let mut counters = AllocationCounters::default();
        counters.record_scene_upload(true);
        counters.record_pipeline_pair();
        counters.record_output();
        counters.output_bind_group_creations =
            counters.output_bind_group_creations.saturating_add(1);
        counters.record_scene_upload(false);

        assert_eq!(counters.scene_texture_creations, 6);
        assert_eq!(counters.scene_buffer_creations, 10);
        assert_eq!(counters.scene_bind_group_layout_creations, 1);
        assert_eq!(counters.scene_upload_bind_group_creations, 2);
        assert_eq!(counters.pipeline_creations, 2);
        assert_eq!(counters.output_uniform_buffer_creations, 2);
        assert_eq!(counters.output_bind_group_creations, 2);
        assert_eq!(counters.output_initial_surface_configurations, 1);
    }

    #[test]
    fn incompatible_output_capabilities_are_bounded_and_actionable() {
        let no_srgb = wgpu::SurfaceCapabilities {
            formats: vec![wgpu::TextureFormat::Rgba16Float],
            present_modes: vec![wgpu::PresentMode::Fifo],
            alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque],
            ..Default::default()
        };
        let error = validate_output_capabilities(&no_srgb).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no supported sRGB render format")
        );

        let no_fifo = wgpu::SurfaceCapabilities {
            formats: vec![wgpu::TextureFormat::Rgba8UnormSrgb],
            present_modes: Vec::new(),
            alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque],
            ..Default::default()
        };
        let error = validate_output_capabilities(&no_fifo).unwrap_err();
        assert!(error.to_string().contains("required FIFO"));

        let no_alpha = wgpu::SurfaceCapabilities {
            formats: vec![wgpu::TextureFormat::Rgba8UnormSrgb],
            present_modes: vec![wgpu::PresentMode::Fifo],
            alpha_modes: Vec::new(),
            ..Default::default()
        };
        let error = validate_output_capabilities(&no_alpha).unwrap_err();
        assert!(error.to_string().contains("no alpha mode"));
    }

    #[test]
    #[ignore = "requires a Vulkan adapter; run explicitly for S03 GPU evidence"]
    fn gpu_indexed_reference_readback_matches_palette_and_cover_vectors() {
        use std::{sync::mpsc, time::Duration};

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .expect("assigned GPU evidence requires a Vulkan adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("worldwall-s03-readback-device"),
            ..wgpu::DeviceDescriptor::default()
        }))
        .expect("assigned GPU evidence requires a Vulkan device");

        let scene = Scene::reference();
        let output_extent = (4, 2);
        let uploaded = upload_scene(&device, &queue, &scene, output_extent).unwrap();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("worldwall-s03-readback-shader"),
            source: wgpu::ShaderSource::Wgsl(INDEXED_SCENE_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("worldwall-s03-readback-layout"),
            bind_group_layouts: &[Some(&uploaded.indexed_layout)],
            immediate_size: 0,
        });
        let pipeline = create_pipeline(
            &device,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            &pipeline_layout,
            &shader,
            "worldwall-s03-readback-pipeline",
        );

        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("worldwall-s03-readback-output"),
            size: wgpu::Extent3d {
                width: output_extent.0,
                height: output_extent.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
        let readback_stride = 256u32;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("worldwall-s03-readback-buffer"),
            size: u64::from(readback_stride * output_extent.1),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("worldwall-s03-readback-encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("worldwall-s03-readback-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &uploaded.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(readback_stride),
                    rows_per_image: Some(output_extent.1),
                },
            },
            wgpu::Extent3d {
                width: output_extent.0,
                height: output_extent.1,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);

        let (sender, receiver) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("GPU readback poll failed");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("GPU readback callback timed out")
            .expect("GPU readback mapping failed");

        let actual = readback
            .slice(..)
            .get_mapped_range()
            .expect("GPU readback view failed");
        let mut expected =
            Vec::with_capacity(output_extent.0 as usize * output_extent.1 as usize * 4);
        for y in 0..output_extent.1 {
            for x in 0..output_extent.0 {
                let (source_x, source_y) = crate::scene::cover_source_coord(
                    (scene.image.width, scene.image.height),
                    output_extent,
                    (x, y),
                );
                let index = scene.image.indices[(source_y * scene.image.width + source_x) as usize];
                let palette_offset = usize::from(index) * 4;
                expected.extend_from_slice(&scene.palette.rgba[palette_offset..palette_offset + 4]);
            }
        }
        for y in 0..output_extent.1 as usize {
            let row = &actual[y * readback_stride as usize..];
            assert_eq!(
                &row[..expected.len() / output_extent.1 as usize],
                &expected[y * expected.len() / output_extent.1 as usize
                    ..(y + 1) * expected.len() / output_extent.1 as usize]
            );
        }
        drop(actual);
        readback.unmap();
    }

    fn readback_scene(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        output_extent: (u32, u32),
        animation_time_seconds: f32,
        civil_seconds: u32,
    ) -> Vec<u8> {
        readback_scene_with_sample(
            device,
            queue,
            scene,
            output_extent,
            animation_time_seconds,
            civil_seconds,
            None,
        )
    }

    fn readback_scene_with_sample(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        output_extent: (u32, u32),
        animation_time_seconds: f32,
        civil_seconds: u32,
        solar_sample: Option<crate::scene::ResolvedPaletteSample>,
    ) -> Vec<u8> {
        use std::{sync::mpsc, time::Duration};

        let uploaded = upload_scene(device, queue, scene, output_extent).unwrap();
        queue.write_buffer(
            &uploaded.animation_clock_buffer,
            0,
            &animation_clock_bytes(
                animation_time_seconds,
                civil_seconds,
                uploaded.time_of_day_keyframe_count,
                uploaded.time_of_day_enabled,
                solar_sample,
            ),
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("worldwall-s04-readback-shader"),
            source: wgpu::ShaderSource::Wgsl(INDEXED_SCENE_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("worldwall-s04-readback-layout"),
            bind_group_layouts: &[Some(&uploaded.indexed_layout)],
            immediate_size: 0,
        });
        let pipeline = create_pipeline(
            device,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            &pipeline_layout,
            &shader,
            "worldwall-s04-readback-pipeline",
        );
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("worldwall-s04-readback-output"),
            size: wgpu::Extent3d {
                width: output_extent.0,
                height: output_extent.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
        let readback_stride = 256u32;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("worldwall-s04-readback-buffer"),
            size: u64::from(readback_stride * output_extent.1),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("worldwall-s04-readback-encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("worldwall-s04-readback-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &uploaded.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(readback_stride),
                    rows_per_image: Some(output_extent.1),
                },
            },
            wgpu::Extent3d {
                width: output_extent.0,
                height: output_extent.1,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);

        let (sender, receiver) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("GPU animated readback poll failed");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("GPU animated readback callback timed out")
            .expect("GPU animated readback mapping failed");

        let actual = readback
            .slice(..)
            .get_mapped_range()
            .expect("GPU animated readback view failed");
        let row_bytes = output_extent.0 as usize * 4;
        let result = actual[..row_bytes].to_vec();
        drop(actual);
        readback.unmap();
        result
    }

    #[test]
    #[ignore = "requires a Vulkan adapter; run explicitly for S04 GPU evidence"]
    fn gpu_animated_palette_readback_matches_cpu_reference_vectors() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .expect("assigned GPU evidence requires a Vulkan adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("worldwall-s04-readback-device"),
            ..wgpu::DeviceDescriptor::default()
        }))
        .expect("assigned GPU evidence requires a Vulkan device");

        let animation = crate::animation::Animation::from_toml(
            r#"
                schema_version = 1
                [[group]]
                name = "hue"
                effect = "hue_cycle"
                indices = [1]
                speed = 0.125
                amplitude = 0.75
                phase = 0.125
                [[group]]
                name = "pulse"
                effect = "brightness_pulse"
                indices = [2]
                speed = 0.25
                amplitude = 0.5
                phase = 0.25
                [[group]]
                name = "cycle"
                effect = "palette_cycle"
                indices = [3, 4, 5]
                speed = -0.2
                amplitude = 1.0
                phase = 0.1
                [[group]]
                name = "blend"
                effect = "blend_shift"
                indices = [6, 7, 8]
                speed = 0.375
                amplitude = 1.0
                phase = 0.25
            "#,
        )
        .unwrap();
        let mut rgba = [0u8; 256 * 4];
        for entry in rgba.chunks_exact_mut(4) {
            entry[3] = 255;
        }
        for (index, color) in [
            (1usize, [255, 32, 16]),
            (2, [32, 255, 64]),
            (3, [255, 0, 0]),
            (4, [0, 255, 0]),
            (5, [0, 0, 255]),
            (6, [15, 40, 200]),
            (7, [120, 80, 20]),
            (8, [220, 180, 60]),
        ] {
            rgba[index * 4..index * 4 + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
        }
        let scene = Scene::with_animation(
            crate::scene::IndexedImage {
                width: 8,
                height: 1,
                indices: vec![1, 2, 3, 4, 5, 6, 7, 8],
            },
            crate::scene::Palette { rgba },
            animation,
        )
        .unwrap();

        for time in [0.0, 0.375, 1.25, 1023.75] {
            let actual = readback_scene(&device, &queue, &scene, (8, 1), time, 0);
            let expected_palette = scene.animation.apply_palette(&scene.palette.rgba, time);
            for (x, index) in scene.image.indices.iter().copied().enumerate() {
                let expected =
                    &expected_palette[usize::from(index) * 4..usize::from(index) * 4 + 4];
                let actual = &actual[x * 4..x * 4 + 4];
                assert_eq!(actual[3], 255, "alpha at x={x}, time={time}");
                for channel in 0..3 {
                    let delta = i16::from(actual[channel]) - i16::from(expected[channel]);
                    assert!(
                        delta.abs() <= 1,
                        "animated GPU/CPU mismatch x={x} channel={channel} time={time}: actual={} expected={}",
                        actual[channel],
                        expected[channel]
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "requires a Vulkan adapter; run explicitly for S04/CR-0002/CR-0005 GPU readback evidence"]
    fn gpu_time_of_day_palette_readback_matches_byte_floor_reference() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .expect("assigned GPU evidence requires a Vulkan adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("worldwall-cr002-time-of-day-readback-device"),
            ..wgpu::DeviceDescriptor::default()
        }))
        .expect("assigned GPU evidence requires a Vulkan device");

        let mut rgba = vec![0u8; 2 * 256 * 4];
        for entry in rgba.chunks_exact_mut(4) {
            entry[3] = 255;
        }
        rgba[0..4].copy_from_slice(&[10, 20, 30, 255]);
        rgba[256 * 4..256 * 4 + 4].copy_from_slice(&[20, 10, 40, 255]);
        let timeline = crate::scene::TimeOfDayPalette::new(
            2,
            rgba,
            vec![
                crate::scene::TimeOfDayKeyframe {
                    civil_seconds: 10,
                    row: 0,
                },
                crate::scene::TimeOfDayKeyframe {
                    civil_seconds: 20,
                    row: 1,
                },
            ],
        )
        .unwrap();
        let mut palette = [0u8; 256 * 4];
        for entry in palette.chunks_exact_mut(4) {
            entry[3] = 255;
        }
        let scene = Scene::with_resources(
            crate::scene::IndexedImage {
                width: 1,
                height: 1,
                indices: vec![0],
            },
            crate::scene::Palette { rgba: palette },
            crate::animation::Animation::static_default(),
            Some(timeline.clone()),
        )
        .unwrap();
        for civil_seconds in [10, 15, 20, 86_399] {
            let actual = readback_scene(&device, &queue, &scene, (1, 1), 0.0, civil_seconds);
            let expected = timeline.sample_rgb(0, civil_seconds);
            assert_eq!(&actual[..3], &expected);
            assert_eq!(actual[3], 255);
        }

        let solar_timeline = crate::scene::TimeOfDayPalette::new_solar(
            2,
            [
                [10, 20, 30, 255]
                    .into_iter()
                    .cycle()
                    .take(256 * 4)
                    .collect::<Vec<_>>(),
                [20, 10, 40, 255]
                    .into_iter()
                    .cycle()
                    .take(256 * 4)
                    .collect::<Vec<_>>(),
            ]
            .concat(),
            vec![
                crate::scene::SolarTimeOfDayKeyframe {
                    event: crate::scene::SolarEvent::Sunrise,
                    offset_minutes: -30,
                    row: 0,
                },
                crate::scene::SolarTimeOfDayKeyframe {
                    event: crate::scene::SolarEvent::Sunset,
                    offset_minutes: 30,
                    row: 1,
                },
            ],
        )
        .unwrap();
        let mut base_palette = [0u8; 256 * 4];
        for entry in base_palette.chunks_exact_mut(4) {
            entry[3] = 255;
        }
        base_palette[0..4].copy_from_slice(&[40, 50, 60, 255]);
        let solar_scene = Scene::with_resources(
            crate::scene::IndexedImage {
                width: 1,
                height: 1,
                indices: vec![0],
            },
            crate::scene::Palette { rgba: base_palette },
            crate::animation::Animation::static_default(),
            Some(solar_timeline),
        )
        .unwrap();
        let actual = readback_scene_with_sample(
            &device,
            &queue,
            &solar_scene,
            (1, 1),
            0.0,
            0,
            Some(crate::scene::ResolvedPaletteSample::Rows {
                lower_row: 0,
                upper_row: 1,
                amount: 0.5,
            }),
        );
        assert_eq!(&actual[..3], &[15, 15, 35]);

        let animated = crate::animation::Animation::from_toml(
            r#"
                schema_version = 1
                [[group]]
                name = "solar-pulse"
                effect = "brightness_pulse"
                indices = [0]
                speed = 0.25
                amplitude = 0.5
                phase = 0.0
            "#,
        )
        .unwrap();
        let animated_scene = Scene::with_resources(
            solar_scene.image.clone(),
            solar_scene.palette.clone(),
            animated.clone(),
            solar_scene.time_of_day.clone(),
        )
        .unwrap();
        let animated_actual = readback_scene_with_sample(
            &device,
            &queue,
            &animated_scene,
            (1, 1),
            1.0,
            0,
            Some(crate::scene::ResolvedPaletteSample::Rows {
                lower_row: 0,
                upper_row: 1,
                amount: 0.5,
            }),
        );
        let mut solar_interpolated_palette = solar_scene.palette.rgba;
        solar_interpolated_palette[0..4].copy_from_slice(&[15, 15, 35, 255]);
        let expected_animated_palette = animated.apply_palette(&solar_interpolated_palette, 1.0);
        assert_eq!(&animated_actual[..3], &expected_animated_palette[..3]);

        let fallback = readback_scene_with_sample(
            &device,
            &queue,
            &solar_scene,
            (1, 1),
            0.0,
            0,
            Some(crate::scene::ResolvedPaletteSample::Base),
        );
        assert_eq!(&fallback[..3], &[40, 50, 60]);
    }
}
