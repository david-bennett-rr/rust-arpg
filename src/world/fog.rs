use bevy::{
    asset::Asset,
    color::Alpha,
    ecs::system::SystemParam,
    image::ImageSampler,
    pbr::Material,
    prelude::*,
    reflect::TypePath,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{AsBindGroup, Extent3d, ShaderRef, TextureDimension, TextureFormat},
    },
};

use crate::combat::smoothstep01;
use crate::enemy::UniqueEnemyMaterialCache;
use crate::player::Player;
use crate::targeting::TargetState;

use super::tilemap::WallSpatialIndex;

const FOG_REVEAL_RADIUS: f32 = 13.0;
const FOG_REVEAL_RADIUS_SQ: f32 = FOG_REVEAL_RADIUS * FOG_REVEAL_RADIUS;
const FOG_VISIBLE_EDGE_SOFTNESS: f32 = 4.0;
const FOG_EXPLORED_ALPHA: f32 = 0.9;
const FOG_MEMORY_MAX_RESOLUTION: u32 = 320;
const FOG_MEMORY_BLUR_PASSES: usize = 1;
const FOG_MEMORY_ALPHA_RESPONSE_SPEED: f32 = 9.0;
const FOG_VISIBLE_TEXTURE_RESOLUTION: u32 = 144;
const FOG_VISIBLE_TEXTURE_WORLD_SIZE: f32 =
    (FOG_REVEAL_RADIUS + FOG_VISIBLE_EDGE_SOFTNESS + 3.0) * 2.0;
const FOG_VISIBLE_REFRESH_DISTANCE: f32 = 0.60;
const FOG_VISIBLE_REFRESH_DISTANCE_SQ: f32 =
    FOG_VISIBLE_REFRESH_DISTANCE * FOG_VISIBLE_REFRESH_DISTANCE;
const FOG_VISIBLE_REFRESH_INTERVAL: f32 = 0.14;
const FOG_VISIBLE_INTERVAL_MIN_DISTANCE: f32 = 0.18;
const FOG_VISIBLE_INTERVAL_MIN_DISTANCE_SQ: f32 =
    FOG_VISIBLE_INTERVAL_MIN_DISTANCE * FOG_VISIBLE_INTERVAL_MIN_DISTANCE;
const FOG_VISIBLE_TRANSITION_SPEED: f32 = 10.0;
const FOG_OVERLAY_MARGIN: f32 = 10.0;
const FOG_OVERLAY_HEIGHT: f32 = 0.06;
const FOG_BLUR_PASSES: usize = 2;
const FOG_BLUR_RADIUS: i32 = 2;
const FOG_DYNAMIC_FADE_SPEED: f32 = 12.0;
const FOG_DYNAMIC_HIDDEN_ALPHA: f32 = 0.02;
const FOG_SHADER_ASSET_PATH: &str = "shaders/fog_overlay.wgsl";

#[derive(Clone)]
pub(crate) struct FogMaterialSet {
    pub(crate) visible: Handle<StandardMaterial>,
}

impl FogMaterialSet {
    pub(crate) fn new(
        materials: &mut Assets<StandardMaterial>,
        base_srgb: Vec3,
        mut visible: StandardMaterial,
    ) -> Self {
        visible.base_color = Color::srgb(base_srgb.x, base_srgb.y, base_srgb.z);
        Self {
            visible: materials.add(visible),
        }
    }
}

#[derive(Component, Clone)]
pub(crate) struct FogStatic;

impl FogStatic {
    pub(crate) fn point(_sample: Vec2, _materials: FogMaterialSet) -> Self {
        Self
    }

