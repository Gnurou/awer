#[cfg(feature = "piston-sys")]
pub mod piston;

use crate::vm::VM;

pub trait Sys {
    fn game_loop(&mut self, vm: &mut VM);
}
