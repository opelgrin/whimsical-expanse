//! Textured terrain GPU pipeline, atlas page bindings, and per-chunk GPU cache.

use std::num::NonZeroU64;

use bytemuck::{Pod, Zeroable};
use expanse_assets::SEASONS;
use expanse_terrain::{CHUNK_SIZE, ChunkPos, Terrain};

use crate::assets::{Assets, GroundBlock};
use crate::camera::VisibleTileRange;
use crate::chunk_cache::ChunkCache;
use crate::terrain_mesh::{AtlasBatch, TerrainInstance, build_chunk_mesh};

/// How many chunks stay cached on the GPU across frames. Easily covers a 4K
/// viewport at minimum zoom plus panning history (~5 MB of instance buffers).
const CHUNK_CACHE_BUDGET: usize = 512;

const INITIAL_CHUNK_ORIGIN_CAPACITY: usize = 64;

/// GPU layout of one ground or ground-pair block's seasonal variant origins.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuBlockInfo {
    variants: [[u32; 2]; SEASONS],
}

impl From<GroundBlock> for GpuBlockInfo {
    fn from(block: GroundBlock) -> Self {
        Self {
            variants: block.variant_origins,
        }
    }
}

/// Per-chunk draw uniform: the chunk's world-tile origin (including wrap offset).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ChunkUniform {
    origin: [f32; 2],
    _padding: [f32; 2],
}

/// Cached GPU instance buffer and draw batches for one canonical chunk.
struct GpuChunk {
    buffer: wgpu::Buffer,
    batches: Vec<AtlasBatch>,
}

/// One visible chunk placement to draw this frame.
#[derive(Debug, Clone, Copy)]
struct VisibleChunk {
    chunk: ChunkPos,
    origin: [f32; 2],
}

/// Owns the textured terrain pipeline, block table, atlas bind groups, and the
/// per-chunk LRU GPU cache.
pub(crate) struct TerrainPipeline {
    pipeline: wgpu::RenderPipeline,
    frame_bind_group_layout: wgpu::BindGroupLayout,
    atlas_bind_group_layout: wgpu::BindGroupLayout,
    chunk_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    frame_bind_group: Option<wgpu::BindGroup>,
    atlas_bind_groups: Vec<wgpu::BindGroup>,
    chunk_origin_buffer: wgpu::Buffer,
    chunk_origin_bind_group: wgpu::BindGroup,
    chunk_origin_capacity: usize,
    chunk_origin_stride: u32,
    chunk_origin_staging: Vec<u8>,
    visible_draws: Vec<VisibleChunk>,
    cache: ChunkCache<GpuChunk>,
}

