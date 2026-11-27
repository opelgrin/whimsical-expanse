//! `wgpu` surface management and instanced 2D quad rendering.

use std::error::Error;
use std::fmt;

use crate::assets::Assets;
use crate::camera::{Camera, CameraUniform, Viewport};
use crate::interpolation::Interpolation;
use crate::palette::TerrainView;
use crate::scene::{QuadInstance, TerrainStyle, build_instances};
use crate::sprite_pipeline::SpritePipeline;
use crate::terrain_pipeline::TerrainPipeline;

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

// Linear RGBA. Assumes an sRGB surface, which `build_surface_config` prefers.
const BACKGROUND_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.025,
    g: 0.032,
    b: 0.045,
    a: 1.0,
};

/// What the UI drew this frame, to paint over the world.
///
/// The fields are `egui`'s output for one frame. Texture changes must be passed
/// on every frame, even when nothing is drawn, because `egui` sends each one
/// only once.
pub struct UiFrame<'a> {
    /// The UI's shapes, tessellated into triangles.
    pub primitives: &'a [egui::ClippedPrimitive],
    /// Textures (such as the font atlas) to create, update or free.
    pub textures_delta: &'a egui::TexturesDelta,
    /// Physical pixels per UI point.
    pub pixels_per_point: f32,
}

/// Owns the `wgpu` surface, device, the world rendering pipelines, and the UI
/// renderer.
pub struct Renderer<'window> {
    surface: wgpu::Surface<'window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    terrain_pipeline: TerrainPipeline,
    sprite_pipeline: SpritePipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instances: Vec<QuadInstance>,
    ui_painter: egui_wgpu::Renderer,
    atlases: Vec<Atlas>,
}

/// An atlas page on the GPU.
struct Atlas {
    /// Kept so that the texture lives as long as the renderer.
    _texture: wgpu::Texture,
    shown: AtlasTexture,
}

