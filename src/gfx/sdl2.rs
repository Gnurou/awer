pub mod canvas;
pub mod gl;

use std::ops::DerefMut;

use sdl2::{event::Event, rect::Rect, video::Window};

use super::Gfx;

/// Initial size of the window when using this renderer.
pub const WINDOW_RESOLUTION: [u32; 2] = [1280, 800];

/// Trait for handling display for `Sdl2Sys`, while providing access to common graphics methods.
pub trait Sdl2Gfx: Gfx {
    /// Display the current framebuffer into the `dst` rectangle of the actual display.
    fn show_game_framebuffer(&mut self, dst: &Rect);

    /// Returns the window the renderer will render into.
    fn window(&self) -> &Window;

    /// Gives the renderer a chance to handle its own input, to e.g. change rendering parameters.
    /// Also useful to catch window resize events.
    fn handle_events(&mut self, _events: &[Event]) {}
}

/// Proxy implementation for containers of `Sdl2Gfx`.
impl<D: Sdl2Gfx + ?Sized + 'static, C: DerefMut<Target = D> + Gfx> Sdl2Gfx for C {
    fn show_game_framebuffer(&mut self, dst: &Rect) {
        self.deref_mut().show_game_framebuffer(dst)
    }

    fn window(&self) -> &Window {
        self.deref().window()
    }

    fn handle_events(&mut self, events: &[Event]) {
        self.deref_mut().handle_events(events)
    }
}
