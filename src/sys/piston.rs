use clap::ArgMatches;
use glutin_window::GlutinWindow;
use piston::event_loop::{EventLoop, EventSettings, Events};
use piston::input;
use piston::input::{PressEvent, ReleaseEvent, RenderEvent, UpdateEvent};
use piston::window::WindowSettings;

use log::{debug, error, trace};
use std::collections::VecDeque;

use crate::gfx;
use crate::gfx::piston::OPENGL_VERSION;
use crate::gfx::piston::{gl, PistonBackend};
use crate::input::*;
use crate::sys::Sys;
use crate::vm::{VMSnapshot, VM};

pub struct PistonSys {
    gfx: Box<dyn PistonBackend>,

    window: GlutinWindow,
    events: Events,
    input: InputState,
    frames_to_wait: usize,
    fast_mode: bool,
    pause: bool,
    snapshot_cpt: usize,

    history: VecDeque<VMSnapshot>,
}

pub const WINDOW_RESOLUTION: [u32; 2] = [800, 600];

pub fn new(matches: &ArgMatches) -> Option<Box<dyn Sys>> {
    // TODO ups looks wrong?
    let events = Events::new(EventSettings::new()).ups(50).max_fps(50);

    let window: GlutinWindow = WindowSettings::new("Another World", WINDOW_RESOLUTION)
        .graphics_api(OPENGL_VERSION)
        .exit_on_esc(true)
        .build()
        .ok()?;

    let gfx = PistonSys::create_gfx(matches);

    Some(Box::new(PistonSys {
        gfx,
        events,
        window,
        input: InputState::new(),
        frames_to_wait: 0,
        fast_mode: false,
        pause: false,
        history: VecDeque::new(),
        snapshot_cpt: 0,
    }))
}

const MAX_GAME_SNAPSHOTS: usize = 50;

impl PistonSys {
    fn create_gfx(matches: &ArgMatches) -> Box<dyn PistonBackend> {
        match matches.value_of("render").unwrap_or("raster") {
            rdr @ "line" | rdr @ "poly" => {
                let poly_render = match rdr {
                    "line" => gl::PolyRender::Line,
                    "poly" => gl::PolyRender::Poly,
                    _ => panic!(),
                };
                Box::new(gl::new().set_poly_render(poly_render))
                    as Box<dyn gfx::piston::PistonBackend>
            }
            "raster" => Box::new(gfx::piston::raster::new()) as Box<dyn gfx::piston::PistonBackend>,
            _ => panic!("unexpected poly_render option"),
        }
    }

    fn take_snapshot(&mut self, vm: &VM) {
        self.history.push_front(VMSnapshot::new(
            vm.get_snapshot(),
            self.gfx.as_gfx().get_snapshot(),
        ));

        while self.history.len() > MAX_GAME_SNAPSHOTS {
            self.history.pop_back();
        }
    }

    // Returns true if the game should continue, false if we should quit.
    fn update(&mut self, vm: &mut VM) -> bool {
        if self.pause {
            return true;
        }

        vm.update_input(&self.input);

        let cycles = if self.fast_mode { 8 } else { 1 };
        for _ in 0..cycles {
            self.snapshot_cpt += 1;
            if self.snapshot_cpt == TICKS_PER_SNAPSHOT {
                self.take_snapshot(vm);
                self.snapshot_cpt = 0;
            }

            if self.frames_to_wait == 0 {
                if !vm.process(self.gfx.as_gfx()) {
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
}

const TICKS_PER_SNAPSHOT: usize = 200;

impl Sys for PistonSys {
    fn game_loop(&mut self, vm: &mut VM) {
        self.history.clear();
        self.take_snapshot(vm);

        while let Some(e) = self.events.next(&mut self.window) {
            if let Some(r) = e.render_args() {
                self.gfx.render(&r);
            }

            if e.update_args().is_some() && !self.update(vm) {
                break;
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
                        if let Some(state) = self.history.pop_front() {
                            state.restore(vm, self.gfx.as_gfx());
                            self.snapshot_cpt = 0;

                            // If we are back to the first state, keep a copy.
                            if self.history.is_empty() {
                                self.take_snapshot(vm);
                            }
                        }
                    }
                    piston::keyboard::Key::N => {
                        if self.pause {
                            self.take_snapshot(vm);
                            vm.update_input(&self.input);
                            vm.process(self.gfx.as_gfx());
                            self.frames_to_wait = vm.get_frames_to_wait();
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
