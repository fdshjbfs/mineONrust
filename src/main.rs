use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::asset::{LoadState, RenderAssetUsages};
use bevy::pbr::DistanceFog;
use bevy::window::{CursorGrabMode, CursorOptions, MonitorSelection, WindowMode};
use std::sync::{mpsc, Mutex};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const WORLD_SIZE: i32 = 512;
const WORLD_HEIGHT: i32 = 96;
const SEA_LEVEL: i32 = 27;
const RENDER_RADIUS: i32 = 160;
const REACH: f32 = 7.0;
const PLAYER_RADIUS: f32 = 0.30;
const PLAYER_HEIGHT: f32 = 1.80;
const PLAYER_EYE_HEIGHT: f32 = 1.62;

const GRASS: u8 = 2;
const DIRT: u8 = 3;
const STONE: u8 = 4;

#[derive(Resource)]
struct VoxelWorld {
    blocks: Vec<u8>,
    seed: i64,
}

impl VoxelWorld {
    fn new() -> Self {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos() as i64).unwrap_or(42);
        let mut blocks = vec![0; (WORLD_SIZE * WORLD_HEIGHT * WORLD_SIZE) as usize];
        for x in 0..WORLD_SIZE {
            for z in 0..WORLD_SIZE {
                let height = terrain_height(seed, x, z);
                for y in 0..=height {
                    let cave_strength = cave_value(seed, x, y, z);
                    let underground_cave = y > 7 && y < height - 4 && cave_strength > 0.76;
                    let surface_opening = y >= height - 2 && cave_strength > 0.91 && hash(seed, x + 41, z - 17) > 0.72;
                    let is_cave = underground_cave || surface_opening;
                    if !is_cave {
                        let mountain_surface = height > SEA_LEVEL + 24;
                        set_block(&mut blocks, x, y, z, if y == height && !mountain_surface { GRASS } else if y > height - 4 && !mountain_surface { DIRT } else { STONE });
                    }
                }
            }
        }
        Self { blocks, seed }
    }

    fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if x < 0 || y < 0 || z < 0 || x >= WORLD_SIZE || y >= WORLD_HEIGHT || z >= WORLD_SIZE { return 0; }
        self.blocks[index(x, y, z)]
    }
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct WorldMesh;

#[derive(Component, Clone, Copy, PartialEq)]
enum FaceGroup { GrassTop, GrassSide, Dirt, Stone }

#[derive(Resource)]
struct TextureHandles {
    top: Handle<Image>,
    side: Handle<Image>,
    bottom: Handle<Image>,
    stone: Handle<Image>,
    reported: bool,
}

#[derive(Resource, Default)]
struct LookState { pitch: f32, yaw: f32 }

#[derive(Resource)]
struct PlayerState {
    flying: bool,
    velocity: Vec3,
    last_space: f32,
}

#[derive(Resource)]
struct Inventory {
    slots: [u32; 9],
    selected: usize,
}

#[derive(Resource, Default)]
struct MiningState { target: Option<IVec3>, progress: f32 }

#[derive(Component)]
struct Hotbar;

#[derive(Component)]
struct MiningProgressBar;

#[derive(Component)]
struct BreakParticle { velocity: Vec3, lifetime: f32 }

#[derive(Resource)]
struct MeshRebuildQueue {
    sender: Sender<[Mesh; 4]>,
    receiver: Mutex<Receiver<[Mesh; 4]>>,
    running: bool,
}

fn index(x: i32, y: i32, z: i32) -> usize {
    ((y * WORLD_SIZE + z) * WORLD_SIZE + x) as usize
}

fn set_block(blocks: &mut [u8], x: i32, y: i32, z: i32, value: u8) {
    blocks[index(x, y, z)] = value;
}

fn hash(seed: i64, x: i32, z: i32) -> f32 {
    let mut value = seed.wrapping_add((x as i64).wrapping_mul(374_761_393)).wrapping_add((z as i64).wrapping_mul(668_265_263));
    value = (value ^ (value >> 13)).wrapping_mul(1_274_126_177);
    ((value ^ (value >> 16)) & 0xffff) as f32 / 65_535.0
}

fn cave_value(seed: i64, x: i32, y: i32, z: i32) -> f32 {
    let tunnel = ((x as f32 * 0.045).sin() + (z as f32 * 0.052).cos() + (y as f32 * 0.11).sin()).abs() / 3.0;
    let chamber = ((x as f32 * 0.021).sin() * (z as f32 * 0.025).cos() * (y as f32 * 0.07).sin()).abs();
    let noise = hash(seed, x / 2 + y * 13, z / 2 - y * 19);
    (tunnel * 0.35 + chamber * 0.45 + noise * 0.20).clamp(0.0, 1.0)
}

