//! A simple sys that is able to run any SDL2-based graphics system, accelerated or not. It does
//! not provide any fancy features - just the basic game.

use clap::ArgMatches;
use sdl2::event::Event;
use sdl2::event::WindowEvent;
use sdl2::keyboard::Keycode;
use sdl2::rect::Rect;
use sdl2::Sdl;
use tracing::error;

use crate::audio::sdl2::Sdl2Audio;
use crate::audio::MusicPlayer;
use crate::gfx::sdl2::canvas_gfx::Sdl2CanvasGfx;
use crate::gfx::sdl2::gl_gfx::RenderingMode;
use crate::gfx::sdl2::gl_gfx::Sdl2GlGfx;
use crate::gfx::sdl2::Sdl2Gfx;
use crate::gfx::{self};
use crate::input::ButtonState;
use crate::input::InputState;
use crate::input::LeftRightDir;
use crate::input::UpDownDir;
use crate::sys::Sys;
use crate::vm::Vm;
use crate::vm::VmSnapshot;

use std::collections::VecDeque;
use std::thread;
use std::time::Duration;
use std::time::Instant;

const TICKS_PER_SECOND: u64 = 50;
const DURATION_PER_TICK: Duration =
    // Use microseconds to add precision.
    Duration::from_micros(1_000_000 / TICKS_PER_SECOND);

pub struct Sdl2Sys<D: Sdl2Gfx> {
    sdl_context: Sdl,
    display: D,
    audio_device: Sdl2Audio,
}

/// Creates a dynamic SDL Sys instance from the command-line arguments.
pub fn new_from_args(matches: &ArgMatches) -> Option<Box<dyn Sys>> {
    let sdl_context = sdl2::init()
        .map_err(|e| {
            error!("Failed to initialize SDL: {}", e);
        })
        .ok()?;

    let audio_device = Sdl2Audio::new(&sdl_context, 22050)
        .map_err(|e| {
            error!("Failed to initialize SDL audio device: {}", e);
        })
        .ok()?;

    let backend = matches.value_of("render").unwrap_or("raster");
    match backend {
        "raster" => Some(Box::new(Sdl2Sys {
            display: Sdl2CanvasGfx::new(&sdl_context).ok()?,
            sdl_context,
            audio_device,
        }) as Box<dyn Sys>),
        "gl_raster" => Some(Box::new(Sdl2Sys {
            display: Sdl2GlGfx::new(&sdl_context, RenderingMode::Raster).ok()?,
            sdl_context,
            audio_device,
        }) as Box<dyn Sys>),
        "gl_poly" => Some(Box::new(Sdl2Sys {
            display: Sdl2GlGfx::new(&sdl_context, RenderingMode::Poly).ok()?,
            sdl_context,
            audio_device,
        }) as Box<dyn Sys>),
        "gl_line" => Some(Box::new(Sdl2Sys {
            display: Sdl2GlGfx::new(&sdl_context, RenderingMode::Line).ok()?,
            sdl_context,
            audio_device,
        }) as Box<dyn Sys>),
        // Just a test for Sdl2Gfx trait object.
        "gl_raster_boxed" => Some(Box::new(Sdl2Sys {
            display: Box::new(Sdl2GlGfx::new(&sdl_context, RenderingMode::Raster).ok()?)
                as Box<dyn Sdl2Gfx>,
            sdl_context,
            audio_device,
        }) as Box<dyn Sys>),
        _ => None,
    }
}

struct Snapshot {
    // Full snapshot of the VM state.
    snapshot: VmSnapshot,
    // Whether the snapshot has just been restored and we should skip it if 'B' is pressed.
    just_restored: bool,
}

impl From<VmSnapshot> for Snapshot {
    fn from(snapshot: VmSnapshot) -> Self {
        Self {
            snapshot,
            just_restored: false,
        }
    }
}

fn take_snapshot<G: gfx::Gfx + ?Sized>(history: &mut VecDeque<Snapshot>, vm: &Vm, gfx: &G) {
    const MAX_GAME_SNAPSHOTS: usize = 50;

    history.push_front(VmSnapshot::new(vm, gfx).into());

    while history.len() > MAX_GAME_SNAPSHOTS {
        history.pop_back();
    }
}

