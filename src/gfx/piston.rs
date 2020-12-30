pub mod gl;
pub mod raster;

use opengl_graphics::OpenGL;
use piston::input::RenderArgs;

pub const OPENGL_VERSION: OpenGL = OpenGL::V3_2;

pub trait PistonBackend {
    fn render(&mut self, args: &RenderArgs);
    fn as_gfx(&mut self) -> &mut dyn super::Backend;
}