fn terrain_height(seed: i64, x: i32, z: i32) -> i32 {
    let continental = (((x as f32 + (seed & 255) as f32) * 0.0035).sin() + ((z as f32 + ((seed >> 8) & 255) as f32) * 0.0042).cos()) * 2.0;
    let rolling_hills = (x as f32 * 0.012).sin() * (z as f32 * 0.014).cos() * 3.5;
    let mountain_region = hash(seed, x / 128, z / 128);
    let ridge = ((x as f32 * 0.006).sin() + (z as f32 * 0.007).cos()).abs();
    let mountain_noise = ((x as f32 * 0.017).sin() * (z as f32 * 0.013).cos()).abs();
    let mountains = if mountain_region > 0.56 {
        let shape = (ridge * 0.72 + mountain_noise * 0.28).powf(2.2);
        shape * 35.0
    } else { 0.0 };
    let detail = (hash(seed, x / 6, z / 6) - 0.5) * 1.2;
    (SEA_LEVEL + 8 + continental as i32 + rolling_hills as i32 + mountains as i32 + detail as i32).clamp(4, WORLD_HEIGHT - 4)
}

fn main() {
    App::new()
        .insert_resource(VoxelWorld::new())
        .insert_resource(LookState::default())
        .insert_resource(PlayerState { flying: false, velocity: Vec3::ZERO, last_space: -10.0 })
        .insert_resource(Inventory { slots: [16, 0, 0, 0, 0, 0, 0, 0, 0], selected: 0 })
        .insert_resource(MiningState::default())
        .insert_resource({ let (sender, receiver) = mpsc::channel(); MeshRebuildQueue { sender, receiver: Mutex::new(receiver), running: false } })
        .insert_resource(ClearColor(Color::srgb(0.34, 0.66, 0.94)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "mineONrust".into(),
                resolution: (1280.0_f32, 720.0_f32).into(),
                cursor_options: CursorOptions { grab_mode: CursorGrabMode::None, visible: true, ..default() },
                present_mode: bevy::window::PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, (report_texture_status, window_controls, mouse_look, player_move, hotbar_input, block_interaction, update_hotbar, update_mining_ui, update_particles, poll_mesh_rebuilds).chain())
        .run();
}

fn setup(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let top = materials.add(StandardMaterial { base_color_texture: Some(asset_server.load("grass_top.jpg")), perceptual_roughness: 1.0, ..default() });
    let side = materials.add(StandardMaterial { base_color_texture: Some(asset_server.load("grass_side.png")), perceptual_roughness: 1.0, ..default() });
    let bottom = materials.add(StandardMaterial { base_color_texture: Some(asset_server.load("dirt_bottom.png")), perceptual_roughness: 1.0, ..default() });
    let stone = materials.add(StandardMaterial { base_color_texture: Some(asset_server.load("stone.jpg")), perceptual_roughness: 1.0, ..default() });
    commands.insert_resource(TextureHandles {
        top: asset_server.load("grass_top.jpg"),
        side: asset_server.load("grass_side.png"),
        bottom: asset_server.load("dirt_bottom.png"),
        stone: asset_server.load("stone.jpg"),
        reported: false,
    });
    for (group, material) in [(FaceGroup::GrassTop, top), (FaceGroup::GrassSide, side), (FaceGroup::Dirt, bottom), (FaceGroup::Stone, stone)] {
        commands.spawn((Mesh3d(meshes.add(build_mesh(&world, group))), MeshMaterial3d(material), WorldMesh, group));
    }

    let spawn_height = terrain_height(world.seed, WORLD_SIZE / 2, WORLD_SIZE / 2) + 3;
    let camera = commands.spawn((Camera3d::default(), Projection::Perspective(PerspectiveProjection { far: 220.0, ..default() }), DistanceFog {
        color: Color::srgb(0.34, 0.66, 0.94),
        falloff: FogFalloff::Linear { start: 80.0, end: 220.0 },
        ..default()
    }, Player, Transform::from_xyz(WORLD_SIZE as f32 / 2.0, spawn_height as f32, WORLD_SIZE as f32 / 2.0))).id();
    commands.spawn((Camera2d, Camera { order: 1, clear_color: ClearColorConfig::None, ..default() }));
    commands.entity(camera).with_children(|parent| {
        parent.spawn((Mesh3d(meshes.add(Cuboid::new(0.20, 0.20, 0.55))), MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.73, 0.42, 0.25), perceptual_roughness: 0.9, ..default()
        })), Transform::from_xyz(0.42, -0.34, -0.72)));
        let crosshair_material = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() });
        parent.spawn((Mesh3d(meshes.add(Cuboid::new(0.003, 0.035, 0.003))), MeshMaterial3d(crosshair_material.clone()), Transform::from_xyz(0.0, 0.0, -0.75)));
        parent.spawn((Mesh3d(meshes.add(Cuboid::new(0.035, 0.003, 0.003))), MeshMaterial3d(crosshair_material), Transform::from_xyz(0.0, 0.0, -0.75)));
    });
        commands.spawn((Text::new("+"), TextFont { font_size: 18.0, ..default() }, TextColor(Color::WHITE),
            Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), top: Val::Percent(50.0), margin: UiRect::new(Val::Px(-4.0), Val::Auto, Val::Px(-9.0), Val::Auto), ..default() }));
        commands.spawn((Text::new("[1 DIRT:16] [2 STONE:0]  3  4  5  6  7  8  9"), TextFont { font_size: 20.0, ..default() }, TextColor(Color::WHITE), Hotbar,
            Node { position_type: PositionType::Absolute, left: Val::Percent(31.0), bottom: Val::Px(24.0), padding: UiRect::axes(Val::Px(18.0), Val::Px(10.0)), border: UiRect::all(Val::Px(2.0)), ..default() },
            BorderColor(Color::srgba(0.95, 0.78, 0.36, 0.9)), BackgroundColor(Color::srgba(0.04, 0.06, 0.10, 0.82))));
        commands.spawn((Text::new(""), TextFont { font_size: 16.0, ..default() }, TextColor(Color::srgb(1.0, 0.8, 0.25)), MiningProgressBar,
            Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), top: Val::Percent(54.0), margin: UiRect::new(Val::Px(-45.0), Val::Auto, Val::Px(-8.0), Val::Auto), ..default() }));
    commands.spawn((DirectionalLight { illuminance: 9_000.0, shadows_enabled: false, color: Color::srgb(1.0, 0.94, 0.82), ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.1, -0.8, 0.0))));
    commands.spawn((DirectionalLight { illuminance: 4_000.0, shadows_enabled: false, color: Color::srgb(0.58, 0.72, 1.0), ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.45, 2.2, 0.0))));
    commands.insert_resource(AmbientLight { color: Color::srgb(0.72, 0.82, 1.0), brightness: 0.85 });
}

