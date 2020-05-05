pub mod piston;

use crate::gfx;
use crate::input;
use crate::vm::VM;

trait Sys {
    // Returns true if the game should continue, false if we should quit.
    fn update(&mut self, vm: &mut VM, gfx: &mut dyn gfx::Backend) -> bool;
    fn get_input(&self) -> &input::InputState;
}