/// An atlas page, as the UI can show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasTexture {
    /// The texture, registered with the UI.
    pub texture_id: egui::TextureId,
    /// The width and height in pixels.
    pub size: [u32; 2],
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

        let (pipeline, camera_buffer, camera_bind_group_layout, camera_bind_group) =
            create_quad_pipeline(&device, surface_config.format);
        let terrain_pipeline = TerrainPipeline::new(&device, surface_config.format);
        let sprite_pipeline =
            SpritePipeline::new(&device, surface_config.format, &camera_bind_group_layout);
        let instance_buffer = create_instance_buffer(&device, INITIAL_INSTANCE_CAPACITY);
        let ui_painter = egui_wgpu::Renderer::new(
            &device,
            surface_config.format,
            egui_wgpu::RendererOptions::default(),
        );

        Ok(Self {
            surface,
            device,
            queue,
            surface_config,
            pipeline,
            terrain_pipeline,
            sprite_pipeline,
            camera_buffer,
            camera_bind_group,
            instance_buffer,
            instance_capacity: INITIAL_INSTANCE_CAPACITY,
            instances: Vec::with_capacity(INITIAL_INSTANCE_CAPACITY),
            ui_painter,
            atlases: Vec::new(),
        })
    }

    /// Uploads the atlas pages (with their mipmap level) and block table of
    /// `assets`, replacing any uploaded before, and registers the base atlas
    /// pages with the UI.
    pub fn upload_assets(&mut self, assets: &Assets) {
        for atlas in self.atlases.drain(..) {
            self.ui_painter.free_texture(&atlas.shown.texture_id);
        }
        let mut atlas_views = Vec::with_capacity(assets.container().atlases().len());
        for (index, page) in assets.container().atlases().iter().enumerate() {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("atlas {index}")),
                size: wgpu::Extent3d {
                    width: page.width(),
                    height: page.height(),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 2,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            write_mip_level(&self.queue, &texture, 0, page.image());
            write_mip_level(&self.queue, &texture, 1, page.mipmap());

            let full_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            atlas_views.push(full_view);

            let base_view = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: 0,
                mip_level_count: Some(1),
                ..wgpu::TextureViewDescriptor::default()
            });
            let texture_id = self.ui_painter.register_native_texture(
                &self.device,
                &base_view,
                wgpu::FilterMode::Nearest,
            );
            self.atlases.push(Atlas {
                _texture: texture,
                shown: AtlasTexture {
                    texture_id,
                    size: [page.width(), page.height()],
                },
            });
        }
        self.terrain_pipeline.set_assets(
            &self.device,
            &self.queue,
            &self.camera_buffer,
            assets,
            &atlas_views,
        );
        self.sprite_pipeline.set_assets(&self.device, &atlas_views);
    }

    /// The atlas pages uploaded, as the UI can show them.
    #[must_use]
    pub fn atlas_textures(&self) -> Vec<AtlasTexture> {
        self.atlases.iter().map(|atlas| atlas.shown).collect()
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

    /// Draws the tiles of the world visible through `camera` and `viewport`
    /// in `terrain`'s style, then the entities where `interpolation` places
    /// them, then `ui` on top.
    /// Draws nothing for an empty viewport, but still applies the texture
    /// changes of `ui`.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::SurfaceValidation`] if acquiring a frame reports
    /// a validation error. Lost or outdated surfaces are reconfigured, and
    /// timeouts and occluded windows skip the frame; none of these is an error.
    #[expect(
        clippy::too_many_arguments,
        reason = "each frame input is a distinct borrowed view"
    )]
    pub fn render(
        &mut self,
        camera: &Camera,
        viewport: Viewport,
        scale_factor: f32,
        interpolation: &Interpolation<'_>,
        assets: &Assets,
        terrain: &TerrainStyle<'_>,
        ui: &UiFrame<'_>,
    ) -> Result<(), RenderError> {
        for (id, deltas) in &ui.textures_delta.set {
            for delta in deltas {
                self.ui_painter
                    .update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        let result = self.draw(
            camera,
            viewport,
            scale_factor,
            interpolation,
            assets,
            terrain,
            ui,
        );
        // Freed only after the frame that may still use them is submitted.
        for id in &ui.textures_delta.free {
            self.ui_painter.free_texture(id);
        }
        result
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "each frame input is a distinct borrowed view"
    )]
    fn draw(
        &mut self,
        camera: &Camera,
        viewport: Viewport,
        scale_factor: f32,
        interpolation: &Interpolation<'_>,
        assets: &Assets,
        terrain: &TerrainStyle<'_>,
        ui: &UiFrame<'_>,
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

        let textured = terrain.display.view == TerrainView::Terrain;
        if textured && let Some(visible) = camera.visible_tiles(viewport) {
            self.terrain_pipeline.prepare(
                &self.device,
                &self.queue,
                visible,
                &interpolation.current.terrain,
                assets,
            );
        }
        self.sprite_pipeline.prepare(
            &self.device,
            &self.queue,
            camera,
            viewport,
            interpolation,
            assets,
            textured,
        );

        let quad_ranges = build_instances(
            &mut self.instances,
            camera,
            viewport,
            &interpolation.current.terrain,
            terrain,
        );
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

        self.draw_world(&mut encoder, &view, textured, quad_ranges);
        let ui_commands = self.draw_ui(&mut encoder, &view, ui);

        self.queue.submit(
            ui_commands
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        self.queue.present(frame);
        if reconfigure_after {
            self.surface.configure(&self.device, &self.surface_config);
        }
        Ok(())
    }

    /// Records the world render pass: terrain (textured or flat), sorted
    /// sprites, and the debug obstacle layer on top.
    fn draw_world(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        textured: bool,
        quad_ranges: crate::scene::QuadRanges,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world-render-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
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

        if textured {
            self.terrain_pipeline.draw(&mut pass);
        } else {
            self.draw_quads(&mut pass, quad_ranges.flat_tiles);
        }

        self.sprite_pipeline
            .draw(&mut pass, &self.camera_bind_group);

        self.draw_quads(&mut pass, quad_ranges.obstacle_marks);
    }

    fn draw_quads<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        range: std::ops::Range<u32>,
    ) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, range);
    }

    /// Records a pass that paints `ui` over what `view` already holds. Returns
    /// the command buffers `egui` prepared, to submit before `encoder`.
    fn draw_ui(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        ui: &UiFrame<'_>,
    ) -> Vec<wgpu::CommandBuffer> {
        if ui.primitives.is_empty() {
            return Vec::new();
        }
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point: ui.pixels_per_point,
        };
        let commands = self.ui_painter.update_buffers(
            &self.device,
            &self.queue,
            encoder,
            ui.primitives,
            &screen,
        );
        // `egui_wgpu` requires a pass without a borrow of the encoder.
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        self.ui_painter.render(&mut pass, ui.primitives, &screen);
        commands
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
) -> (
    wgpu::RenderPipeline,
    wgpu::Buffer,
    wgpu::BindGroupLayout,
    wgpu::BindGroup,
) {
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

    (
        pipeline,
        camera_buffer,
        camera_bind_group_layout,
        camera_bind_group,
    )
}

fn write_mip_level(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    mip_level: u32,
    image: &expanse_assets::Image,
) {
    let bytes: Vec<u8> = image.pixels().iter().flatten().copied().collect();
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * image.width()),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width: image.width(),
            height: image.height(),
            depth_or_array_layers: 1,
        },
    );
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
