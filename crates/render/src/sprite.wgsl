struct CameraUniform {
    scale: vec2<f32>,
    centre: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;

@group(1) @binding(1)
var atlas_sampler: sampler;

struct SpriteInstance {
    @location(0) origin: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) texel_origin: vec2<f32>,
    @location(3) texel_size: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texel_pos: vec2<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: SpriteInstance,
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

    let world_pos = instance.origin + corner * instance.size;
    let ndc = (world_pos - camera.centre) * camera.scale;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    out.texel_pos = instance.texel_origin + corner * instance.texel_size;
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
