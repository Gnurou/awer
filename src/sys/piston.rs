use glutin_window::GlutinWindow;
use piston::event_loop::{EventLoop, EventSettings, Events};
use piston::input;
use piston::input::{PressEvent, ReleaseEvent, RenderEvent, UpdateEvent};
use piston::window::WindowSettings;

use log::{debug, error, trace};
use std::collections::VecDeque;

use crate::gfx;
use crate::gfx::piston::PistonBackend;
use crate::gfx::piston::OPENGL_VERSION;
use crate::input::*;
use crate::sys::Sys;
use crate::vm::{VMState, VM};

pub struct PistonSys {
    window: GlutinWindow,
    events: Events,
    input: InputState,
    frames_to_wait: usize,
    fast_mode: bool,
    pause: bool,

    history: VecDeque<VMState>,
}

pub const WINDOW_RESOLUTION: [u32; 2] = [800, 600];

pub fn new() -> PistonSys {
    // TODO ups looks wrong?
    let events = Events::new(EventSettings::new()).ups(50).max_fps(50);

    let window: GlutinWindow = WindowSettings::new("Another World", WINDOW_RESOLUTION)
        .graphics_api(OPENGL_VERSION)
        .exit_on_esc(true)
        .build()
        .unwrap();

    PistonSys {
        events,
        window,
        input: InputState::new(),
        frames_to_wait: 0,
        fast_mode: false,
        pause: false,
        history: VecDeque::new(),
    }
}

impl PistonSys {
    pub fn game_loop(&mut self, vm: &mut VM, gfx: &mut dyn PistonBackend) {
        let mut cpt = 0usize;
        self.history.clear();
        self.history.push_front(vm.get_snapshot());

        while let Some(e) = self.events.next(&mut self.window) {
            if let Some(r) = e.render_args() {
                gfx.render(&r);
            }

            if e.update_args().is_some() {
                if !self.pause && self.frames_to_wait == 0 {
                    if cpt == 50 {
                        while self.history.len() >= 10 {
                            self.history.pop_back();
                        }
                        self.history.push_front(vm.get_snapshot());
                        cpt = 0;
                    } else {
                        cpt += 1;
                    }
                }

                if !self.update(vm, gfx.as_gfx()) {
                    break;
                }
            }

            if let Some(input::Button::Keyboard(c)) = e.press_args() {
                trace!("pressed {:?}", c);
                match c {
                    piston::keyboard::Key::Left => self.input.horizontal = LeftRightDir::Left,
                    piston::keyboard::Key::Right => self.input.horizontal = LeftRightDir::Right,
                    piston::keyboard::Key::Up => self.input.vertical = UpDownDir::Up,
                    piston::keyboard::Key::Down => self.input.vertical = UpDownDir::Down,
                    piston::keyboard::Key::Space => self.input.button = ButtonState::Pushed,
                    piston::keyboard::Key::F => self.fast_mode = true,
                    piston::keyboard::Key::P => {
                        // Flip
                        self.pause ^= true;
                    }
                    piston::keyboard::Key::B => {
                        // TODO prevent key repeat here?
                        if let Some(state) = self.history.front() {
                            vm.set_snapshot(state.clone());
                            if self.history.len() > 1 {
                                self.history.pop_front();
                            }
                        }
                    }
                    _ => (),
                }
            }
            if let Some(input::Button::Keyboard(c)) = e.release_args() {
                trace!("released {:?}", c);
                match c {
                    piston::keyboard::Key::Left | piston::keyboard::Key::Right => {
                        self.input.horizontal = LeftRightDir::Neutral
                    }
                    piston::keyboard::Key::Up | piston::keyboard::Key::Down => {
                        self.input.vertical = UpDownDir::Neutral
                    }
                    piston::keyboard::Key::Space => self.input.button = ButtonState::Released,
                    piston::keyboard::Key::F => self.fast_mode = false,
                    _ => (),
                }
            }
        }
    }
}

impl Sys for PistonSys {
    fn update(&mut self, vm: &mut VM, gfx: &mut dyn gfx::Backend) -> bool {
        if self.pause {
            return true;
        }
        vm.update_input(self.get_input());

        let cycles = if self.fast_mode { 8 } else { 1 };
        for _ in 0..cycles {
            if self.frames_to_wait <= 0 {
                if !vm.process(gfx) {
                    error!("0 threads to run, exiting.");
                    return false;
                }
                self.frames_to_wait = vm.get_frames_to_wait();
                debug!(
                    "Need to wait {} frames for this cycle.",
                    self.frames_to_wait
                );
            }
            self.frames_to_wait -= 1;
        }

        true
    }

    fn get_input(&self) -> &InputState {
        &self.input
    }
}
