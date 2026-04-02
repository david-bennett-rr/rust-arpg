use smallvec::SmallVec;

use bevy::{
    asset::Asset,
    color::Alpha,
    image::ImageSampler,
    pbr::Material,
    prelude::*,
    reflect::TypePath,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{AsBindGroup, Extent3d, ShaderRef, TextureDimension, TextureFormat},
    },
};

use crate::enemy::UniqueEnemyMaterials;
use crate::player::Player;
use crate::targeting::TargetState;

use super::tilemap::{WallSpatialIndex, TILE_SIZE};

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
pub(crate) struct FogStatic {
    #[allow(dead_code)]
    samples: SmallVec<[Vec2; 5]>,
}

impl FogStatic {
    pub(crate) fn point(sample: Vec2, _materials: FogMaterialSet) -> Self {
        Self {
            samples: point_samples(sample),
        }
    }

    pub(crate) fn edge(sample_a: Vec2, sample_b: Vec2, _materials: FogMaterialSet) -> Self {
        Self {
            samples: edge_samples(sample_a, sample_b),
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct FogDynamic {
    alpha: f32,
}

impl Default for FogDynamic {
    fn default() -> Self {
        Self { alpha: 0.0 }
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
    visible_previous_image: Handle<Image>,
    visible_current_image: Handle<Image>,
    visible_size: UVec2,
    visible_previous_world_min: Vec2,
    visible_current_world_min: Vec2,
    visible_world_size: Vec2,
    visible_transition: f32,
    memory_target_alpha: Vec<f32>,
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
        visible_previous_image: visible_previous_image_handle,
        visible_current_image: visible_current_image_handle,
        visible_size,
        visible_previous_world_min: visible_world_min,
        visible_current_world_min: visible_world_min,
        visible_world_size,
        visible_transition: 1.0,
        memory_target_alpha: vec![1.0; (memory_size.x * memory_size.y) as usize],
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

        let mut target_visible_alpha = vec![1.0_f32; visible_width * visible_height];
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

                target_visible_alpha[idx] = 1.0 - visible_strength;
            }
        }

        for _ in 0..FOG_BLUR_PASSES {
            target_visible_alpha = blur_alpha(&target_visible_alpha, visible_width, visible_height);
        }

        if initializing {
            overlay.visible_previous_world_min = next_visible_world_min;
            overlay.visible_current_world_min = next_visible_world_min;
            write_alpha_image(
                &mut images,
                &overlay.visible_previous_image,
                &target_visible_alpha,
                visible_width,
                visible_height,
            );
            write_alpha_image(
                &mut images,
                &overlay.visible_current_image,
                &target_visible_alpha,
                visible_width,
                visible_height,
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
                &target_visible_alpha,
                visible_width,
                visible_height,
            );
            overlay.visible_transition = 0.0;
        }

        let memory_width = overlay.memory_size.x as usize;
        let memory_height = overlay.memory_size.y as usize;
        let mut target_memory_alpha = vec![1.0_f32; memory_width * memory_height];
        for (idx, target) in target_memory_alpha.iter_mut().enumerate() {
            *target = if overlay.explored[idx] {
                FOG_EXPLORED_ALPHA
            } else {
                1.0
            };
        }

        for _ in 0..FOG_MEMORY_BLUR_PASSES {
            target_memory_alpha = blur_alpha(&target_memory_alpha, memory_width, memory_height);
        }
        overlay.memory_target_alpha = target_memory_alpha;
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

    let Some(memory_image) = images.get_mut(&overlay.memory_image) else {
        return;
    };
    let memory_width = overlay.memory_size.x as usize;
    let memory_height = overlay.memory_size.y as usize;

    let memory_blend =
        response_blend(FOG_MEMORY_ALPHA_RESPONSE_SPEED, time.delta_secs()).clamp(0.0, 1.0);
    for idx in 0..overlay.memory_alpha.len() {
        let target = overlay.memory_target_alpha[idx];
        let current = &mut overlay.memory_alpha[idx];
        *current += (target - *current) * memory_blend;
    }

    for y in 0..memory_height {
        for x in 0..memory_width {
            let idx = y * memory_width + x;
            let pixel = memory_image
                .pixel_bytes_mut(UVec3::new(x as u32, y as u32, 0))
                .unwrap();
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
            pixel[3] = (overlay.memory_alpha[idx].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

pub(crate) fn update_dynamic_fog(
    player: Option<Single<&Transform, With<Player>>>,
    time: Res<Time>,
    fog_runtime: Res<FogRuntimeState>,
    wall_index: Res<WallSpatialIndex>,
    mut target_state: Option<ResMut<TargetState>>,
    children_query: Query<&Children>,
    material_handles: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fogged: Query<
        (
            Entity,
            &GlobalTransform,
            &mut Visibility,
            &mut FogDynamic,
            Has<UniqueEnemyMaterials>,
        ),
        With<FogDynamic>,
    >,
) {
    let Some(player) = player else {
        return;
    };
    let player_ground = horizontal(player.translation);
    let visibility_origin = fog_runtime.visible_window_center.unwrap_or(player_ground);
    let fade_blend = response_blend(FOG_DYNAMIC_FADE_SPEED, time.delta_secs()).clamp(0.0, 1.0);

    for (entity, global_transform, mut visibility, mut fog_dynamic, has_unique_materials) in
        &mut fogged
    {
        let target_visible = sample_visible(
            visibility_origin,
            horizontal(global_transform.translation()),
            &wall_index,
        );
        if has_unique_materials {
            let target_alpha = if target_visible { 1.0 } else { 0.0 };
            fog_dynamic.alpha += (target_alpha - fog_dynamic.alpha) * fade_blend;
            let display_alpha = fog_dynamic.alpha.clamp(0.0, 1.0);

            if target_visible || display_alpha > FOG_DYNAMIC_HIDDEN_ALPHA {
                *visibility = Visibility::Visible;
            }
            apply_fog_alpha_to_entity(
                entity,
                display_alpha,
                &children_query,
                &material_handles,
                &mut materials,
            );
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

        let Some(target_state) = target_state.as_mut() else {
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

fn write_alpha_image(
    images: &mut Assets<Image>,
    handle: &Handle<Image>,
    alpha: &[f32],
    width: usize,
    height: usize,
) {
    let Some(image) = images.get_mut(handle) else {
        return;
    };
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let pixel = image
                .pixel_bytes_mut(UVec3::new(x as u32, y as u32, 0))
                .unwrap();
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
            pixel[3] = (alpha[idx].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
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
        return 1.0;
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

fn apply_fog_alpha_to_entity(
    root: Entity,
    alpha: f32,
    children_query: &Query<&Children>,
    material_handles: &Query<&MeshMaterial3d<StandardMaterial>>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Ok(material_handle) = material_handles.get(entity) {
            let Some(material) = materials.get_mut(&material_handle.0) else {
                continue;
            };
            material.alpha_mode = if alpha < 0.999 {
                AlphaMode::Blend
            } else {
                AlphaMode::Opaque
            };
            material.base_color.set_alpha(alpha);
        }

        if let Ok(children) = children_query.get(entity) {
            stack.extend(children.iter().copied());
        }
    }
}

fn blur_alpha(source: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut horizontal = vec![0.0; source.len()];
    let mut out = vec![0.0; source.len()];

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

    out
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

fn point_samples(center: Vec2) -> SmallVec<[Vec2; 5]> {
    let offset = TILE_SIZE * 0.32;
    smallvec::smallvec![
        center,
        center + Vec2::X * offset,
        center - Vec2::X * offset,
        center + Vec2::Y * offset,
        center - Vec2::Y * offset,
    ]
}

fn edge_samples(start: Vec2, end: Vec2) -> SmallVec<[Vec2; 5]> {
    smallvec::smallvec![
        start.lerp(end, 0.0),
        start.lerp(end, 0.25),
        start.lerp(end, 0.5),
        start.lerp(end, 0.75),
        start.lerp(end, 1.0),
    ]
}

fn horizontal(point: Vec3) -> Vec2 {
    Vec2::new(point.x, point.z)
}

fn smoothstep01(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
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
        let source = vec![1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        let blurred = blur_alpha(&source, 3, 3);

        assert!(blurred[4] > 0.0);
        assert!(blurred[4] < 1.0);
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
