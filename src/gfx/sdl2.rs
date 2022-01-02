pub mod canvas;
pub mod gl;

use sdl2::{event::Event, rect::Rect, video::Window};

use super::Gfx;

/// Initial size of the window when using this renderer.
pub const WINDOW_RESOLUTION: [u32; 2] = [1280, 800];

/// Trait for handling display for `Sdl2Sys`, while providing access to regular graphics methods.
pub trait Sdl2Display {
    /// Blit the rendered framebuffer into the `dst` rectangle of the actual
    /// display and display it.
    fn blit_game(&mut self, dst: &Rect);

    /// Returns the window the renderer will render into.
    fn window(&self) -> &Window;

    /// Gives the renderer a chance to handle its own input, to e.g. change rendering parameters.
    /// Also useful to catch window resize events.
    fn handle_events(&mut self, _events: &[Event]) {}

    /// Return a reference to the underlying `Gfx` implementation.
    fn as_gfx(&self) -> &dyn Gfx;

    /// Return a mutable reference to the underlying `Gfx` implementation.
    fn as_gfx_mut(&mut self) -> &mut dyn Gfx;
}