fn report_texture_status(
    mut handles: Option<ResMut<TextureHandles>>,
    server: Res<AssetServer>,
) {
    let Some(handles) = handles.as_deref_mut() else { return; };
    if handles.reported { return; }
    let textures = [("grass_top.jpg", &handles.top), ("grass_side.png", &handles.side), ("dirt_bottom.png", &handles.bottom), ("stone.jpg", &handles.stone)];
    let states: Vec<_> = textures.iter().map(|(_, handle)| server.get_load_state(handle.id())).collect();
    if states.iter().all(|state| matches!(state, Some(LoadState::Loaded))) {
        info!("All terrain textures loaded successfully");
        handles.reported = true;
    } else if states.iter().any(|state| matches!(state, Some(LoadState::Failed(_)))) {
        for ((name, _), state) in textures.iter().zip(states) {
            if matches!(state, Some(LoadState::Failed(_))) {
                error!("Texture failed to load: {name}. Run cargo from the project root.");
            }
        }
        handles.reported = true;
    }
}

fn build_mesh(world: &VoxelWorld, group: FaceGroup) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let faces: [([i32; 3], [[f32; 3]; 4]); 6] = [
        ([0, 1, 0], [[0.,1.,0.],[0.,1.,1.],[1.,1.,1.],[1.,1.,0.]]),
        ([0,-1, 0], [[0.,0.,0.],[1.,0.,0.],[1.,0.,1.],[0.,0.,1.]]),
        ([1, 0, 0], [[1.,0.,0.],[1.,1.,0.],[1.,1.,1.],[1.,0.,1.]]),
        ([-1,0, 0], [[0.,0.,1.],[0.,1.,1.],[0.,1.,0.],[0.,0.,0.]]),
        ([0, 0, 1], [[1.,0.,1.],[1.,1.,1.],[0.,1.,1.],[0.,0.,1.]]),
        ([0, 0,-1], [[0.,0.,0.],[0.,1.,0.],[1.,1.,0.],[1.,0.,0.]]),
    ];
    for x in 0..WORLD_SIZE { for y in 0..WORLD_HEIGHT { for z in 0..WORLD_SIZE {
        let center = WORLD_SIZE / 2;
        if (x - center).abs() > RENDER_RADIUS || (z - center).abs() > RENDER_RADIUS { continue; }
        if world.get(x, y, z) == 0 { continue; }
        for (normal, corners) in faces {
            if world.get(x + normal[0], y + normal[1], z + normal[2]) != 0 { continue; }
            let block_group = match world.get(x, y, z) {
                GRASS if normal[1] > 0 => FaceGroup::GrassTop,
                GRASS => FaceGroup::GrassSide,
                DIRT => FaceGroup::Dirt,
                STONE => FaceGroup::Stone,
                _ => continue,
            };
            let face_group = block_group;
            if std::mem::discriminant(&face_group) != std::mem::discriminant(&group) { continue; }
            let base = positions.len() as u32;
            for (corner, uv) in corners.into_iter().zip([[0.01,0.01],[0.01,0.99],[0.99,0.99],[0.99,0.01]]) {
                positions.push([x as f32 + corner[0], y as f32 + corner[1], z as f32 + corner[2]]);
                normals.push([normal[0] as f32, normal[1] as f32, normal[2] as f32]);
                uvs.push(if group == FaceGroup::GrassSide { [uv[0], 1.0 - uv[1]] } else { uv });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }}}
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn window_controls(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut windows: Query<&mut Window>,
) {
    let Ok(mut window) = windows.get_single_mut() else { return; };
    if keys.just_pressed(KeyCode::F11) {
        window.mode = match window.mode {
            WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            _ => WindowMode::Windowed,
        };
    }
    if mouse.just_pressed(MouseButton::Left) {
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
        window.cursor_options.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
    }
    if !window.focused {
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
    }
}

fn mouse_look(
    mut motion: EventReader<MouseMotion>,
    mut look: ResMut<LookState>,
    windows: Query<&Window>,
    mut query: Query<&mut Transform, With<Player>>,
) {
    let Ok(window) = windows.get_single() else { return; };
    if window.cursor_options.grab_mode != CursorGrabMode::Locked { return; }
    let Ok(mut transform) = query.get_single_mut() else { return; };
    let mut delta = Vec2::ZERO;
    for event in motion.read() { delta += event.delta; }
    look.yaw -= delta.x * 0.0025;
    look.pitch = (look.pitch - delta.y * 0.0025).clamp(-1.54, 1.54);
    transform.rotation = Quat::from_euler(EulerRot::YXZ, look.yaw, look.pitch, 0.0);
}

fn player_move(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    world: Res<VoxelWorld>,
    mut state: ResMut<PlayerState>,
    mut query: Query<&mut Transform, With<Player>>,
) {
    let Ok(mut transform) = query.get_single_mut() else { return; };
    let now = time.elapsed_secs();
    if keys.just_pressed(KeyCode::Space) {
        if now - state.last_space < 0.32 {
            state.flying = !state.flying;
            state.velocity = Vec3::ZERO;
        } else if !state.flying && is_grounded(&world, transform.translation) {
            state.velocity.y = 7.0;
        }
        state.last_space = now;
    }
    let mut direction = Vec3::ZERO;
    let forward = transform.forward().with_y(0.0).normalize_or_zero();
    let right = transform.right().with_y(0.0).normalize_or_zero();
    if keys.pressed(KeyCode::KeyW) { direction += forward; }
    if keys.pressed(KeyCode::KeyS) { direction -= forward; }
    if keys.pressed(KeyCode::KeyD) { direction += right; }
    if keys.pressed(KeyCode::KeyA) { direction -= right; }
    if state.flying {
        if keys.pressed(KeyCode::Space) { direction.y += 1.0; }
        if keys.pressed(KeyCode::ShiftLeft) { direction.y -= 1.0; }
        transform.translation += direction.normalize_or_zero() * 14.0 * time.delta_secs();
        return;
    }
    state.velocity.y -= 22.0 * time.delta_secs();
    let movement = direction.normalize_or_zero() * 6.0 * time.delta_secs()
        + Vec3::Y * state.velocity.y * time.delta_secs();
    move_with_collision(&world, &mut transform.translation, movement, &mut state.velocity);
}

fn collides(world: &VoxelWorld, eye_position: Vec3) -> bool {
    let min_x = (eye_position.x - PLAYER_RADIUS).floor() as i32;
    let max_x = (eye_position.x + PLAYER_RADIUS).floor() as i32;
    let min_y = (eye_position.y - PLAYER_EYE_HEIGHT).floor() as i32;
    let max_y = (eye_position.y + (PLAYER_HEIGHT - PLAYER_EYE_HEIGHT)).floor() as i32;
    let min_z = (eye_position.z - PLAYER_RADIUS).floor() as i32;
    let max_z = (eye_position.z + PLAYER_RADIUS).floor() as i32;
    for x in min_x..=max_x { for y in min_y..=max_y { for z in min_z..=max_z {
        if world.get(x, y, z) != 0 { return true; }
    }}}
    false
}

fn is_grounded(world: &VoxelWorld, position: Vec3) -> bool {
    !collides(world, position) && collides(world, position - Vec3::Y * 0.08)
}

fn move_with_collision(world: &VoxelWorld, position: &mut Vec3, movement: Vec3, velocity: &mut Vec3) {
    for (axis, amount) in [(0, movement.x), (1, movement.y), (2, movement.z)] {
        if amount.abs() < f32::EPSILON { continue; }
        let mut candidate = *position;
        candidate[axis] += amount;
        if !collides(world, candidate) {
            *position = candidate;
        } else if axis == 1 {
            if amount < 0.0 {
                position.y = (candidate.y - PLAYER_EYE_HEIGHT).floor() + 1.0 + PLAYER_EYE_HEIGHT + 0.001;
            } else {
                position.y = candidate.y.ceil() - (PLAYER_HEIGHT - PLAYER_EYE_HEIGHT) - 0.001;
            }
            *velocity = velocity.with_y(0.0);
        }
    }
}

fn hotbar_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: EventReader<MouseWheel>,
    mut inventory: ResMut<Inventory>,
) {
    for key in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9] {
        if keys.just_pressed(key) {
            inventory.selected = match key {
                KeyCode::Digit1 => 0, KeyCode::Digit2 => 1, KeyCode::Digit3 => 2,
                KeyCode::Digit4 => 3, KeyCode::Digit5 => 4, KeyCode::Digit6 => 5,
                KeyCode::Digit7 => 6, KeyCode::Digit8 => 7, KeyCode::Digit9 => 8, _ => 0,
            };
        }
    }
    for event in wheel.read() {
        if event.y > 0.0 { inventory.selected = (inventory.selected + 8) % 9; }
        if event.y < 0.0 { inventory.selected = (inventory.selected + 1) % 9; }
    }
}

