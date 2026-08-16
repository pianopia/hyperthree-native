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
    pub position: [f64; 3],
    pub scale: [f64; 3],
    pub rotation_y: f64,
    pub color: [f64; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryData {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
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
        let geometry = GeometryData { positions, indices };
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

#[derive(Debug, Clone, Copy)]
pub struct CameraSnapshot {
    pub position: [f64; 3],
    pub target: [f64; 3],
    pub fov_y_degrees: f64,
    pub near: f64,
    pub far: f64,
}

#[derive(Debug, Clone)]
pub struct NativeRenderSnapshot {
    pub clear_color: [f64; 4],
    pub cubes: Vec<CubeSnapshot>,
    pub custom_meshes: Vec<CustomMeshSnapshot>,
    pub camera: CameraSnapshot,
    pub geometry_registry: SharedGeometryRegistry,
}

#[derive(Debug)]
pub struct NativeRenderState {
    clear_color: [f64; 4],
    cubes: Vec<CubeSnapshot>,
    custom_meshes: Vec<CustomMeshSnapshot>,
    camera: CameraSnapshot,
    geometry_registry: SharedGeometryRegistry,
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
            },
            geometry_registry: GeometryRegistry::shared(),
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
    ) -> Result<(), String> {
        self.geometry_registry
            .lock()
            .map_err(|_| "geometry registry poisoned".to_string())?
            .register(geometry_id, positions, indices)
    }

    pub fn push_custom_mesh(
        &mut self,
        geometry_id: u64,
        position: [f64; 3],
        scale: [f64; 3],
        rotation_y: f64,
        color: [f64; 4],
    ) {
        self.custom_meshes.push(CustomMeshSnapshot {
            geometry_id,
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
}