impl TerrainPipeline {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let (frame_bind_group_layout, atlas_bind_group_layout, chunk_bind_group_layout) =
            create_bind_group_layouts(device);
        let pipeline = create_render_pipeline(
            device,
            format,
            &frame_bind_group_layout,
            &atlas_bind_group_layout,
            &chunk_bind_group_layout,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });

        let alignment = device.limits().min_uniform_buffer_offset_alignment;
        let uniform_size =
            u32::try_from(size_of::<ChunkUniform>()).expect("ChunkUniform fits in u32");
        let chunk_origin_stride = uniform_size.next_multiple_of(alignment.max(1));
        let (chunk_origin_buffer, chunk_origin_bind_group) = create_chunk_origin_buffer(
            device,
            &chunk_bind_group_layout,
            INITIAL_CHUNK_ORIGIN_CAPACITY,
            chunk_origin_stride,
        );

        Self {
            pipeline,
            frame_bind_group_layout,
            atlas_bind_group_layout,
            chunk_bind_group_layout,
            sampler,
            frame_bind_group: None,
            atlas_bind_groups: Vec::new(),
            chunk_origin_buffer,
            chunk_origin_bind_group,
            chunk_origin_capacity: INITIAL_CHUNK_ORIGIN_CAPACITY,
            chunk_origin_stride,
            chunk_origin_staging: Vec::new(),
            visible_draws: Vec::new(),
            cache: ChunkCache::new(CHUNK_CACHE_BUDGET),
        }
    }

    /// Uploads the resolved block table and binds the uploaded atlas texture
    /// views. Clears the chunk cache so chunks rebuild against the new assets.
    pub(crate) fn set_assets(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera_buffer: &wgpu::Buffer,
        assets: &Assets,
        atlas_views: &[wgpu::TextureView],
    ) {
        self.cache.clear();
        let blocks: Vec<GpuBlockInfo> = if assets.ground_blocks().is_empty() {
            vec![GpuBlockInfo::zeroed()]
        } else {
            assets
                .ground_blocks()
                .iter()
                .copied()
                .map(GpuBlockInfo::from)
                .collect()
        };
        let byte_size = (blocks.len() * size_of::<GpuBlockInfo>()) as wgpu::BufferAddress;
        let blocks_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain-blocks-buffer"),
            size: byte_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&blocks_buffer, 0, bytemuck::cast_slice(&blocks));

        self.frame_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-frame-bind-group"),
            layout: &self.frame_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: blocks_buffer.as_entire_binding(),
                },
            ],
        }));

        self.atlas_bind_groups = atlas_views
            .iter()
            .enumerate()
            .map(|(index, view)| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("terrain-atlas-bind-group-{index}")),
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

    /// Prepares every visible chunk in `terrain` for drawing this frame:
    /// ensures each is in the GPU cache, uploads chunk origins, and evicts
    /// least recently used entries beyond the budget.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        visible: VisibleTileRange,
        terrain: &Terrain,
        assets: &Assets,
    ) {
        self.visible_draws.clear();
        if self.frame_bind_group.is_none() || self.atlas_bind_groups.is_empty() {
            return;
        }
        self.cache.begin_frame(terrain.grid());

        let first_column = visible.min_x.div_euclid(CHUNK_SIZE);
        let last_column = visible.max_x.div_euclid(CHUNK_SIZE);
        let first_row = visible.min_y.div_euclid(CHUNK_SIZE);
        let last_row = visible.max_y.div_euclid(CHUNK_SIZE);
        let width_chunks = terrain.chunks_wide();

        for row in first_row..=last_row {
            for column in first_column..=last_column {
                let chunk = ChunkPos {
                    x: column.rem_euclid(width_chunks),
                    y: row,
                };
                let _ = self.cache.get_or_build(terrain, chunk, || {
                    upload_chunk(device, queue, terrain, assets, chunk)
                });
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "visible chunk origins fit accurately in f32"
                )]
                let origin = [(column * CHUNK_SIZE) as f32, (row * CHUNK_SIZE) as f32];
                self.visible_draws.push(VisibleChunk { chunk, origin });
            }
        }
        self.cache.evict();

        if !self.visible_draws.is_empty() {
            self.upload_chunk_origins(device, queue);
        }
    }

    /// Draws all chunks prepared by [`prepare`](Self::prepare) into `pass`.
    pub(crate) fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        let Some(frame_bind_group) = &self.frame_bind_group else {
            return;
        };
        if self.visible_draws.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, frame_bind_group, &[]);
        let mut bound_atlas: Option<u16> = None;

        for (draw_index, visible) in self.visible_draws.iter().enumerate() {
            let Some(gpu_chunk) = self.cache.get_current(visible.chunk) else {
                continue;
            };
            if gpu_chunk.batches.is_empty() {
                continue;
            }
            let offset = u32::try_from(draw_index).expect("visible chunk count fits in u32")
                * self.chunk_origin_stride;
            pass.set_bind_group(2, &self.chunk_origin_bind_group, &[offset]);
            pass.set_vertex_buffer(0, gpu_chunk.buffer.slice(..));
            for batch in &gpu_chunk.batches {
                if bound_atlas != Some(batch.atlas) {
                    pass.set_bind_group(1, &self.atlas_bind_groups[usize::from(batch.atlas)], &[]);
                    bound_atlas = Some(batch.atlas);
                }
                pass.draw(0..6, batch.start..batch.start + batch.count);
            }
        }
    }

    fn upload_chunk_origins(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let count = self.visible_draws.len();
        if count > self.chunk_origin_capacity {
            let new_capacity = count.next_power_of_two();
            let (buffer, bind_group) = create_chunk_origin_buffer(
                device,
                &self.chunk_bind_group_layout,
                new_capacity,
                self.chunk_origin_stride,
            );
            self.chunk_origin_buffer = buffer;
            self.chunk_origin_bind_group = bind_group;
            self.chunk_origin_capacity = new_capacity;
        }
        let stride = usize::try_from(self.chunk_origin_stride).expect("stride fits in usize");
        self.chunk_origin_staging.resize(count * stride, 0);
        for (index, visible) in self.visible_draws.iter().enumerate() {
            let uniform = ChunkUniform {
                origin: visible.origin,
                _padding: [0.0, 0.0],
            };
            let bytes = bytemuck::bytes_of(&uniform);
            let start = index * stride;
            self.chunk_origin_staging[start..start + bytes.len()].copy_from_slice(bytes);
        }
        queue.write_buffer(&self.chunk_origin_buffer, 0, &self.chunk_origin_staging);
    }
}

fn create_bind_group_layouts(
    device: &wgpu::Device,
) -> (
    wgpu::BindGroupLayout,
    wgpu::BindGroupLayout,
    wgpu::BindGroupLayout,
) {
    let frame = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("terrain-frame-bind-group-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let atlas = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("terrain-atlas-bind-group-layout"),
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
    });
    let chunk = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("terrain-chunk-bind-group-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(size_of::<ChunkUniform>() as u64),
            },
            count: None,
        }],
    });
    (frame, atlas, chunk)
}

fn create_render_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    frame_layout: &wgpu::BindGroupLayout,
    atlas_layout: &wgpu::BindGroupLayout,
    chunk_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("terrain-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("terrain.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("terrain-pipeline-layout"),
        bind_group_layouts: &[Some(frame_layout), Some(atlas_layout), Some(chunk_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("terrain-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(terrain_instance_layout())],
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

fn terrain_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Uint32,
        1 => Uint32,
    ];
    wgpu::VertexBufferLayout {
        array_stride: size_of::<TerrainInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

fn create_chunk_origin_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    capacity: usize,
    stride: u32,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let total_bytes = (capacity as wgpu::BufferAddress) * wgpu::BufferAddress::from(stride);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("terrain-chunk-origins"),
        size: total_bytes.max(wgpu::BufferAddress::from(stride)),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("terrain-chunk-origin-bind-group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: NonZeroU64::new(size_of::<ChunkUniform>() as u64),
            }),
        }],
    });
    (buffer, bind_group)
}

fn upload_chunk(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    terrain: &Terrain,
    assets: &Assets,
    chunk: ChunkPos,
) -> GpuChunk {
    let mesh = build_chunk_mesh(terrain, assets, chunk);
    let byte_size = (mesh.instances.len() * size_of::<TerrainInstance>()) as wgpu::BufferAddress;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("terrain-chunk-instances"),
        size: byte_size.max(size_of::<TerrainInstance>() as wgpu::BufferAddress),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !mesh.instances.is_empty() {
        queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&mesh.instances));
    }
    GpuChunk {
        buffer,
        batches: mesh.batches,
    }
}
