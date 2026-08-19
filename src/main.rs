use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::asset::RenderAssetUsages;
use bevy::window::{CursorGrabMode, CursorOptions};

const SIZE_X: i32 = 64;
const SIZE_Y: i32 = 32;
const SIZE_Z: i32 = 64;
const REACH: f32 = 7.0;

#[derive(Resource)]
struct VoxelWorld {
    blocks: Vec<u8>,
}

impl VoxelWorld {
    fn new() -> Self {
        let mut blocks = vec![0; (SIZE_X * SIZE_Y * SIZE_Z) as usize];
        for x in 0..SIZE_X {
            for z in 0..SIZE_Z {
                let wave = ((x as f32 * 0.18).sin() * 2.0 + (z as f32 * 0.13).cos() * 2.0) as i32;
                let height = (9 + wave).clamp(2, SIZE_Y - 2);
                for y in 0..=height {
                    let block = if y == height { 2 } else if y > height - 3 { 3 } else { 1 };
                    self_set(&mut blocks, x, y, z, block);
                }
            }
        }
        Self { blocks }
    }

    fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if x < 0 || y < 0 || z < 0 || x >= SIZE_X || y >= SIZE_Y || z >= SIZE_Z { return 0; }
        self.blocks[index(x, y, z)]
    }
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct WorldMesh;

#[derive(Resource, Default)]
struct LookState { pitch: f32, yaw: f32 }

fn index(x: i32, y: i32, z: i32) -> usize {
    ((y * SIZE_Z + z) * SIZE_X + x) as usize
}

fn self_set(blocks: &mut [u8], x: i32, y: i32, z: i32, value: u8) {
    blocks[index(x, y, z)] = value;
}

fn main() {
    App::new()
        .insert_resource(VoxelWorld::new())
        .insert_resource(LookState::default())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "mineONrust".into(),
                resolution: (1280.0_f32, 720.0_f32).into(),
                cursor_options: CursorOptions { grab_mode: CursorGrabMode::Locked, visible: false, ..default() },
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, (mouse_look, player_move, block_interaction).chain())
        .run();
}

fn setup(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(build_mesh(&world));
    commands.spawn((Mesh3d(mesh), MeshMaterial3d(materials.add(StandardMaterial {
        base_color: Color::srgb(0.43, 0.60, 0.30),
        perceptual_roughness: 1.0,
        ..default()
    })), WorldMesh));

    commands.spawn((Camera3d::default(), Player, Transform::from_xyz(32.0, 15.0, 45.0)));
    commands.spawn((DirectionalLight { illuminance: 12_000.0, shadows_enabled: true, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.0, -0.8, 0.0))));
    commands.insert_resource(AmbientLight { color: Color::srgb(0.55, 0.65, 0.8), brightness: 0.35 });
}

fn build_mesh(world: &VoxelWorld) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let faces: [([i32; 3], [[f32; 3]; 4]); 6] = [
        ([0, 1, 0], [[0.,1.,0.],[1.,1.,0.],[1.,1.,1.],[0.,1.,1.]]),
        ([0,-1, 0], [[0.,0.,1.],[1.,0.,1.],[1.,0.,0.],[0.,0.,0.]]),
        ([1, 0, 0], [[1.,0.,0.],[1.,0.,1.],[1.,1.,1.],[1.,1.,0.]]),
        ([-1,0, 0], [[0.,0.,1.],[0.,0.,0.],[0.,1.,0.],[0.,1.,1.]]),
        ([0, 0, 1], [[1.,0.,1.],[0.,0.,1.],[0.,1.,1.],[1.,1.,1.]]),
        ([0, 0,-1], [[0.,0.,0.],[1.,0.,0.],[1.,1.,0.],[0.,1.,0.]]),
    ];
    for x in 0..SIZE_X { for y in 0..SIZE_Y { for z in 0..SIZE_Z {
        if world.get(x, y, z) == 0 { continue; }
        for (normal, corners) in faces {
            if world.get(x + normal[0], y + normal[1], z + normal[2]) != 0 { continue; }
            let base = positions.len() as u32;
            for corner in corners {
                positions.push([x as f32 + corner[0], y as f32 + corner[1], z as f32 + corner[2]]);
                normals.push([normal[0] as f32, normal[1] as f32, normal[2] as f32]);
                uvs.push([corner[0], corner[2]]);
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

fn player_move(keys: Res<ButtonInput<KeyCode>>, time: Res<Time>, mut query: Query<&mut Transform, With<Player>>) {
    let Ok(mut transform) = query.get_single_mut() else { return; };
    let mut direction = Vec3::ZERO;
    let forward = transform.forward().with_y(0.0).normalize_or_zero();
    let right = transform.right().with_y(0.0).normalize_or_zero();
    if keys.pressed(KeyCode::KeyW) { direction += forward; }
    if keys.pressed(KeyCode::KeyS) { direction -= forward; }
    if keys.pressed(KeyCode::KeyD) { direction += right; }
    if keys.pressed(KeyCode::KeyA) { direction -= right; }
    if keys.pressed(KeyCode::Space) { direction.y += 1.0; }
    if keys.pressed(KeyCode::ShiftLeft) { direction.y -= 1.0; }
    transform.translation += direction.normalize_or_zero() * 14.0 * time.delta_secs();
}

fn block_interaction(
    mouse: Res<ButtonInput<MouseButton>>,
    camera: Query<&GlobalTransform, With<Player>>,
    mut world: ResMut<VoxelWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mesh_query: Query<&Mesh3d, With<WorldMesh>>,
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
            if target.x >= 0 && target.y >= 0 && target.z >= 0 && target.x < SIZE_X && target.y < SIZE_Y && target.z < SIZE_Z {
                let target_index = index(target.x, target.y, target.z);
                if mouse.pressed(MouseButton::Right) && world.blocks[target_index] == 0 {
                    world.blocks[target_index] = 1;
                } else if mouse.pressed(MouseButton::Left) {
                    world.blocks[target_index] = 0;
                } else {
                    break;
                }
                if let Ok(handle) = mesh_query.get_single() {
                    if let Some(mesh) = meshes.get_mut(&handle.0) { *mesh = build_mesh(&world); }
                }
            }
            break;
        }
        previous = point;
    }
}