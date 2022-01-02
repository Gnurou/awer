use clap::ArgMatches;
use log::error;
use sdl2::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    rect::Rect,
    Sdl,
};

use crate::{
    gfx::{
        self,
        sdl2::{
            canvas::Sdl2CanvasGfx,
            gl::{RenderingMode, Sdl2GlGfx},
            Sdl2Gfx,
        },
    },
    input::{ButtonState, InputState, LeftRightDir, UpDownDir},
    sys::Sys,
    vm::{Vm, VmSnapshot},
};

use std::{
    collections::VecDeque,
    thread,
    time::{Duration, Instant},
};

const TICKS_PER_SECOND: u64 = 50;
const DURATION_PER_TICK: Duration = Duration::from_millis(1000 / TICKS_PER_SECOND);

pub struct Sdl2Sys<D: Sdl2Gfx + ?Sized> {
    sdl_context: Sdl,
    display: D,
}

/// Creates a dynamic SDL Sys instance from the command-line arguments.
pub fn new_from_args(matches: &ArgMatches) -> Option<Box<dyn Sys>> {
    let sdl_context = sdl2::init()
        .map_err(|e| {
            log::error!("Failed to initialize SDL: {}", e);
        })
        .ok()?;

    let backend = matches.value_of("render").unwrap_or("raster");
    match backend {
        "raster" => Some(Box::new(Sdl2Sys {
            display: Sdl2CanvasGfx::new(&sdl_context).ok()?,
            sdl_context,
        }) as Box<dyn Sys>),
        "gl_raster" => Some(Box::new(Sdl2Sys {
            display: Sdl2GlGfx::new(&sdl_context, RenderingMode::Raster).ok()?,
            sdl_context,
        }) as Box<dyn Sys>),
        "gl_poly" => Some(Box::new(Sdl2Sys {
            display: Sdl2GlGfx::new(&sdl_context, RenderingMode::Poly).ok()?,
            sdl_context,
        }) as Box<dyn Sys>),
        "gl_line" => Some(Box::new(Sdl2Sys {
            display: Sdl2GlGfx::new(&sdl_context, RenderingMode::Line).ok()?,
            sdl_context,
        }) as Box<dyn Sys>),
        // Just a test for Sdl2Gfx trait object.
        "gl_raster_boxed" => Some(Box::new(Sdl2Sys {
            display: Box::new(Sdl2GlGfx::new(&sdl_context, RenderingMode::Raster).ok()?)
                as Box<dyn Sdl2Gfx>,
            sdl_context,
        }) as Box<dyn Sys>),
        _ => None,
    }
}

fn take_snapshot<G: gfx::Gfx + ?Sized>(history: &mut VecDeque<VmSnapshot>, vm: &Vm, gfx: &G) {
    const MAX_GAME_SNAPSHOTS: usize = 50;

    history.push_front(VmSnapshot::new(vm, gfx));

    while history.len() > MAX_GAME_SNAPSHOTS {
        history.pop_back();
    }
}

impl<D: Sdl2Gfx + ?Sized> Sys for Sdl2Sys<D> {
    fn game_loop(&mut self, vm: &mut crate::vm::Vm) {
        // Events, time and input
        let mut sdl_events = self.sdl_context.event_pump().unwrap();
        let mut last_tick_time = Instant::now();
        let mut ticks_to_wait = 0;
        let mut input = InputState::new();

        // Modes
        let mut fast_mode = false;
        let mut pause = false;

        // State rewind
        const TICKS_PER_SNAPSHOT: usize = 200;
        let mut history: VecDeque<VmSnapshot> = VecDeque::new();
        let mut snapshot_cpt = 0;
        take_snapshot(&mut history, vm, &self.display);

        // Ignore keys presses from being handled right after window has gained
        // focus to avoid e.g escape being considered if esc was part of the
        // shortcut that made us gain focus.
        const KEYPRESS_COOLDOWN_TICKS: usize = 1;
        let mut keypress_cooldown = KEYPRESS_COOLDOWN_TICKS;

        let mut pending_events: Vec<Event> = Vec::new();
        'run: loop {
            // Update input
            // TODO keep the key released events in a separate input state, so
            // we process all key pressed events when updating the VM even if
            // press/release occured within the same game tick.
            // TODO use wait_event_timeout() or wait_timeout_iter() to process
            // events in real-time while maintaining game cadence.

            pending_events.clear();
            for event in sdl_events.poll_iter() {
                pending_events.push(event);
            }

            for event in &pending_events {
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
                        Keycode::P => pause ^= true,
                        Keycode::B => {
                            if let Some(state) = history.pop_front() {
                                state.restore(vm, &mut self.display);
                                snapshot_cpt = 0;
                            }

                            // If we are back to the first state, keep a copy.
                            if history.is_empty() {
                                take_snapshot(&mut history, vm, &self.display);
                            }
                        }
                        Keycode::N if pause => {
                            take_snapshot(&mut history, vm, &self.display);
                            vm.update_input(&input);
                            vm.process(&mut self.display);
                            ticks_to_wait = vm.get_frames_to_wait();
                        }
                        _ => {}
                    },
                    Event::KeyUp {
                        keycode: Some(key),
                        repeat: false,
                        ..
                    } => match key {
                        Keycode::Left | Keycode::Right => input.horizontal = LeftRightDir::Neutral,
                        Keycode::Up | Keycode::Down => input.vertical = UpDownDir::Neutral,
                        Keycode::Space => input.button = ButtonState::Released,
                        Keycode::F => fast_mode = false,
                        _ => {}
                    },
                    _ => {}
                }
            }
            vm.update_input(&input);
            self.display.handle_events(&pending_events);

            // Decrease keypress cooldown if we just gained focus.
            if keypress_cooldown > 0 {
                keypress_cooldown -= 1;
            }

            let mut duration_since_last_tick = Instant::now().duration_since(last_tick_time);

            // Wait until the time slice for the current game tick is elapsed
            if duration_since_last_tick < DURATION_PER_TICK {
                thread::sleep(DURATION_PER_TICK - duration_since_last_tick);
            }
            duration_since_last_tick = Instant::now().duration_since(last_tick_time);

            // Update VM state
            let ticks_to_run = if pause {
                last_tick_time = Instant::now();
                0
            } else if fast_mode {
                last_tick_time = Instant::now();
                8
            } else {
                let ticks_to_run = duration_since_last_tick.as_millis() as u32
                    / DURATION_PER_TICK.as_millis() as u32;
                last_tick_time += DURATION_PER_TICK * ticks_to_run;
                std::cmp::min(ticks_to_run, 16)
            };

            for _ in 0..ticks_to_run {
                snapshot_cpt += 1;
                if snapshot_cpt == TICKS_PER_SNAPSHOT {
                    take_snapshot(&mut history, vm, &self.display);
                    snapshot_cpt = 0;
                }

                if ticks_to_wait == 0 {
                    if !vm.process(&mut self.display) {
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

            self.display.blit_game(&viewport_dst);
        }
    }
}
