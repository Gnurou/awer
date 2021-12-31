pub mod gl;
pub mod raster;

use crate::gfx;
use sdl2::{event::Event, rect::Rect, video::Window};

/// Initial size of the window when using this renderer.
pub const WINDOW_RESOLUTION: [u32; 2] = [1280, 800];

/// Trait for handling the display for `Sdl2Sys`.
pub trait Sdl2Display {
    /// Blit the rendered framebuffer into the `dst` rectangle of the actual
    /// display.
    fn blit_game(&mut self, dst: &Rect);
    /// Page-flip the display.
    fn present(&mut self);

    /// Returns a reference to the renderer.
    fn as_renderer(&self) -> &dyn gfx::Renderer;
    /// Returns a mutable reference to the renderer.
    fn as_renderer_mut(&mut self) -> &mut dyn gfx::Renderer;

    /// Returns the window the renderer will render into.
    fn window(&self) -> &Window;

    /// Gives the renderer a change to handle its own input, to e.g. change
    /// rendering parameters. Also useful to catch window resize events.
    fn handle_events(&mut self, _events: &[Event]) {}
}
