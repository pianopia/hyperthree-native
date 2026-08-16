use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy)]
pub struct CubeSnapshot {
    pub position: [f64; 3],
    pub scale: [f64; 3],
    pub rotation_y: f64,
    pub color: [f64; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct CameraSnapshot {
    pub position: [f64; 3],
    pub target: [f64; 3],
    pub fov_y_degrees: f64,
    pub near: f64,
    pub far: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct NativeRenderSnapshot {
    pub clear_color: [f64; 4],
    pub cube: CubeSnapshot,
    pub camera: CameraSnapshot,
}

#[derive(Debug)]
pub struct NativeRenderState {
    clear_color: [f64; 4],
    cube: CubeSnapshot,
    camera: CameraSnapshot,
}

pub type SharedRenderState = Arc<Mutex<NativeRenderState>>;

impl Default for NativeRenderState {
    fn default() -> Self {
        Self {
            clear_color: [0.025, 0.04, 0.09, 1.0],
            cube: CubeSnapshot {
                position: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                rotation_y: 0.0,
                color: [0.1, 0.8, 0.95, 1.0],
            },
            camera: CameraSnapshot {
                position: [0.0, 0.0, 4.0],
                target: [0.0, 0.0, 0.0],
                fov_y_degrees: 60.0,
                near: 0.1,
                far: 100.0,
            },
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
            cube: self.cube,
            camera: self.camera,
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
        self.cube = CubeSnapshot {
            position,
            scale: scale.map(|value| value.max(0.001)),
            rotation_y,
            color: color.map(|component| component.clamp(0.0, 1.0)),
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
        };
    }
}

#[cfg(test)]
mod tests {
    use super::NativeRenderState;

    #[test]
    fn clamps_colors_to_gpu_range() {
        let mut state = NativeRenderState::default();
        state.set_clear_color([-1.0, 0.25, 2.0, 1.0]);
        let snapshot = state.snapshot();
        assert_eq!(snapshot.clear_color, [0.0, 0.25, 1.0, 1.0]);
        assert_eq!(snapshot.camera.position, [0.0, 0.0, 4.0]);
    }
}
