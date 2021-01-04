pub mod raster;

use crate::gfx;
use sdl2::rect::Rect;

pub trait SDL2Renderer {
    /// Returns a rectangle of the size of the visible area (i.e. window).
    fn viewport(&self) -> Rect;
    /// Render the current game state into an off-screen buffer.
    fn render_game(&mut self);
    /// Blit the rendered off-screen buffer into the `dst` rectangle of the
    /// actual display.
    fn blit_game(&mut self, dst: Rect);
    /// Page-flip the display.
    fn present(&mut self);

    /// Returns a reference to the graphics backend.
    fn as_gfx(&self) -> &dyn gfx::Backend;
    /// Returns a mutable reference to the graphics backend.
    fn as_gfx_mut(&mut self) -> &mut dyn gfx::Backend;
}