impl<D: Sdl2Gfx> Sys for Sdl2Sys<D> {
    fn game_loop(&mut self, vm: &mut Vm) {
        // Events, time and input
        let mut sdl_events = self.sdl_context.event_pump().unwrap();
        let mut next_tick_time = Instant::now();
        let mut ticks_to_wait = 0;
        let mut input = InputState::new();

        // Modes
        let mut fast_mode = false;
        let mut pause = false;

        // State rewind
        const TICKS_PER_SNAPSHOT: usize = 200;
        let mut history: VecDeque<Snapshot> = VecDeque::new();
        let mut snapshot_cpt = 0;
        take_snapshot(&mut history, vm, &self.display);

        // Ignore keys presses from being handled right after window has gained
        // focus to avoid e.g escape being considered if esc was part of the
        // shortcut that made us gain focus.
        const KEYPRESS_COOLDOWN_TICKS: usize = 1;
        let mut keypress_cooldown = KEYPRESS_COOLDOWN_TICKS;

        let mut released_keys = Vec::new();
        'run: loop {
            // Update input
            released_keys.clear();
            for event in sdl_events.poll_iter() {
                match event {
                    Event::Quit { .. } => break 'run,
                    Event::Window {
                        win_event: WindowEvent::FocusGained,
                        ..
                    } => keypress_cooldown = KEYPRESS_COOLDOWN_TICKS,
                    Event::KeyDown {
                        keycode: Some(key),
                        repeat: false,
                        ..
                    } if keypress_cooldown == 0 => match key {
                        Keycode::Escape => break 'run,
                        Keycode::Left => input.horizontal = LeftRightDir::Left,
                        Keycode::Right => input.horizontal = LeftRightDir::Right,
                        Keycode::Up => input.vertical = UpDownDir::Up,
                        Keycode::Down => input.vertical = UpDownDir::Down,
                        Keycode::Space => input.button = ButtonState::Pushed,
                        Keycode::F => fast_mode = true,
                        Keycode::P => {
                            pause ^= true;
                            if pause {
                                self.audio_device.pause();
                            } else {
                                self.audio_device.resume();
                            }
                        }
                        Keycode::B => {
                            if let Some(state) = history.front() {
                                // If the state has just been restored, remove it unless that would
                                // mean we are left with just one state.
                                if state.just_restored && history.len() >= 2 {
                                    history.pop_front();
                                }
                            }

                            if let Some(state) = history.front_mut() {
                                state.snapshot.restore(vm, &mut self.display);
                                snapshot_cpt = 0;
                                state.just_restored = true;
                            }
                        }
                        Keycode::N if pause => {
                            take_snapshot(&mut history, vm, &self.display);
                            vm.update_input(&input);
                            if let Some(value_of_0xf4) = self.audio_device.take_value_of_0xf4() {
                                vm.set_reg(0xf4, value_of_0xf4);
                            }
                            vm.process_round(&mut self.display, &mut self.audio_device);
                            ticks_to_wait = vm.get_frames_to_wait();
                        }
                        _ => {}
                    },
                    // Store key released events so they can be processed later after the VM update.
                    // This gives the game a chance to proceed keys that have been both pressed and
                    // released within the same cycle.
                    Event::KeyUp {
                        keycode: Some(key),
                        repeat: false,
                        ..
                    } => released_keys.push(key),
                    _ => {}
                }

                // Give the display subsystem a chance to manage its own input (hack!)
                self.display.handle_event(&event);
            }
            vm.update_input(&input);

            // Now update the state of all the released keys.
            for key in &released_keys {
                match key {
                    Keycode::Left | Keycode::Right => input.horizontal = LeftRightDir::Neutral,
                    Keycode::Up | Keycode::Down => input.vertical = UpDownDir::Neutral,
                    Keycode::Space => input.button = ButtonState::Released,
                    Keycode::F => fast_mode = false,
                    _ => {}
                }
            }

            // Decrease keypress cooldown if we just gained focus.
            keypress_cooldown = keypress_cooldown.saturating_sub(1);

            // Wait until the time slice for the current game tick is elapsed.
            let now = Instant::now();
            match now - next_tick_time {
                d if d < DURATION_PER_TICK => {
                    thread::sleep(DURATION_PER_TICK - d);
                }
                _ => (),
            }
            let now = Instant::now();

            // Get how many ticks we need to run and set next_tick_time to the next tick.
            let ticks_to_run = if pause {
                next_tick_time = Instant::now();
                0
            } else if fast_mode {
                next_tick_time = Instant::now();
                8
            } else {
                let mut ticks_to_run = 1;
                next_tick_time += DURATION_PER_TICK;
                while now + (DURATION_PER_TICK * ticks_to_run) < next_tick_time {
                    ticks_to_run += 1;
                }
                ticks_to_run
            };

            // If we try to restore a state twice within that cooldown, we will restore the state
            // before that one instead.
            const SNAPSHOT_REMOVAL_COOLDOWN: usize = 10;
            // Update VM state
            for _ in 0..ticks_to_run {
                snapshot_cpt += 1;

                if snapshot_cpt == SNAPSHOT_REMOVAL_COOLDOWN {
                    if let Some(snapshot) = history.front_mut() {
                        snapshot.just_restored = false;
                    }
                }

                if snapshot_cpt == TICKS_PER_SNAPSHOT {
                    take_snapshot(&mut history, vm, &self.display);
                    snapshot_cpt = 0;
                }

                if ticks_to_wait == 0 {
                    if let Some(value_of_0xf4) = self.audio_device.take_value_of_0xf4() {
                        vm.set_reg(0xf4, value_of_0xf4);
                    }
                    if !vm.process_round(&mut self.display, &mut self.audio_device) {
                        error!("0 threads to run, exiting.");
                        break 'run;
                    }

                    ticks_to_wait = vm.get_frames_to_wait();
                }
                ticks_to_wait -= 1;
            }

            fn div_by_screen_ratio(x: u32) -> u32 {
                x * 5 / 8
            }

            fn mul_by_screen_ratio(x: u32) -> u32 {
                x * 8 / 5
            }

            // Compute destination rectangle of game screen
            let viewport = {
                let (w, h) = self.display.window().drawable_size();
                Rect::new(0, 0, w, h)
            };
            let viewport_dst = if div_by_screen_ratio(viewport.width()) < viewport.height() {
                let w = viewport.width();
                let h = div_by_screen_ratio(viewport.width());
                sdl2::rect::Rect::new(0, (viewport.height() - h) as i32 / 2, w, h)
            } else {
                let w = mul_by_screen_ratio(viewport.height());
                let h = viewport.height();
                sdl2::rect::Rect::new((viewport.width() - w) as i32 / 2, 0, w, h)
            };

            self.display.show_game_framebuffer(&viewport_dst);
            self.display.present();
        }
    }
}
