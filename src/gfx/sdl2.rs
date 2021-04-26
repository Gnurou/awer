pub mod gl;
pub mod raster;

use crate::gfx;
use sdl2::{event::Event, rect::Rect, video::Window};

/// Initial size of the window when using this renderer.
pub const WINDOW_RESOLUTION: [u32; 2] = [1280, 800];

pub trait Sdl2Renderer {
    /// Blit the rendered framebuffer into the `dst` rectangle of the actual
    /// display.
    fn blit_game(&mut self, dst: &Rect);
    /// Page-flip the display.
    fn present(&mut self);

    /// Returns a reference to the graphics backend.
    fn as_gfx(&self) -> &dyn gfx::Backend;
    /// Returns a mutable reference to the graphics backend.
    fn as_gfx_mut(&mut self) -> &mut dyn gfx::Backend;

    /// Returns the window the renderer will render into.
    fn window(&self) -> &Window;

    /// Gives the renderer a change to handle its own input, to e.g. change
    /// rendering parameters. Also useful to catch window resize events.
    fn handle_events(&mut self, _events: &[Event]) {}
}
