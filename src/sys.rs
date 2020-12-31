#[cfg(feature = "piston-sys")]
pub mod piston;

#[cfg(feature = "sdl2-sys")]
pub mod sdl2;

use crate::vm::VM;

pub trait Sys {
    fn game_loop(&mut self, vm: &mut VM);
}