fn update_hotbar(inventory: Res<Inventory>, mut query: Query<&mut Text, With<Hotbar>>) {
    if !inventory.is_changed() { return; }
    let labels: Vec<String> = inventory.slots.iter().enumerate().map(|(index, count)| {
        let marker = if index == inventory.selected { "[" } else { " " };
        let close = if index == inventory.selected { "]" } else { " " };
        format!("{marker}{}:{}{close}", index + 1, count)
    }).collect();
    for mut text in &mut query { *text = Text::new(labels.join(" ")); }
}

fn update_mining_ui(mining: Res<MiningState>, mut query: Query<&mut Text, With<MiningProgressBar>>) {
    if !mining.is_changed() { return; }
    let text = if mining.target.is_some() {
        let filled = (mining.progress * 12.0).clamp(0.0, 12.0) as usize;
        format!("CRACKS [{}{}]", "#".repeat(filled), "-".repeat(12 - filled))
    } else { String::new() };
    for mut label in &mut query { *label = Text::new(text.clone()); }
}

fn update_particles(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Transform, &mut BreakParticle)>,
) {
    for (entity, mut transform, mut particle) in &mut query {
        particle.lifetime -= time.delta_secs();
        transform.translation += particle.velocity * time.delta_secs();
        particle.velocity.y -= 12.0 * time.delta_secs();
        transform.scale *= 0.985;
        if particle.lifetime <= 0.0 { commands.entity(entity).despawn(); }
    }
}

