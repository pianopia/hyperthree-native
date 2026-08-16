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
    pub color: [f64; 4],
}

#[derive(Debug, Clone)]
pub struct CustomMeshSnapshot {
    pub geometry_id: u64,
    pub texture_id: Option<u64>,
    pub position: [f64; 3],
    pub scale: [f64; 3],
    pub rotation_y: f64,
    pub color: [f64; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryData {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
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
        let geometry = GeometryData {
            positions,
            indices,
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
    pub camera: CameraSnapshot,
    pub geometry_registry: SharedGeometryRegistry,
    pub texture_registry: SharedTextureRegistry,
}

#[derive(Debug)]
pub struct NativeRenderState {
    clear_color: [f64; 4],
    cubes: Vec<CubeSnapshot>,
    custom_meshes: Vec<CustomMeshSnapshot>,
    camera: CameraSnapshot,
    geometry_registry: SharedGeometryRegistry,
    texture_registry: SharedTextureRegistry,
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
            }],
            custom_meshes: Vec::new(),
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
            camera: self.camera,
            geometry_registry: self.geometry_registry.clone(),
            texture_registry: self.texture_registry.clone(),
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
        });
    }

    pub fn begin_frame(&mut self) {
        self.cubes.clear();
        self.custom_meshes.clear();
    }

    pub fn register_geometry(
        &mut self,
        geometry_id: u64,
        positions: Vec<[f32; 3]>,
        indices: Vec<u32>,
        uvs: Vec<[f32; 2]>,
    ) -> Result<(), String> {
        self.geometry_registry
            .lock()
            .map_err(|_| "geometry registry poisoned".to_string())?
            .register(geometry_id, positions, indices, uvs)
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
        });
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
