use super::Program;

pub struct FillRenderer;

impl Program for FillRenderer {}

impl FillRenderer {
    pub fn new() -> Self {
        Self
    }

    pub fn fill(&mut self, color: u8) {
        unsafe {
            gl::ClearColor(color as f32 / 256.0, 0.0, 0.0, 0.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
    }
}
