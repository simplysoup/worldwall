//! Minimal Vulkan presentation for the retained S01 Wayland surface.
//!
//! The renderer owns only wgpu resources. The caller owns the Wayland
//! connection and layer surface and must keep them alive for this renderer's
//! lifetime. The raw-handle boundary is intentionally one small function so
//! its validity and drop-order argument can be reviewed independently.

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
    bind_group: wgpu::BindGroup,
    index_texture: wgpu::Texture,
    palette_texture: wgpu::Texture,
    mapping_buffer: wgpu::Buffer,
    source_extent: (u32, u32),
    queue: wgpu::Queue,
    device: wgpu::Device,
    surface: wgpu::Surface<'wayland>,
    instance: wgpu::Instance,
    format: wgpu::TextureFormat,
    config: wgpu::SurfaceConfiguration,
    wayland_surface: &'wayland wl_surface::WlSurface,
    scene_upload_count: u64,
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
        let UploadedScene {
            index_texture,
            palette_texture,
            bind_group,
            mapping_buffer,
            indexed_layout,
        } = uploaded;
        let indexed_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("worldwall-indexed-scene-shader"),
            source: wgpu::ShaderSource::Wgsl(INDEXED_SCENE_SHADER.into()),
        });
        let indexed_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("worldwall-indexed-scene-layout"),
                bind_group_layouts: &[Some(&indexed_layout)],
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
            bind_group,
            index_texture,
            palette_texture,
            mapping_buffer,
            source_extent: (scene.image.width, scene.image.height),
            format,
            config,
            wayland_surface,
            scene_upload_count: 1,
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

    /// Acquire, draw, submit, and present one opaque fullscreen triangle.
    ///
    /// The caller requests a Wayland frame callback before calling this method.
    /// wgpu's `present` performs the actual surface commit; no duplicate manual
    /// commit is issued here.
    pub fn render(&mut self) -> Result<RenderOutcome, Box<dyn Error>> {
        #[cfg(debug_assertions)]
        if let Some(outcome) = self.test_surface_error.take() {
            println!("trace test-surface-error injected={outcome:?}");
            return Ok(outcome);
        }

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
                pass.set_bind_group(0, &self.bind_group, &[]);
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
        let _scene_resources_alive = (&self.index_texture, &self.palette_texture);
        self.scene_upload_count
    }

    fn update_mapping(&self, extent: (u32, u32)) {
        self.queue.write_buffer(
            &self.mapping_buffer,
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

struct UploadedScene {
    index_texture: wgpu::Texture,
    palette_texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    mapping_buffer: wgpu::Buffer,
    indexed_layout: wgpu::BindGroupLayout,
}

fn upload_scene(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &Scene,
    output_extent: (u32, u32),
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

    // The layout is created here so the bind group and pipeline share the same
    // concrete layout object for this upload.
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        ],
    });
    let index_view = index_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let palette_view = palette_texture.create_view(&wgpu::TextureViewDescriptor::default());
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
        ],
    });
    Ok(UploadedScene {
        index_texture,
        palette_texture,
        bind_group,
        mapping_buffer,
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
    let authored = textureLoad(palette, vec2<i32>(i32(index), 0), 0).rgb;
    return vec4f(
        srgb_to_linear(authored.r),
        srgb_to_linear(authored.g),
        srgb_to_linear(authored.b),
        1.0,
    );
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
}