    pub(crate) fn edge(_sample_a: Vec2, _sample_b: Vec2, _materials: FogMaterialSet) -> Self {
        Self
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct FogDynamic {
    alpha: f32,
    applied_alpha: u8,
}

impl Default for FogDynamic {
    fn default() -> Self {
        Self {
            alpha: 0.0,
            applied_alpha: 0,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct FogRuntimeState {
    visible_window_center: Option<Vec2>,
    visible_refresh_elapsed: f32,
}

#[derive(Resource)]
pub(crate) struct FogOverlay {
    material: Handle<FogOverlayMaterial>,
    memory_image: Handle<Image>,
    memory_size: UVec2,
    memory_world_min: Vec2,
    memory_world_size: Vec2,
    memory_alpha: Vec<f32>,
    memory_horizontal: Vec<f32>,
    visible_previous_image: Handle<Image>,
    visible_current_image: Handle<Image>,
    visible_size: UVec2,
    visible_previous_world_min: Vec2,
    visible_current_world_min: Vec2,
    visible_world_size: Vec2,
    visible_transition: f32,
    visible_alpha: Vec<f32>,
    visible_scratch: Vec<f32>,
    visible_horizontal: Vec<f32>,
    memory_target_alpha: Vec<f32>,
    memory_scratch: Vec<f32>,
    explored: Vec<bool>,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(crate) struct FogOverlayMaterial {
    #[texture(0)]
    #[sampler(1)]
    memory_texture: Handle<Image>,
    #[uniform(2)]
    memory_bounds: Vec4,
    #[uniform(3)]
    visible_previous_bounds: Vec4,
    #[uniform(4)]
    visible_current_bounds: Vec4,
    #[uniform(5)]
    visible_mix: Vec4,
    #[texture(6)]
    #[sampler(7)]
    visible_previous_texture: Handle<Image>,
    #[texture(8)]
    #[sampler(9)]
    visible_current_texture: Handle<Image>,
    alpha_mode: AlphaMode,
}

impl Material for FogOverlayMaterial {
    fn fragment_shader() -> ShaderRef {
        FOG_SHADER_ASSET_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
}

pub(crate) fn spawn_fog_overlay(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    fog_materials: &mut Assets<FogOverlayMaterial>,
    images: &mut Assets<Image>,
    center: Vec3,
    half_x: f32,
    half_z: f32,
) {
    let memory_world_size = Vec2::new(
        half_x * 2.0 + FOG_OVERLAY_MARGIN,
        half_z * 2.0 + FOG_OVERLAY_MARGIN,
    );
    let memory_size = overlay_resolution(memory_world_size, FOG_MEMORY_MAX_RESOLUTION);
    let memory_image_handle = images.add(new_overlay_image(memory_size));

    let visible_world_size = Vec2::splat(FOG_VISIBLE_TEXTURE_WORLD_SIZE);
    let visible_size = UVec2::splat(FOG_VISIBLE_TEXTURE_RESOLUTION);
    let visible_previous_image_handle = images.add(new_overlay_image(visible_size));
    let visible_current_image_handle = images.add(new_overlay_image(visible_size));

    let memory_world_min = Vec2::new(
        center.x - memory_world_size.x * 0.5,
        center.z - memory_world_size.y * 0.5,
    );
    let visible_world_min = Vec2::new(
        center.x - visible_world_size.x * 0.5,
        center.z - visible_world_size.y * 0.5,
    );
    let material = fog_materials.add(FogOverlayMaterial {
        memory_texture: memory_image_handle.clone(),
        memory_bounds: Vec4::new(
            memory_world_min.x,
            memory_world_min.y,
            memory_world_size.x,
            memory_world_size.y,
        ),
        visible_previous_bounds: Vec4::new(
            visible_world_min.x,
            visible_world_min.y,
            visible_world_size.x,
            visible_world_size.y,
        ),
        visible_current_bounds: Vec4::new(
            visible_world_min.x,
            visible_world_min.y,
            visible_world_size.x,
            visible_world_size.y,
        ),
        visible_mix: Vec4::splat(1.0),
        visible_previous_texture: visible_previous_image_handle.clone(),
        visible_current_texture: visible_current_image_handle.clone(),
        alpha_mode: AlphaMode::Blend,
    });

    commands.insert_resource(FogOverlay {
        material: material.clone(),
        memory_image: memory_image_handle,
        memory_size,
        memory_world_min,
        memory_world_size,
        memory_alpha: vec![1.0; (memory_size.x * memory_size.y) as usize],
        memory_horizontal: vec![0.0; (memory_size.x * memory_size.y) as usize],
        visible_previous_image: visible_previous_image_handle,
        visible_current_image: visible_current_image_handle,
        visible_size,
        visible_previous_world_min: visible_world_min,
        visible_current_world_min: visible_world_min,
        visible_world_size,
        visible_transition: 1.0,
        visible_alpha: vec![1.0; (visible_size.x * visible_size.y) as usize],
        visible_scratch: vec![0.0; (visible_size.x * visible_size.y) as usize],
        visible_horizontal: vec![0.0; (visible_size.x * visible_size.y) as usize],
        memory_target_alpha: vec![1.0; (memory_size.x * memory_size.y) as usize],
        memory_scratch: vec![0.0; (memory_size.x * memory_size.y) as usize],
        explored: vec![false; (memory_size.x * memory_size.y) as usize],
    });

    commands.spawn((
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(memory_world_size.x, memory_world_size.y),
            ),
        ),
        MeshMaterial3d(material),
        Transform::from_xyz(center.x, FOG_OVERLAY_HEIGHT, center.z),
    ));
}

pub(crate) fn update_static_fog(
    player: Option<Single<&Transform, With<Player>>>,
    time: Res<Time>,
    mut fog_runtime: ResMut<FogRuntimeState>,
    wall_index: Res<WallSpatialIndex>,
    overlay: Option<ResMut<FogOverlay>>,
    mut fog_materials: ResMut<Assets<FogOverlayMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(player) = player else {
        return;
    };
    let Some(mut overlay) = overlay else {
        return;
    };

    let player_ground = horizontal(player.translation);
    let initializing = fog_runtime.visible_window_center.is_none();
    fog_runtime.visible_refresh_elapsed += time.delta_secs();

    let visible_width = overlay.visible_size.x as usize;
    let visible_height = overlay.visible_size.y as usize;
    if should_refresh_visible_mask(
        fog_runtime.visible_window_center,
        player_ground,
        fog_runtime.visible_refresh_elapsed,
    ) {
        fog_runtime.visible_window_center = Some(player_ground);
        fog_runtime.visible_refresh_elapsed = 0.0;
        let next_visible_world_min = snap_world_min_to_texels(
            player_ground - overlay.visible_world_size * 0.5,
            overlay.visible_world_size,
            overlay.visible_size,
        );

        overlay.visible_alpha.fill(1.0);
        for y in 0..visible_height {
            for x in 0..visible_width {
                let idx = y * visible_width + x;
                let world = pixel_to_world(
                    next_visible_world_min,
                    overlay.visible_world_size,
                    overlay.visible_size,
                    x as u32,
                    y as u32,
                );
                let visible_strength =
                    sample_visibility_strength(player_ground, world, &wall_index);
                if visible_strength > 0.01 {
                    if let Some(memory_idx) = overlay.memory_index_for_world(world) {
                        overlay.explored[memory_idx] = true;
                    }
                }

                overlay.visible_alpha[idx] = 1.0 - visible_strength;
            }
        }

        {
            let FogOverlay {
                visible_alpha,
                visible_scratch,
                visible_horizontal,
                ..
            } = &mut *overlay;
            blur_alpha_passes(
                visible_alpha,
                visible_scratch,
                visible_horizontal,
                visible_width,
                visible_height,
                FOG_BLUR_PASSES,
            );
        }

        if initializing {
            overlay.visible_previous_world_min = next_visible_world_min;
            overlay.visible_current_world_min = next_visible_world_min;
            write_alpha_image(
                &mut images,
                &overlay.visible_previous_image,
                &overlay.visible_alpha,
            );
            write_alpha_image(
                &mut images,
                &overlay.visible_current_image,
                &overlay.visible_alpha,
            );
            overlay.visible_transition = 1.0;
        } else {
            let previous_image = overlay.visible_previous_image.clone();
            overlay.visible_previous_image = overlay.visible_current_image.clone();
            overlay.visible_current_image = previous_image;
            overlay.visible_previous_world_min = overlay.visible_current_world_min;
            overlay.visible_current_world_min = next_visible_world_min;
            write_alpha_image(
                &mut images,
                &overlay.visible_current_image,
                &overlay.visible_alpha,
            );
            overlay.visible_transition = 0.0;
        }

        let memory_width = overlay.memory_size.x as usize;
        let memory_height = overlay.memory_size.y as usize;
        overlay.memory_target_alpha.fill(1.0);
        for idx in 0..overlay.memory_target_alpha.len() {
            overlay.memory_target_alpha[idx] = if overlay.explored[idx] {
                FOG_EXPLORED_ALPHA
            } else {
                1.0
            };
        }

        {
            let FogOverlay {
                memory_target_alpha,
                memory_scratch,
                memory_horizontal,
                ..
            } = &mut *overlay;
            blur_alpha_passes(
                memory_target_alpha,
                memory_scratch,
                memory_horizontal,
                memory_width,
                memory_height,
                FOG_MEMORY_BLUR_PASSES,
            );
        }
    }

    overlay.visible_transition =
        (overlay.visible_transition + time.delta_secs() * FOG_VISIBLE_TRANSITION_SPEED).min(1.0);
    if let Some(material) = fog_materials.get_mut(&overlay.material) {
        material.visible_previous_bounds = Vec4::new(
            overlay.visible_previous_world_min.x,
            overlay.visible_previous_world_min.y,
            overlay.visible_world_size.x,
            overlay.visible_world_size.y,
        );
        material.visible_current_bounds = Vec4::new(
            overlay.visible_current_world_min.x,
            overlay.visible_current_world_min.y,
            overlay.visible_world_size.x,
            overlay.visible_world_size.y,
        );
        material.visible_mix = Vec4::splat(overlay.visible_transition);
        material.visible_previous_texture = overlay.visible_previous_image.clone();
        material.visible_current_texture = overlay.visible_current_image.clone();
    }

    let memory_blend =
        response_blend(FOG_MEMORY_ALPHA_RESPONSE_SPEED, time.delta_secs()).clamp(0.0, 1.0);
    let mut memory_image_changed = false;
    for idx in 0..overlay.memory_alpha.len() {
        let target = overlay.memory_target_alpha[idx];
        let current = &mut overlay.memory_alpha[idx];
        let previous_byte = alpha_to_byte(*current);
        *current += (target - *current) * memory_blend;
        memory_image_changed |= previous_byte != alpha_to_byte(*current);
    }

    if memory_image_changed {
        write_alpha_image(&mut images, &overlay.memory_image, &overlay.memory_alpha);
    }
}

pub(crate) type FoggedQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GlobalTransform,
        &'static mut Visibility,
        &'static mut FogDynamic,
        Option<&'static UniqueEnemyMaterialCache>,
    ),
    With<FogDynamic>,
>;

#[derive(SystemParam)]
pub(crate) struct DynamicFogContext<'w, 's> {
    player: Option<Single<'w, &'static Transform, With<Player>>>,
    time: Res<'w, Time>,
    fog_runtime: Res<'w, FogRuntimeState>,
    wall_index: Res<'w, WallSpatialIndex>,
    target_state: Option<ResMut<'w, TargetState>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    fogged: FoggedQuery<'w, 's>,
}

pub(crate) fn update_dynamic_fog(mut ctx: DynamicFogContext<'_, '_>) {
    let Some(player) = ctx.player else {
        return;
    };
    let player_ground = horizontal(player.translation);
    let visibility_origin = ctx
        .fog_runtime
        .visible_window_center
        .unwrap_or(player_ground);
    let fade_blend = response_blend(FOG_DYNAMIC_FADE_SPEED, ctx.time.delta_secs()).clamp(0.0, 1.0);

    for (entity, global_transform, mut visibility, mut fog_dynamic, material_cache) in
        &mut ctx.fogged
    {
        let target_visible = sample_visible(
            visibility_origin,
            horizontal(global_transform.translation()),
            &ctx.wall_index,
        );
        if let Some(material_cache) = material_cache {
            let target_alpha = if target_visible { 1.0 } else { 0.0 };
            fog_dynamic.alpha += (target_alpha - fog_dynamic.alpha) * fade_blend;
            let display_alpha = fog_dynamic.alpha.clamp(0.0, 1.0);
            let alpha_byte = alpha_to_byte(display_alpha);

            if target_visible || display_alpha > FOG_DYNAMIC_HIDDEN_ALPHA {
                *visibility = Visibility::Visible;
            }
            if alpha_byte != fog_dynamic.applied_alpha {
                apply_fog_alpha_to_materials(
                    &material_cache.handles,
                    display_alpha,
                    &mut ctx.materials,
                );
                fog_dynamic.applied_alpha = alpha_byte;
            }
            if !target_visible && display_alpha <= FOG_DYNAMIC_HIDDEN_ALPHA {
                *visibility = Visibility::Hidden;
            }
        } else {
            *visibility = if target_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }

        let Some(target_state) = ctx.target_state.as_mut() else {
            continue;
        };
        if !target_visible && target_state.hovered == Some(entity) {
            target_state.hovered = None;
        }
        if !target_visible && target_state.targeted == Some(entity) {
            target_state.targeted = None;
        }
    }
}

impl FogOverlay {
    fn memory_index_for_world(&self, world: Vec2) -> Option<usize> {
        let pixel = world_to_pixel(
            self.memory_world_min,
            self.memory_world_size,
            self.memory_size,
            world,
        )?;
        Some((pixel.y * self.memory_size.x + pixel.x) as usize)
    }
}

fn overlay_resolution(world_size: Vec2, max_resolution: u32) -> UVec2 {
    let longest = world_size.x.max(world_size.y).max(1.0);
    let scale = max_resolution as f32 / longest;
    UVec2::new(
        (world_size.x * scale).round().max(32.0) as u32,
        (world_size.y * scale).round().max(32.0) as u32,
    )
}

fn new_overlay_image(size: UVec2) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

fn write_alpha_image(images: &mut Assets<Image>, handle: &Handle<Image>, alpha: &[f32]) {
    let Some(image) = images.get_mut(handle) else {
        return;
    };
    for (pixel, &value) in image.data.chunks_exact_mut(4).zip(alpha.iter()) {
        pixel[0] = 0;
        pixel[1] = 0;
        pixel[2] = 0;
        pixel[3] = alpha_to_byte(value);
    }
}

fn alpha_to_byte(alpha: f32) -> u8 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn pixel_to_world(world_min: Vec2, world_size: Vec2, size: UVec2, x: u32, y: u32) -> Vec2 {
    let fx = (x as f32 + 0.5) / size.x as f32;
    let fy = (y as f32 + 0.5) / size.y as f32;
    world_min + Vec2::new(fx * world_size.x, fy * world_size.y)
}

fn world_to_pixel(world_min: Vec2, world_size: Vec2, size: UVec2, world: Vec2) -> Option<UVec2> {
    let relative = world - world_min;
    if relative.x < 0.0
        || relative.y < 0.0
        || relative.x >= world_size.x
        || relative.y >= world_size.y
    {
        return None;
    }

    let fx = (relative.x / world_size.x).clamp(0.0, 0.999_999);
    let fy = (relative.y / world_size.y).clamp(0.0, 0.999_999);
    Some(UVec2::new(
        (fx * size.x as f32).floor() as u32,
        (fy * size.y as f32).floor() as u32,
    ))
}

fn response_blend(speed: f32, delta_secs: f32) -> f32 {
    if speed <= 0.0 || delta_secs <= 0.0 {
        return 0.0;
    }
    1.0 - (-speed * delta_secs).exp()
}

fn should_refresh_visible_mask(
    current_center: Option<Vec2>,
    player_ground: Vec2,
    elapsed: f32,
) -> bool {
    let Some(current_center) = current_center else {
        return true;
    };
    let distance_sq = current_center.distance_squared(player_ground);
    distance_sq >= FOG_VISIBLE_REFRESH_DISTANCE_SQ
        || (elapsed >= FOG_VISIBLE_REFRESH_INTERVAL
            && distance_sq >= FOG_VISIBLE_INTERVAL_MIN_DISTANCE_SQ)
}

fn snap_world_min_to_texels(world_min: Vec2, world_size: Vec2, size: UVec2) -> Vec2 {
    let texel_size = Vec2::new(world_size.x / size.x as f32, world_size.y / size.y as f32);
    Vec2::new(
        (world_min.x / texel_size.x).round() * texel_size.x,
        (world_min.y / texel_size.y).round() * texel_size.y,
    )
}

fn apply_fog_alpha_to_materials(
    handles: &[Handle<StandardMaterial>],
    alpha: f32,
    materials: &mut Assets<StandardMaterial>,
) {
    let alpha_mode = if alpha < 0.999 {
        AlphaMode::Blend
    } else {
        AlphaMode::Opaque
    };
    for handle in handles {
        let Some(material) = materials.get_mut(handle) else {
            continue;
        };
        material.alpha_mode = alpha_mode.clone();
        material.base_color.set_alpha(alpha);
    }
}

fn blur_alpha_passes(
    buffer: &mut [f32],
    scratch: &mut [f32],
    horizontal: &mut [f32],
    width: usize,
    height: usize,
    passes: usize,
) {
    if passes == 0 {
        return;
    }

    let mut source_is_buffer = true;
    for _ in 0..passes {
        if source_is_buffer {
            blur_alpha_once(buffer, scratch, horizontal, width, height);
        } else {
            blur_alpha_once(scratch, buffer, horizontal, width, height);
        }
        source_is_buffer = !source_is_buffer;
    }

    if !source_is_buffer {
        buffer.copy_from_slice(scratch);
    }
}

fn blur_alpha_once(
    source: &[f32],
    out: &mut [f32],
    horizontal: &mut [f32],
    width: usize,
    height: usize,
) {
    for y in 0..height {
        for x in 0..width {
            let mut total = 0.0;
            let mut weight = 0.0;

            for ox in -FOG_BLUR_RADIUS..=FOG_BLUR_RADIUS {
                let sx = (x as i32 + ox).clamp(0, width as i32 - 1) as usize;
                let kernel = 1.0 / (1.0 + ox.abs() as f32);
                total += source[y * width + sx] * kernel;
                weight += kernel;
            }

            horizontal[y * width + x] = if weight > 0.0 {
                total / weight
            } else {
                source[y * width + x]
            };
        }
    }

    for y in 0..height {
        for x in 0..width {
            let mut total = 0.0;
            let mut weight = 0.0;

            for oy in -FOG_BLUR_RADIUS..=FOG_BLUR_RADIUS {
                let sy = (y as i32 + oy).clamp(0, height as i32 - 1) as usize;
                let kernel = 1.0 / (1.0 + oy.abs() as f32);
                total += horizontal[sy * width + x] * kernel;
                weight += kernel;
            }

            out[y * width + x] = if weight > 0.0 {
                total / weight
            } else {
                horizontal[y * width + x]
            };
        }
    }
}

fn sample_visible(player_ground: Vec2, sample: Vec2, wall_index: &WallSpatialIndex) -> bool {
    sample_visibility_strength(player_ground, sample, wall_index) > 0.0
}

fn sample_visibility_strength(
    player_ground: Vec2,
    sample: Vec2,
    wall_index: &WallSpatialIndex,
) -> f32 {
    let delta = sample - player_ground;
    let distance_sq = delta.length_squared();
    if distance_sq > FOG_REVEAL_RADIUS_SQ {
        return 0.0;
    }

    if distance_sq > 0.0001 && !wall_index.segment_clear(player_ground, sample, 0.0) {
        return 0.0;
    }

    let distance = distance_sq.sqrt();
    let edge_start = (FOG_REVEAL_RADIUS - FOG_VISIBLE_EDGE_SOFTNESS).max(0.0);
    if distance <= edge_start {
        return 1.0;
    }

    smoothstep01(((FOG_REVEAL_RADIUS - distance) / FOG_VISIBLE_EDGE_SOFTNESS).clamp(0.0, 1.0))
}

fn horizontal(point: Vec3) -> Vec2 {
    Vec2::new(point.x, point.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tilemap::{WallSegment, WallSpatialIndex};

    #[test]
    fn samples_outside_radius_are_unseen() {
        let wall_index = WallSpatialIndex::default();

        assert!(!sample_visible(
            Vec2::ZERO,
            Vec2::new(FOG_REVEAL_RADIUS + 1.0, 0.0),
            &wall_index,
        ));
    }

    #[test]
    fn walls_block_visibility() {
        let mut wall_index = WallSpatialIndex::default();
        wall_index.rebuild(vec![WallSegment {
            center: Vec2::new(5.0, 0.0),
            half_extents: Vec2::new(1.0, 1.0),
        }]);

        assert!(!sample_visible(
            Vec2::ZERO,
            Vec2::new(10.0, 0.0),
            &wall_index,
        ));
    }

    #[test]
    fn visibility_softens_near_radius_edge() {
        let wall_index = WallSpatialIndex::default();
        let strength = sample_visibility_strength(
            Vec2::ZERO,
            Vec2::new(FOG_REVEAL_RADIUS - FOG_VISIBLE_EDGE_SOFTNESS * 0.5, 0.0),
            &wall_index,
        );

        assert!(strength > 0.0);
        assert!(strength < 1.0);
    }

    #[test]
    fn blur_preserves_midtones() {
        let mut buffer = vec![1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        let mut scratch = vec![0.0; buffer.len()];
        let mut horizontal = vec![0.0; buffer.len()];
        blur_alpha_passes(&mut buffer, &mut scratch, &mut horizontal, 3, 3, 1);

        assert!(buffer[4] > 0.0);
        assert!(buffer[4] < 1.0);
    }

    #[test]
    fn visible_mask_refreshes_after_meaningful_movement() {
        assert!(should_refresh_visible_mask(
            Some(Vec2::ZERO),
            Vec2::new(FOG_VISIBLE_REFRESH_DISTANCE, 0.0),
            0.0,
        ));
    }

    #[test]
    fn visible_mask_skips_tiny_motion_before_interval() {
        assert!(!should_refresh_visible_mask(
            Some(Vec2::ZERO),
            Vec2::new(0.02, 0.0),
            FOG_VISIBLE_REFRESH_INTERVAL * 0.5,
        ));
    }

    #[test]
    fn visible_mask_skips_tiny_motion_even_after_interval() {
        assert!(!should_refresh_visible_mask(
            Some(Vec2::ZERO),
            Vec2::new(FOG_VISIBLE_INTERVAL_MIN_DISTANCE * 0.5, 0.0),
            FOG_VISIBLE_REFRESH_INTERVAL,
        ));
    }
}
