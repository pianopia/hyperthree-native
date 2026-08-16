use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct NativeRenderSnapshot {
    pub clear_color: [f64; 4],
    pub vertex_colors: [[f64; 3]; 3],
}

#[derive(Debug)]
pub struct NativeRenderState {
    clear_color: [f64; 4],
    vertex_colors: [[f64; 3]; 3],
}

pub type SharedRenderState = Arc<Mutex<NativeRenderState>>;

impl Default for NativeRenderState {
    fn default() -> Self {
        Self {
            clear_color: [0.025, 0.04, 0.09, 1.0],
            vertex_colors: [[0.08, 0.85, 0.78], [0.16, 0.35, 0.98], [0.75, 0.25, 0.96]],
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
            vertex_colors: self.vertex_colors,
        }
    }

    pub fn set_clear_color(&mut self, color: [f64; 4]) {
        self.clear_color = color.map(|component| component.clamp(0.0, 1.0));
    }

    pub fn set_vertex_color(&mut self, index: usize, color: [f64; 3]) {
        if let Some(vertex) = self.vertex_colors.get_mut(index) {
            *vertex = color.map(|component| component.clamp(0.0, 1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NativeRenderState;

    #[test]
    fn clamps_colors_to_gpu_range() {
        let mut state = NativeRenderState::default();
        state.set_clear_color([-1.0, 0.25, 2.0, 1.0]);
        state.set_vertex_color(1, [2.0, -1.0, 0.5]);
        let snapshot = state.snapshot();
        assert_eq!(snapshot.clear_color, [0.0, 0.25, 1.0, 1.0]);
        assert_eq!(snapshot.vertex_colors[1], [1.0, 0.0, 0.5]);
    }
}
