//! `wgpu` surface management and instanced 2D quad rendering.

use std::error::Error;
use std::fmt;

use expanse_sim::Snapshot;

use crate::camera::{Camera, CameraUniform, Viewport};
use crate::scene::{QuadInstance, build_instances};

/// Errors that can prevent creating or running the renderer.
#[derive(Debug)]
pub enum RenderError {
    /// Failed to create the window surface.
    CreateSurface(wgpu::CreateSurfaceError),
    /// No compatible GPU adapter was found.
    RequestAdapter(wgpu::RequestAdapterError),
    /// Failed to open the logical GPU device.
    RequestDevice(wgpu::RequestDeviceError),
    /// The surface reported no supported texture formats.
    UnsupportedSurface,
    /// The surface reported a validation error while acquiring a frame.
    SurfaceValidation,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateSurface(err) => write!(f, "failed to create GPU surface: {err}"),
            Self::RequestAdapter(err) => write!(f, "no suitable GPU adapter found: {err}"),
            Self::RequestDevice(err) => write!(f, "failed to request GPU device: {err}"),
            Self::UnsupportedSurface => write!(f, "surface exposes no compatible texture format"),
            Self::SurfaceValidation => write!(f, "surface validation error while acquiring frame"),
        }
    }
}

impl Error for RenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CreateSurface(err) => Some(err),
            Self::RequestAdapter(err) => Some(err),
            Self::RequestDevice(err) => Some(err),
            Self::UnsupportedSurface | Self::SurfaceValidation => None,
        }
    }
}

fn quad_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
    ];
    wgpu::VertexBufferLayout {
        array_stride: size_of::<QuadInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

const INITIAL_INSTANCE_CAPACITY: usize = 1024;

// Placeholder M0 palette in linear RGBA. Assumes an sRGB surface, which
// `build_surface_config` prefers.
const BACKGROUND_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.025,
    g: 0.032,
    b: 0.045,
    a: 1.0,
};

/// Owns the `wgpu` surface, device, and instanced 2D quad pipeline.
pub struct Renderer<'window> {
    surface: wgpu::Surface<'window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instances: Vec<QuadInstance>,
}

impl<'window> Renderer<'window> {
    /// Initializes `wgpu` for `target` with an initial physical pixel size.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] if the surface cannot be created, no compatible
    /// GPU adapter or device can be opened, or the surface has no usable
    /// texture format.
    pub fn new(
        display: impl wgpu::rwh::HasDisplayHandle + fmt::Debug + Send + Sync + 'static,
        target: impl Into<wgpu::SurfaceTarget<'window>>,
        physical_width: u32,
        physical_height: u32,
    ) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display)),
        );
        let surface = instance
            .create_surface(target)
            .map_err(RenderError::CreateSurface)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(RenderError::RequestAdapter)?;

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(RenderError::RequestDevice)?;

        let surface_config =
            build_surface_config(&surface, &adapter, physical_width, physical_height)?;
        if physical_width > 0 && physical_height > 0 {
            surface.configure(&device, &surface_config);
        }

        let (pipeline, camera_buffer, camera_bind_group) =
            create_quad_pipeline(&device, surface_config.format);
        let instance_buffer = create_instance_buffer(&device, INITIAL_INSTANCE_CAPACITY);

        Ok(Self {
            surface,
            device,
            queue,
            surface_config,
            pipeline,
            camera_buffer,
            camera_bind_group,
            instance_buffer,
            instance_capacity: INITIAL_INSTANCE_CAPACITY,
            instances: Vec::with_capacity(INITIAL_INSTANCE_CAPACITY),
        })
    }

    /// Reconfigures the swapchain when the window's physical size changes.
    /// Zero-sized surfaces (such as minimized windows) are ignored until
    /// restored.
    pub fn resize(&mut self, physical_width: u32, physical_height: u32) {
        if physical_width == 0 || physical_height == 0 {
            return;
        }
        self.surface_config.width = physical_width;
        self.surface_config.height = physical_height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Draws the tiles of the world visible through `camera` and `viewport`,
    /// then the entities in `snapshot`. Does nothing for an empty viewport.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::SurfaceValidation`] if acquiring a frame reports
    /// a validation error. Lost or outdated surfaces are reconfigured, and
    /// timeouts and occluded windows skip the frame; none of these is an error.
    pub fn render(
        &mut self,
        camera: &Camera,
        viewport: Viewport,
        scale_factor: f32,
        snapshot: &Snapshot,
    ) -> Result<(), RenderError> {
        if viewport.width <= 0.0 || viewport.height <= 0.0 {
            return Ok(());
        }

        let (frame, reconfigure_after) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(RenderError::SurfaceValidation);
            }
        };

        let uniform = camera.uniform(viewport, scale_factor);
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));

        build_instances(&mut self.instances, camera, viewport, snapshot);
        if !self.instances.is_empty() {
            self.ensure_instance_capacity(self.instances.len());
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instances),
            );
        }

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("world-frame-encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BACKGROUND_CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            if let Ok(instance_count) = u32::try_from(self.instances.len())
                && instance_count > 0
            {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                pass.draw(0..6, 0..instance_count);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.present(frame);
        if reconfigure_after {
            self.surface.configure(&self.device, &self.surface_config);
        }
        Ok(())
    }

    fn ensure_instance_capacity(&mut self, needed: usize) {
        if needed <= self.instance_capacity {
            return;
        }
        let new_capacity = needed.next_power_of_two();
        self.instance_buffer = create_instance_buffer(&self.device, new_capacity);
        self.instance_capacity = new_capacity;
    }
}

fn build_surface_config(
    surface: &wgpu::Surface<'_>,
    adapter: &wgpu::Adapter,
    physical_width: u32,
    physical_height: u32,
) -> Result<wgpu::SurfaceConfiguration, RenderError> {
    let capabilities = surface.get_capabilities(adapter);
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .or_else(|| capabilities.formats.first().copied())
        .ok_or(RenderError::UnsupportedSurface)?;
    let alpha_mode = capabilities
        .alpha_modes
        .first()
        .copied()
        .unwrap_or(wgpu::CompositeAlphaMode::Auto);

    Ok(wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width: physical_width.max(1),
        height: physical_height.max(1),
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode,
        view_formats: Vec::new(),
        desired_maximum_frame_latency: 2,
    })
}

fn create_quad_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup) {
    let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("camera-uniform"),
        size: size_of::<CameraUniform>() as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let camera_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera-bind-group-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera-bind-group"),
        layout: &camera_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("quad-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("quad-pipeline-layout"),
        bind_group_layouts: &[Some(&camera_bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("quad-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(quad_instance_layout())],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    (pipeline, camera_buffer, camera_bind_group)
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    let byte_size = (capacity * size_of::<QuadInstance>()) as wgpu::BufferAddress;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("quad-instance-buffer"),
        size: byte_size.max(size_of::<QuadInstance>() as wgpu::BufferAddress),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
