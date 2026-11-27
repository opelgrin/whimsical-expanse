struct CameraUniform {
    scale: vec2<f32>,
    centre: vec2<f32>,
};

struct BlockInfo {
    // Top-left pixel (inside padding) of the first frame of each seasonal
    // variant: spring, summer, fall, winter.
    variants: array<vec2<u32>, 4>,
};

struct ChunkUniform {
    origin: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(0) @binding(1)
var<storage, read> blocks: array<BlockInfo>;

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;

@group(1) @binding(1)
var atlas_sampler: sampler;

@group(2) @binding(0)
var<uniform> chunk: ChunkUniform;

struct TerrainInstance {
    @location(0) tile_and_slot: u32,
    @location(1) block_and_flags: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texel_pos: vec2<f32>,
};

const TILE_SIZE: f32 = 16.0;
const CELL_SIZE: f32 = 20.0;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: TerrainInstance,
) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vertex_index];

    let local_x = f32(instance.tile_and_slot & 0xFFu);
    let local_y = f32((instance.tile_and_slot >> 8u) & 0xFFu);
    let slot_col = f32((instance.tile_and_slot >> 16u) & 0xFFu);
    let slot_row = f32((instance.tile_and_slot >> 24u) & 0xFFu);

    let block_index = instance.block_and_flags & 0xFFFFu;
    // Variant 0 until M1 item 6 selects the seasonal variant per tile.
    let block_origin = vec2<f32>(blocks[block_index].variants[0]);
    let tile_origin = block_origin + vec2<f32>(slot_col, slot_row) * CELL_SIZE;

    let world_pos = chunk.origin + vec2<f32>(local_x, local_y) + corner;
    let ndc = (world_pos - camera.centre) * camera.scale;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    out.texel_pos = tile_origin + corner * TILE_SIZE;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let atlas_size = vec2<f32>(textureDimensions(atlas_texture, 0));
    let uv = in.texel_pos / atlas_size;
    let ddx = dpdx(uv);
    let ddy = dpdy(uv);

    // Art pixel centres lie at integer + 0.5 in texel coordinates. Blend only
    // across the single screen pixel where two art pixels meet when zoomed in;
    // at 1x and below (fw >= 1.0) this reduces to linear + mipmap sampling.
    let fw = max(fwidth(in.texel_pos), vec2<f32>(1e-5));
    let box_size = min(fw, vec2<f32>(1.0));
    let shifted = in.texel_pos - vec2<f32>(0.5);
    let frac = fract(shifted);
    let sharp = floor(shifted)
        + vec2<f32>(0.5)
        + clamp(
            (frac - vec2<f32>(0.5)) / box_size + vec2<f32>(0.5),
            vec2<f32>(0.0),
            vec2<f32>(1.0),
        );
    return textureSampleGrad(atlas_texture, atlas_sampler, sharp / atlas_size, ddx, ddy);
}