fn block_interaction(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    camera: Query<&GlobalTransform, With<Player>>,
    mut world: ResMut<VoxelWorld>,
    mut inventory: ResMut<Inventory>,
    mut mining: ResMut<MiningState>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rebuild_queue: ResMut<MeshRebuildQueue>,
) {
    let Ok(transform) = camera.get_single() else { return; };
    let origin = transform.translation();
    let direction = transform.forward();
    let mut point = origin;
    let mut previous = origin;
    let mut hit = None;
    for _ in 0..(REACH as usize * 20) {
        point += direction * 0.05;
        let cell = point.floor().as_ivec3();
        if world.get(cell.x, cell.y, cell.z) != 0 {
            hit = Some((cell, previous.floor().as_ivec3()));
            break;
        }
        previous = point;
    }
    if mouse.just_pressed(MouseButton::Right) {
        if let Some((_, place)) = hit {
            let selected_block = if inventory.selected == 0 { DIRT } else if inventory.selected == 1 { STONE } else { 0 };
            if selected_block != 0 && inventory.slots[inventory.selected] > 0 && place.x >= 0 && place.y >= 0 && place.z >= 0 && place.x < WORLD_SIZE && place.y < WORLD_HEIGHT && place.z < WORLD_SIZE && world.get(place.x, place.y, place.z) == 0 {
                world.blocks[index(place.x, place.y, place.z)] = selected_block;
                let selected_slot = inventory.selected;
                inventory.slots[selected_slot] -= 1;
                queue_mesh_rebuild(&world, &mut rebuild_queue);
            }
        }
        return;
    }
    if !mouse.pressed(MouseButton::Left) {
        mining.target = None;
        mining.progress = 0.0;
        return;
    }
    let Some((target, _)) = hit else { mining.target = None; mining.progress = 0.0; return; };
    if mining.target != Some(target) { mining.target = Some(target); mining.progress = 0.0; }
    let block = world.get(target.x, target.y, target.z);
    let break_time = if block == STONE { 2.2 } else { 0.28 };
    mining.progress += time.delta_secs() / break_time;
    if mining.progress >= 1.0 {
        world.blocks[index(target.x, target.y, target.z)] = 0;
        if block != STONE {
            let slot = if inventory.selected == 0 { 0 } else { 0 };
            inventory.slots[slot] = inventory.slots[slot].saturating_add(1);
        }
        mining.target = None;
        mining.progress = 0.0;
        let particle_material = materials.add(StandardMaterial { base_color: Color::srgb(0.45, 0.30, 0.18), unlit: true, ..default() });
        for particle_index in 0..8 {
            let angle = particle_index as f32 * 0.78;
            commands.spawn((Mesh3d(meshes.add(Cuboid::new(0.08, 0.08, 0.08))), MeshMaterial3d(particle_material.clone()),
                Transform::from_translation(target.as_vec3() + Vec3::splat(0.5)),
                BreakParticle { velocity: Vec3::new(angle.cos() * 2.0, 2.5 + (particle_index % 3) as f32, angle.sin() * 2.0), lifetime: 0.45 }));
        }
        queue_mesh_rebuild(&world, &mut rebuild_queue);
    }
}

