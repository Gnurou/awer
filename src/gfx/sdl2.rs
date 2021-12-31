pub mod gl;
pub mod raster;

use sdl2::{event::Event, rect::Rect, video::Window};

/// Initial size of the window when using this renderer.
pub const WINDOW_RESOLUTION: [u32; 2] = [1280, 800];

/// Trait for handling display for `Sdl2Sys`.
pub trait Sdl2Display {
    /// Blit the rendered framebuffer into the `dst` rectangle of the actual
    /// display.
    fn blit_game(&mut self, dst: &Rect);
    /// Page-flip the display.
    fn present(&mut self);

    /// Returns the window the renderer will render into.
    fn window(&self) -> &Window;

    /// Gives the renderer a chance to handle its own input, to e.g. change
    /// rendering parameters. Also useful to catch window resize events.
    fn handle_events(&mut self, _events: &[Event]) {}
}
