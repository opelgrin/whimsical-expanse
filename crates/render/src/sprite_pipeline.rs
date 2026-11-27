//! Textured sprite GPU pipeline, dynamic instance buffer, and atlas page
//! bindings.

use crate::assets::Assets;
use crate::camera::{Camera, Viewport};
use crate::interpolation::Interpolation;
use crate::sprites::{SpriteInstance, SpriteScene};

const INITIAL_INSTANCE_CAPACITY: usize = 1024;

/// Owns the textured sprite render pipeline, its per-frame instance buffer,
/// and the atlas bind groups.
pub(crate) struct SpritePipeline {
    pipeline: wgpu::RenderPipeline,
    atlas_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    atlas_bind_groups: Vec<wgpu::BindGroup>,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    scene: SpriteScene,
}

impl SpritePipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let atlas_bind_group_layout = create_atlas_bind_group_layout(device);
        let pipeline = create_render_pipeline(
            device,
            format,
            camera_bind_group_layout,
            &atlas_bind_group_layout,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sprite-atlas-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        let instance_buffer = create_instance_buffer(device, INITIAL_INSTANCE_CAPACITY);

        Self {
            pipeline,
            atlas_bind_group_layout,
            sampler,
            atlas_bind_groups: Vec::new(),
            instance_buffer,
            instance_capacity: INITIAL_INSTANCE_CAPACITY,
            scene: SpriteScene::default(),
        }
    }

    /// Binds the uploaded atlas texture views, replacing any bound before.
    pub(crate) fn set_assets(&mut self, device: &wgpu::Device, atlas_views: &[wgpu::TextureView]) {
        self.atlas_bind_groups = atlas_views
            .iter()
            .enumerate()
            .map(|(index, view)| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("sprite-atlas-bind-group-{index}")),
                    layout: &self.atlas_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                })
            })
            .collect();
    }

    /// Gathers, sorts, and uploads every visible sprite for this frame.
    #[expect(
        clippy::too_many_arguments,
        reason = "each frame input is a distinct borrowed view"
    )]
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &Camera,
        viewport: Viewport,
        interpolation: &Interpolation<'_>,
        assets: &Assets,
        include_obstacles: bool,
    ) {
        if self.atlas_bind_groups.is_empty() {
            self.scene.instances.clear();
            self.scene.batches.clear();
            return;
        }
        self.scene
            .build(camera, viewport, interpolation, assets, include_obstacles);
        if !self.scene.instances.is_empty() {
            self.ensure_capacity(device, self.scene.instances.len());
            queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.scene.instances),
            );
        }
    }

    /// Draws the sprites prepared by [`prepare`](Self::prepare) into `pass`.
    pub(crate) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera_bind_group: &'pass wgpu::BindGroup,
    ) {
        if self.scene.batches.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        let mut bound_atlas: Option<u16> = None;
        for batch in &self.scene.batches {
            if bound_atlas != Some(batch.atlas) {
                pass.set_bind_group(1, &self.atlas_bind_groups[usize::from(batch.atlas)], &[]);
                bound_atlas = Some(batch.atlas);
            }
            pass.draw(0..6, batch.start..batch.start + batch.count);
        }
    }

    fn ensure_capacity(&mut self, device: &wgpu::Device, needed: usize) {
        if needed <= self.instance_capacity {
            return;
        }
        let new_capacity = needed.next_power_of_two();
        self.instance_buffer = create_instance_buffer(device, new_capacity);
        self.instance_capacity = new_capacity;
    }
}

fn create_atlas_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sprite-atlas-bind-group-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_render_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    atlas_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sprite-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sprite-pipeline-layout"),
        bind_group_layouts: &[Some(camera_layout), Some(atlas_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sprite-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(sprite_instance_layout())],
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
    })
}

fn sprite_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32x2,
    ];
    wgpu::VertexBufferLayout {
        array_stride: size_of::<SpriteInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    let byte_size = (capacity * size_of::<SpriteInstance>()) as wgpu::BufferAddress;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sprite-instance-buffer"),
        size: byte_size.max(size_of::<SpriteInstance>() as wgpu::BufferAddress),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