fn queue_mesh_rebuild(world: &VoxelWorld, queue: &mut MeshRebuildQueue) {
    if queue.running { return; }
    queue.running = true;
    let blocks = world.blocks.clone();
    let sender = queue.sender.clone();
    thread::spawn(move || {
        let background_world = VoxelWorld { blocks, seed: 0 };
        let result = [
            build_mesh(&background_world, FaceGroup::GrassTop),
            build_mesh(&background_world, FaceGroup::GrassSide),
            build_mesh(&background_world, FaceGroup::Dirt),
            build_mesh(&background_world, FaceGroup::Stone),
        ];
        let _ = sender.send(result);
    });
}

fn poll_mesh_rebuilds(
    mut queue: ResMut<MeshRebuildQueue>,
    mut meshes: ResMut<Assets<Mesh>>,
    mesh_query: Query<(&Mesh3d, &FaceGroup), With<WorldMesh>>,
) {
    let Ok(result) = queue.receiver.lock().unwrap().try_recv() else { return; };
    for (handle, group) in mesh_query.iter() {
        let mesh = match group {
            FaceGroup::GrassTop => &result[0],
            FaceGroup::GrassSide => &result[1],
            FaceGroup::Dirt => &result[2],
            FaceGroup::Stone => &result[3],
        };
        if let Some(asset) = meshes.get_mut(&handle.0) { *asset = mesh.clone(); }
    }
    queue.running = false;
}