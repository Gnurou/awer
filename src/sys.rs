#[cfg(feature = "sdl2-sys")]
pub mod sdl2;

use crate::vm::Vm;

pub trait Sys {
    fn game_loop(&mut self, vm: &mut Vm);
}

/// Trait for elements which state can be captured to be restored afterwards.
pub trait Snapshotable {
    type State;

    /// Take a snapshot of the current state. The returned value can be restored by calling
    /// `restore_snapshot` on `self`.
    fn take_snapshot(&self) -> Self::State;

    /// Restore a previously taken snapshot.
    ///
    /// `snapshot` must be a value previously returned from `take_snapshot` called on `self`.
    ///
    /// Returns `true` if the snapshot has been successfully reapplied, `false` otherwise (might
    /// happen if `snapshot` did not come from `self`).
    ///
    fn restore_snapshot(&mut self, snapshot: Self::State) -> bool;
}
