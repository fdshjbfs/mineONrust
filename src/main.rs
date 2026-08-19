use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::asset::{LoadState, RenderAssetUsages};
use bevy::window::{CursorGrabMode, CursorOptions};

const WORLD_SIZE: i32 = 512;
const WORLD_HEIGHT: i32 = 96;
const SEA_LEVEL: i32 = 27;
const RENDER_RADIUS: i32 = 160;
const REACH: f32 = 7.0;
const PLAYER_RADIUS: f32 = 0.30;
const PLAYER_HEIGHT: f32 = 1.80;
const PLAYER_EYE_HEIGHT: f32 = 1.62;

#[derive(Resource)]
struct VoxelWorld {
    blocks: Vec<u8>,
}

impl VoxelWorld {
    fn new() -> Self {
        let mut blocks = vec![0; (WORLD_SIZE * WORLD_HEIGHT * WORLD_SIZE) as usize];
        for x in 0..WORLD_SIZE {
            for z in 0..WORLD_SIZE {
                let height = terrain_height(x, z);
                for y in 0..=height {
                    let block = if y == height { 2 } else if y > height - 4 { 3 } else { 1 };
                    set_block(&mut blocks, x, y, z, block);
                }
            }
        }
        Self { blocks }
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
enum FaceGroup { Top, Side, Bottom }

#[derive(Resource)]
struct TextureHandles {
    top: Handle<Image>,
    side: Handle<Image>,
    bottom: Handle<Image>,
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

fn index(x: i32, y: i32, z: i32) -> usize {
    ((y * WORLD_SIZE + z) * WORLD_SIZE + x) as usize
}

fn set_block(blocks: &mut [u8], x: i32, y: i32, z: i32, value: u8) {
    blocks[index(x, y, z)] = value;
}

fn hash(x: i32, z: i32) -> f32 {
    let mut value = (x as i64).wrapping_mul(374_761_393).wrapping_add((z as i64).wrapping_mul(668_265_263));
    value = (value ^ (value >> 13)).wrapping_mul(1_274_126_177);
    ((value ^ (value >> 16)) & 0xffff) as f32 / 65_535.0
}

fn terrain_height(x: i32, z: i32) -> i32 {
    let broad = ((x as f32 * 0.010).sin() + (z as f32 * 0.012).cos()) * 7.0;
    let hills = ((x as f32 * 0.035).sin() * (z as f32 * 0.029).cos()) * 5.0;
    let detail = (hash(x / 4, z / 4) - 0.5) * 5.0;
    (SEA_LEVEL + 9 + broad as i32 + hills as i32 + detail as i32).clamp(4, WORLD_HEIGHT - 4)
}

fn main() {
    App::new()
        .insert_resource(VoxelWorld::new())
        .insert_resource(LookState::default())
        .insert_resource(PlayerState { flying: false, velocity: Vec3::ZERO, last_space: -10.0 })
        .insert_resource(ClearColor(Color::srgb(0.34, 0.66, 0.94)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "mineONrust".into(),
                resolution: (1280.0_f32, 720.0_f32).into(),
                cursor_options: CursorOptions { grab_mode: CursorGrabMode::Confined, visible: false, ..default() },
                present_mode: bevy::window::PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, (report_texture_status, mouse_look, player_move, block_interaction).chain())
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
    commands.insert_resource(TextureHandles {
        top: asset_server.load("grass_top.jpg"),
        side: asset_server.load("grass_side.png"),
        bottom: asset_server.load("dirt_bottom.png"),
        reported: false,
    });
    for (group, material) in [(FaceGroup::Top, top), (FaceGroup::Side, side), (FaceGroup::Bottom, bottom)] {
        commands.spawn((Mesh3d(meshes.add(build_mesh(&world, group))), MeshMaterial3d(material), WorldMesh, group));
    }

    let spawn_height = terrain_height(WORLD_SIZE / 2, WORLD_SIZE / 2) + 3;
    let camera = commands.spawn((Camera3d::default(), Projection::Perspective(PerspectiveProjection { far: 220.0, ..default() }), Player, Transform::from_xyz(WORLD_SIZE as f32 / 2.0, spawn_height as f32, WORLD_SIZE as f32 / 2.0))).id();
    commands.entity(camera).with_children(|parent| {
        parent.spawn((Mesh3d(meshes.add(Cuboid::new(0.20, 0.20, 0.55))), MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.73, 0.42, 0.25), perceptual_roughness: 0.9, ..default()
        })), Transform::from_xyz(0.42, -0.34, -0.72)));
    });
    commands.spawn((Text::new("+"), TextFont { font_size: 24.0, ..default() }, TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), top: Val::Percent(50.0), ..default() }));
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
    let textures = [("grass_top.jpg", &handles.top), ("grass_side.png", &handles.side), ("dirt_bottom.png", &handles.bottom)];
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
            let face_group = if normal[1] > 0 { FaceGroup::Top } else if normal[1] < 0 { FaceGroup::Bottom } else { FaceGroup::Side };
            if std::mem::discriminant(&face_group) != std::mem::discriminant(&group) { continue; }
            let base = positions.len() as u32;
            for (corner, uv) in corners.into_iter().zip([[0.,0.],[0.,1.],[1.,1.],[1.,0.]]) {
                positions.push([x as f32 + corner[0], y as f32 + corner[1], z as f32 + corner[2]]);
                normals.push([normal[0] as f32, normal[1] as f32, normal[2] as f32]);
                uvs.push(if group == FaceGroup::Side { [uv[0], 1.0 - uv[1]] } else { uv });
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

fn mouse_look(mut motion: EventReader<MouseMotion>, mut look: ResMut<LookState>, mut query: Query<&mut Transform, With<Player>>) {
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

fn block_interaction(
    mouse: Res<ButtonInput<MouseButton>>,
    camera: Query<&GlobalTransform, With<Player>>,
    mut world: ResMut<VoxelWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mesh_query: Query<(&Mesh3d, &FaceGroup), With<WorldMesh>>,
) {
    if !(mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right)) { return; }
    let Ok(transform) = camera.get_single() else { return; };
    let origin = transform.translation();
    let direction = transform.forward();
    let mut point = origin;
    let mut previous = origin;
    for _ in 0..(REACH as usize * 20) {
        point += direction * 0.05;
        let cell = point.floor().as_ivec3();
        if world.get(cell.x, cell.y, cell.z) != 0 {
            let target = if mouse.pressed(MouseButton::Right) { previous.floor().as_ivec3() } else { cell };
            if target.x >= 0 && target.y >= 0 && target.z >= 0 && target.x < WORLD_SIZE && target.y < WORLD_HEIGHT && target.z < WORLD_SIZE {
                let target_index = index(target.x, target.y, target.z);
                if mouse.pressed(MouseButton::Right) && world.blocks[target_index] == 0 {
                    world.blocks[target_index] = 1;
                } else if mouse.pressed(MouseButton::Left) {
                    world.blocks[target_index] = 0;
                } else {
                    break;
                }
                for (handle, group) in mesh_query.iter() {
                    if let Some(mesh) = meshes.get_mut(&handle.0) {
                        *mesh = build_mesh(&world, *group);
                    }
                }
            }
            break;
        }
        previous = point;
    }
}