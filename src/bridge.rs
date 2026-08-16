use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Copy)]
pub enum GeometryKind {
    Cube,
    Plane,
    Sphere,
}

#[derive(Debug, Clone, Copy)]
pub struct CubeSnapshot {
    pub geometry: GeometryKind,
    pub position: [f64; 3],
    pub scale: [f64; 3],
    pub rotation_y: f64,
    #[allow(dead_code)]
    pub color: [f64; 4],
    pub material: MaterialSnapshot,
    pub model_matrix: Option<[[f64; 4]; 4]>,
}

#[derive(Debug, Clone, Copy)]
pub struct MaterialSnapshot {
    pub base_color: [f64; 4],
    pub metallic: f64,
    pub roughness: f64,
    pub emissive: [f64; 3],
    pub unlit: bool,
    pub base_color_texture: Option<u64>,
}

impl Default for MaterialSnapshot {
    fn default() -> Self {
        Self {
            base_color: [0.1, 0.8, 0.95, 1.0],
            metallic: 0.0,
            roughness: 0.65,
            emissive: [0.0, 0.0, 0.0],
            unlit: false,
            base_color_texture: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DirectionalLightSnapshot {
    pub direction: [f64; 3],
    pub color: [f64; 3],
    pub intensity: f64,
    pub ambient: [f64; 3],
}

impl Default for DirectionalLightSnapshot {
    fn default() -> Self {
        Self {
            direction: [-0.35, -0.8, -0.45],
            color: [1.0, 0.95, 0.85],
            intensity: 2.5,
            ambient: [0.08, 0.1, 0.14],
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomMeshSnapshot {
    pub geometry_id: u64,
    pub texture_id: Option<u64>,
    pub position: [f64; 3],
    pub scale: [f64; 3],
    pub rotation_y: f64,
    #[allow(dead_code)]
    pub color: [f64; 4],
    pub material: MaterialSnapshot,
    pub model_matrix: Option<[[f64; 4]; 4]>,
}

#[derive(Debug, Clone, Copy)]
pub struct ParticleSnapshot {
    pub position: [f64; 3],
    pub size: f64,
    pub color: [f64; 4],
    pub emissive: [f64; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryData {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct GeometryRegistry {
    geometries: HashMap<u64, Arc<GeometryData>>,
}

pub type SharedGeometryRegistry = Arc<Mutex<GeometryRegistry>>;

impl GeometryRegistry {
    pub fn shared() -> SharedGeometryRegistry {
        Arc::new(Mutex::new(Self::default()))
    }

    pub fn register(
        &mut self,
        geometry_id: u64,
        positions: Vec<[f32; 3]>,
        indices: Vec<u32>,
        normals: Vec<[f32; 3]>,
        uvs: Vec<[f32; 2]>,
    ) -> Result<(), String> {
        if positions.len() < 3 {
            return Err("BufferGeometry must contain at least three vertices".to_string());
        }
        if indices.len() < 3 || indices.len() % 3 != 0 {
            return Err("BufferGeometry indices must contain complete triangles".to_string());
        }
        if indices
            .iter()
            .any(|index| *index as usize >= positions.len())
        {
            return Err("BufferGeometry index is outside the position attribute".to_string());
        }
        if !uvs.is_empty() && uvs.len() != positions.len() {
            return Err("BufferGeometry UV count must match the position count".to_string());
        }
        if !normals.is_empty() && normals.len() != positions.len() {
            return Err("BufferGeometry normal count must match the position count".to_string());
        }
        let normals = if normals.is_empty() {
            generate_vertex_normals(&positions, &indices)
        } else {
            normals
        };
        let geometry = GeometryData {
            positions,
            indices,
            normals,
            uvs,
        };
        if self
            .geometries
            .get(&geometry_id)
            .is_some_and(|existing| existing.as_ref() == &geometry)
        {
            return Ok(());
        }
        self.geometries.insert(geometry_id, Arc::new(geometry));
        Ok(())
    }

    pub fn get(&self, geometry_id: u64) -> Option<Arc<GeometryData>> {
        self.geometries.get(&geometry_id).cloned()
    }
}

fn generate_vertex_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0; 3]; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let a = positions[triangle[0] as usize];
        let b = positions[triangle[1] as usize];
        let c = positions[triangle[2] as usize];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let face = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        for index in triangle {
            let normal = &mut normals[*index as usize];
            normal[0] += face[0];
            normal[1] += face[1];
            normal[2] += face[2];
        }
    }
    for normal in &mut normals {
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
            .sqrt()
            .max(f32::EPSILON);
        *normal = [normal[0] / length, normal[1] / length, normal[2] / length];
    }
    normals
}

#[derive(Debug, Default)]
pub struct TextureRegistry {
    textures: HashMap<u64, Arc<TextureData>>,
}

pub type SharedTextureRegistry = Arc<Mutex<TextureRegistry>>;

impl TextureRegistry {
    pub fn shared() -> SharedTextureRegistry {
        Arc::new(Mutex::new(Self::default()))
    }

    pub fn register(
        &mut self,
        texture_id: u64,
        width: u32,
        height: u32,
        rgba8: Vec<u8>,
    ) -> Result<(), String> {
        if width == 0 || height == 0 {
            return Err("texture dimensions must be positive".to_string());
        }
        if rgba8.len() != width as usize * height as usize * 4 {
            return Err("texture data must be RGBA8".to_string());
        }
        let texture = TextureData {
            width,
            height,
            rgba8,
        };
        if self
            .textures
            .get(&texture_id)
            .is_some_and(|existing| existing.as_ref() == &texture)
        {
            return Ok(());
        }
        self.textures.insert(texture_id, Arc::new(texture));
        Ok(())
    }

    pub fn get(&self, texture_id: u64) -> Option<Arc<TextureData>> {
        self.textures.get(&texture_id).cloned()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CameraProjection {
    Perspective,
    Orthographic {
        left: f64,
        right: f64,
        top: f64,
        bottom: f64,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct CameraSnapshot {
    pub position: [f64; 3],
    pub target: [f64; 3],
    pub fov_y_degrees: f64,
    pub near: f64,
    pub far: f64,
    pub projection: CameraProjection,
}

#[derive(Debug, Clone)]
pub struct NativeRenderSnapshot {
    pub clear_color: [f64; 4],
    pub cubes: Vec<CubeSnapshot>,
    pub custom_meshes: Vec<CustomMeshSnapshot>,
    pub particles: Vec<ParticleSnapshot>,
    pub camera: CameraSnapshot,
    pub geometry_registry: SharedGeometryRegistry,
    pub texture_registry: SharedTextureRegistry,
    pub directional_light: DirectionalLightSnapshot,
}

#[derive(Debug)]
pub struct NativeRenderState {
    clear_color: [f64; 4],
    cubes: Vec<CubeSnapshot>,
    custom_meshes: Vec<CustomMeshSnapshot>,
    particles: Vec<ParticleSnapshot>,
    camera: CameraSnapshot,
    geometry_registry: SharedGeometryRegistry,
    texture_registry: SharedTextureRegistry,
    directional_light: DirectionalLightSnapshot,
}

pub type SharedRenderState = Arc<Mutex<NativeRenderState>>;

#[derive(Debug, Default)]
pub struct NativeInputState {
    pressed_keys: HashSet<String>,
    mouse_buttons: HashSet<u8>,
    mouse_position: [f64; 2],
}

pub type SharedInputState = Arc<Mutex<NativeInputState>>;

impl NativeInputState {
    pub fn shared() -> SharedInputState {
        Arc::new(Mutex::new(Self::default()))
    }

    pub fn set_key(&mut self, code: impl Into<String>, pressed: bool) {
        let code = code.into();
        if pressed {
            self.pressed_keys.insert(code);
        } else {
            self.pressed_keys.remove(&code);
        }
    }

    pub fn is_key_down(&self, code: &str) -> bool {
        self.pressed_keys.contains(code)
    }

    pub fn clear(&mut self) {
        self.pressed_keys.clear();
        self.mouse_buttons.clear();
    }

    pub fn set_mouse_button(&mut self, button: u8, pressed: bool) {
        if pressed {
            self.mouse_buttons.insert(button);
        } else {
            self.mouse_buttons.remove(&button);
        }
    }

    pub fn is_mouse_button_down(&self, button: u8) -> bool {
        self.mouse_buttons.contains(&button)
    }

    pub fn set_mouse_position(&mut self, x: f64, y: f64) {
        self.mouse_position = [x, y];
    }

    pub fn mouse_position(&self) -> [f64; 2] {
        self.mouse_position
    }
}

impl Default for NativeRenderState {
    fn default() -> Self {
        Self {
            clear_color: [0.025, 0.04, 0.09, 1.0],
            cubes: vec![CubeSnapshot {
                geometry: GeometryKind::Cube,
                position: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                rotation_y: 0.0,
                color: [0.1, 0.8, 0.95, 1.0],
                material: MaterialSnapshot::default(),
                model_matrix: None,
            }],
            custom_meshes: Vec::new(),
            particles: Vec::new(),
            camera: CameraSnapshot {
                position: [0.0, 0.0, 4.0],
                target: [0.0, 0.0, 0.0],
                fov_y_degrees: 60.0,
                near: 0.1,
                far: 100.0,
                projection: CameraProjection::Perspective,
            },
            geometry_registry: GeometryRegistry::shared(),
            texture_registry: TextureRegistry::shared(),
            directional_light: DirectionalLightSnapshot::default(),
        }
    }
}

impl NativeRenderState {
    pub fn shared() -> SharedRenderState {
        Arc::new(Mutex::new(Self::default()))
    }

    pub fn snapshot(&self) -> NativeRenderSnapshot {
        NativeRenderSnapshot {
            clear_color: self.clear_color,
            cubes: self.cubes.clone(),
            custom_meshes: self.custom_meshes.clone(),
            particles: self.particles.clone(),
            camera: self.camera,
            geometry_registry: self.geometry_registry.clone(),
            texture_registry: self.texture_registry.clone(),
            directional_light: self.directional_light,
        }
    }

    pub fn set_clear_color(&mut self, color: [f64; 4]) {
        self.clear_color = color.map(|component| component.clamp(0.0, 1.0));
    }

    pub fn set_cube(
        &mut self,
        position: [f64; 3],
        scale: [f64; 3],
        rotation_y: f64,
        color: [f64; 4],
    ) {
        let cube = CubeSnapshot {
            geometry: GeometryKind::Cube,
            position,
            scale: scale.map(|value| value.max(0.001)),
            rotation_y,
            color: color.map(|component| component.clamp(0.0, 1.0)),
            material: MaterialSnapshot {
                base_color: color.map(|component| component.clamp(0.0, 1.0)),
                ..MaterialSnapshot::default()
            },
            model_matrix: None,
        };
        if let Some(first) = self.cubes.first_mut() {
            *first = cube;
        } else {
            self.cubes.push(cube);
        }
    }

    pub fn push_cube(
        &mut self,
        position: [f64; 3],
        scale: [f64; 3],
        rotation_y: f64,
        color: [f64; 4],
    ) {
        self.cubes.push(CubeSnapshot {
            geometry: GeometryKind::Cube,
            position,
            scale: scale.map(|value| value.max(0.001)),
            rotation_y,
            color: color.map(|component| component.clamp(0.0, 1.0)),
            material: MaterialSnapshot {
                base_color: color.map(|component| component.clamp(0.0, 1.0)),
                ..MaterialSnapshot::default()
            },
            model_matrix: None,
        });
    }

    pub fn push_plane(
        &mut self,
        position: [f64; 3],
        scale: [f64; 3],
        rotation_y: f64,
        color: [f64; 4],
    ) {
        self.cubes.push(CubeSnapshot {
            geometry: GeometryKind::Plane,
            position,
            scale: scale.map(|value| value.max(0.001)),
            rotation_y,
            color: color.map(|component| component.clamp(0.0, 1.0)),
            material: MaterialSnapshot {
                base_color: color.map(|component| component.clamp(0.0, 1.0)),
                ..MaterialSnapshot::default()
            },
            model_matrix: None,
        });
    }

    pub fn push_sphere(
        &mut self,
        position: [f64; 3],
        scale: [f64; 3],
        rotation_y: f64,
        color: [f64; 4],
    ) {
        self.cubes.push(CubeSnapshot {
            geometry: GeometryKind::Sphere,
            position,
            scale: scale.map(|value| value.max(0.001)),
            rotation_y,
            color: color.map(|component| component.clamp(0.0, 1.0)),
            material: MaterialSnapshot {
                base_color: color.map(|component| component.clamp(0.0, 1.0)),
                ..MaterialSnapshot::default()
            },
            model_matrix: None,
        });
    }

    pub fn begin_frame(&mut self) {
        self.cubes.clear();
        self.custom_meshes.clear();
        self.particles.clear();
    }

    pub fn push_particle(
        &mut self,
        position: [f64; 3],
        size: f64,
        color: [f64; 4],
        emissive: [f64; 3],
    ) {
        self.particles.push(ParticleSnapshot {
            position,
            size: size.max(0.001),
            color: color.map(|component| component.clamp(0.0, 1.0)),
            emissive: emissive.map(|component| component.max(0.0)),
        });
    }

    pub fn push_primitive_matrix_with_material(
        &mut self,
        geometry: GeometryKind,
        model_matrix: [[f64; 4]; 4],
        material: MaterialSnapshot,
    ) {
        self.cubes.push(CubeSnapshot {
            geometry,
            position: [model_matrix[3][0], model_matrix[3][1], model_matrix[3][2]],
            scale: [
                column_length(model_matrix, 0),
                column_length(model_matrix, 1),
                column_length(model_matrix, 2),
            ],
            rotation_y: 0.0,
            color: material.base_color,
            material,
            model_matrix: Some(model_matrix),
        });
    }

    pub fn register_geometry(
        &mut self,
        geometry_id: u64,
        positions: Vec<[f32; 3]>,
        indices: Vec<u32>,
        normals: Vec<[f32; 3]>,
        uvs: Vec<[f32; 2]>,
    ) -> Result<(), String> {
        self.geometry_registry
            .lock()
            .map_err(|_| "geometry registry poisoned".to_string())?
            .register(geometry_id, positions, indices, normals, uvs)
    }

    pub fn register_texture(
        &mut self,
        texture_id: u64,
        width: u32,
        height: u32,
        rgba8: Vec<u8>,
    ) -> Result<(), String> {
        self.texture_registry
            .lock()
            .map_err(|_| "texture registry poisoned".to_string())?
            .register(texture_id, width, height, rgba8)
    }

    pub fn push_custom_mesh_with_texture(
        &mut self,
        geometry_id: u64,
        texture_id: Option<u64>,
        position: [f64; 3],
        scale: [f64; 3],
        rotation_y: f64,
        color: [f64; 4],
    ) {
        self.custom_meshes.push(CustomMeshSnapshot {
            geometry_id,
            texture_id,
            position,
            scale: scale.map(|value| value.max(0.001)),
            rotation_y,
            color: color.map(|component| component.clamp(0.0, 1.0)),
            material: MaterialSnapshot {
                base_color: color.map(|component| component.clamp(0.0, 1.0)),
                base_color_texture: texture_id,
                ..MaterialSnapshot::default()
            },
            model_matrix: None,
        });
    }

    pub fn push_custom_mesh_with_material(
        &mut self,
        geometry_id: u64,
        position: [f64; 3],
        scale: [f64; 3],
        rotation_y: f64,
        material: MaterialSnapshot,
    ) {
        self.custom_meshes.push(CustomMeshSnapshot {
            geometry_id,
            texture_id: material.base_color_texture,
            position,
            scale: scale.map(|value| value.max(0.001)),
            rotation_y,
            color: material
                .base_color
                .map(|component| component.clamp(0.0, 1.0)),
            material,
            model_matrix: None,
        });
    }

    pub fn push_custom_mesh_matrix_with_material(
        &mut self,
        geometry_id: u64,
        model_matrix: [[f64; 4]; 4],
        material: MaterialSnapshot,
    ) {
        self.custom_meshes.push(CustomMeshSnapshot {
            geometry_id,
            texture_id: material.base_color_texture,
            position: [model_matrix[3][0], model_matrix[3][1], model_matrix[3][2]],
            scale: [1.0, 1.0, 1.0],
            rotation_y: 0.0,
            color: material
                .base_color
                .map(|component| component.clamp(0.0, 1.0)),
            material,
            model_matrix: Some(model_matrix),
        });
    }

    pub fn set_directional_light(
        &mut self,
        direction: [f64; 3],
        color: [f64; 3],
        intensity: f64,
        ambient: [f64; 3],
    ) {
        self.directional_light = DirectionalLightSnapshot {
            direction,
            color: color.map(|component| component.clamp(0.0, 1.0)),
            intensity: intensity.max(0.0),
            ambient: ambient.map(|component| component.clamp(0.0, 1.0)),
        };
    }

    pub fn set_camera(
        &mut self,
        position: [f64; 3],
        target: [f64; 3],
        fov_y_degrees: f64,
        near: f64,
        far: f64,
    ) {
        let near = near.max(0.001);
        self.camera = CameraSnapshot {
            position,
            target,
            fov_y_degrees: fov_y_degrees.clamp(1.0, 179.0),
            near,
            far: far.max(near + 0.01),
            projection: CameraProjection::Perspective,
        };
    }

    pub fn set_orthographic_camera(
        &mut self,
        position: [f64; 3],
        target: [f64; 3],
        bounds: [f64; 4],
        near: f64,
        far: f64,
    ) {
        let [left, right, top, bottom] = bounds;
        let left = left.min(right - 0.001);
        let right = right.max(left + 0.001);
        let bottom = bottom.min(top - 0.001);
        let top = top.max(bottom + 0.001);
        let near = near.max(0.001);
        self.camera = CameraSnapshot {
            position,
            target,
            fov_y_degrees: 60.0,
            near,
            far: far.max(near + 0.01),
            projection: CameraProjection::Orthographic {
                left,
                right,
                top,
                bottom,
            },
        };
    }
}

fn column_length(matrix: [[f64; 4]; 4], column: usize) -> f64 {
    (matrix[column][0] * matrix[column][0]
        + matrix[column][1] * matrix[column][1]
        + matrix[column][2] * matrix[column][2])
        .sqrt()
        .max(0.001)
}

#[cfg(test)]
mod tests {
    use super::{NativeInputState, NativeRenderState};

    #[test]
    fn clamps_colors_to_gpu_range() {
        let mut state = NativeRenderState::default();
        state.set_clear_color([-1.0, 0.25, 2.0, 1.0]);
        let snapshot = state.snapshot();
        assert_eq!(snapshot.clear_color, [0.0, 0.25, 1.0, 1.0]);
        assert_eq!(snapshot.camera.position, [0.0, 0.0, 4.0]);
        assert_eq!(snapshot.cubes.len(), 1);
    }

    #[test]
    fn input_state_tracks_physical_keys() {
        let mut input = NativeInputState::default();
        input.set_key("KeyW", true);
        assert!(input.is_key_down("KeyW"));
        input.clear();
        assert!(!input.is_key_down("KeyW"));
    }

    #[test]
    fn input_state_tracks_mouse_position_and_buttons() {
        let mut input = NativeInputState::default();
        input.set_mouse_position(12.5, 24.0);
        input.set_mouse_button(0, true);
        assert_eq!(input.mouse_position(), [12.5, 24.0]);
        assert!(input.is_mouse_button_down(0));
        input.clear();
        assert!(!input.is_mouse_button_down(0));
    }

    #[test]
    fn geometry_and_texture_registries_validate_native_upload_shapes() {
        let mut state = NativeRenderState::default();
        state
            .register_geometry(
                7,
                vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                vec![0, 1, 2],
                Vec::new(),
                vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            )
            .unwrap();
        assert_eq!(
            state
                .snapshot()
                .geometry_registry
                .lock()
                .unwrap()
                .get(7)
                .unwrap()
                .uvs
                .len(),
            3
        );
        assert!(state
            .register_texture(9, 1, 1, vec![255, 0, 0, 255])
            .is_ok());
        assert!(state.register_texture(10, 1, 1, vec![255, 0, 0]).is_err());
    }
}
