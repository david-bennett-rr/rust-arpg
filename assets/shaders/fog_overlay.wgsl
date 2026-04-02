#import bevy_pbr::forward_io::VertexOutput

@group(2) @binding(0) var memory_texture: texture_2d<f32>;
@group(2) @binding(1) var memory_sampler: sampler;
@group(2) @binding(2) var<uniform> memory_bounds: vec4<f32>;
@group(2) @binding(3) var<uniform> visible_previous_bounds: vec4<f32>;
@group(2) @binding(4) var<uniform> visible_current_bounds: vec4<f32>;
@group(2) @binding(5) var<uniform> visible_mix: vec4<f32>;
@group(2) @binding(6) var visible_previous_texture: texture_2d<f32>;
@group(2) @binding(7) var visible_previous_sampler: sampler;
@group(2) @binding(8) var visible_current_texture: texture_2d<f32>;
@group(2) @binding(9) var visible_current_sampler: sampler;

fn world_to_uv(world_xz: vec2<f32>, bounds: vec4<f32>) -> vec2<f32> {
    let world_size = max(bounds.zw, vec2<f32>(0.0001, 0.0001));
    return clamp((world_xz - bounds.xy) / world_size, vec2<f32>(0.0), vec2<f32>(1.0));
}

fn world_in_bounds(world_xz: vec2<f32>, bounds: vec4<f32>) -> bool {
    let max_corner = bounds.xy + bounds.zw;
    return world_xz.x >= bounds.x
        && world_xz.y >= bounds.y
        && world_xz.x <= max_corner.x
        && world_xz.y <= max_corner.y;
}

fn sample_visible_alpha(
    world_xz: vec2<f32>,
    bounds: vec4<f32>,
    fog_texture: texture_2d<f32>,
    fog_sampler: sampler,
) -> f32 {
    if !world_in_bounds(world_xz, bounds) {
        return 1.0;
    }
    let visible_uv = world_to_uv(world_xz, bounds);
    return textureSample(fog_texture, fog_sampler, visible_uv).a;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let world_xz = mesh.world_position.xz;
    let memory_uv = world_to_uv(world_xz, memory_bounds);
    let memory_fog = textureSample(memory_texture, memory_sampler, memory_uv);
    let previous_alpha = sample_visible_alpha(
        world_xz,
        visible_previous_bounds,
        visible_previous_texture,
        visible_previous_sampler,
    );
    let current_alpha = sample_visible_alpha(
        world_xz,
        visible_current_bounds,
        visible_current_texture,
        visible_current_sampler,
    );
    let mix_alpha = clamp(visible_mix.x, 0.0, 1.0);
    let visible_alpha = previous_alpha + (current_alpha - previous_alpha) * mix_alpha;
    let alpha = min(memory_fog.a, visible_alpha);
    return vec4<f32>(0.0, 0.0, 0.0, alpha);
}
