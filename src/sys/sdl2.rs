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
            gl::{RenderingMode, Sdl2GlRenderer},
            raster::Sdl2RasterRenderer,
            Sdl2Renderer,
        },
    },
    input::{ButtonState, InputState, LeftRightDir, UpDownDir},
    vm::{Vm, VmSnapshot},
};

use super::Sys;

use std::thread;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const TICKS_PER_SECOND: u64 = 50;
const DURATION_PER_TICK: Duration = Duration::from_millis(1000 / TICKS_PER_SECOND);

pub struct Sdl2Sys {
    sdl_context: Sdl,
    renderer: Box<dyn Sdl2Renderer>,
}

pub fn new(matches: &ArgMatches) -> Option<Box<dyn Sys>> {
    let sdl_context = sdl2::init()
        .map_err(|e| {
            eprintln!("Failed to initialize SDL: {}", e);
        })
        .ok()?;

    let renderer: Box<dyn Sdl2Renderer> = match matches.value_of("render").unwrap_or("raster") {
        "raster" => Sdl2RasterRenderer::new(&sdl_context).ok()?,
        "gl_raster" => Sdl2GlRenderer::new(&sdl_context, RenderingMode::Raster).ok()?,
        "gl_poly" => Sdl2GlRenderer::new(&sdl_context, RenderingMode::Poly).ok()?,
        "gl_line" => Sdl2GlRenderer::new(&sdl_context, RenderingMode::Line).ok()?,
        _ => return None,
    };

    Some(Box::new(Sdl2Sys {
        sdl_context,
        renderer,
    }))
}

fn take_snapshot(history: &mut VecDeque<VmSnapshot>, vm: &Vm, gfx: &dyn gfx::Backend) {
    const MAX_GAME_SNAPSHOTS: usize = 50;

    history.push_front(VmSnapshot::new(vm.get_snapshot(), gfx.get_snapshot()));

    while history.len() > MAX_GAME_SNAPSHOTS {
        history.pop_back();
    }
}

impl Sys for Sdl2Sys {
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
        take_snapshot(&mut history, &vm, self.renderer.as_gfx());

        // Ignore keys presses from being handled right after window has gained
        // focus to avoid e.g escape being considered if esc was part of the
        // shortcut that made us gain focus.
        const KEYPRESS_COOLDOWN_TICKS: usize = 1;
        let mut keypress_cooldown = KEYPRESS_COOLDOWN_TICKS;

        'run: loop {
            // Update input
            // TODO keep the key released events in a separate input state, so
            // we process all key pressed events when updating the VM even if
            // press/release occured within the same game tick.
            // TODO use wait_event_timeout() or wait_timeout_iter() to process
            // events in real-time while maintaining game cadence.
            for event in sdl_events.poll_iter() {
                match event {
                    Event::Quit { .. } => break 'run,
                    Event::Window {
                        win_event: WindowEvent::FocusGained,
                        ..
                    } => keypress_cooldown = KEYPRESS_COOLDOWN_TICKS,
                    Event::Window {
                        win_event: WindowEvent::Resized(w, h),
                        ..
                    } => self.renderer.window_resized(w as usize, h as usize),
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
                                state.restore(vm, self.renderer.as_gfx_mut());
                                snapshot_cpt = 0;
                            }

                            // If we are back to the first state, keep a copy.
                            if history.is_empty() {
                                take_snapshot(&mut history, vm, self.renderer.as_gfx());
                            }
                        }
                        Keycode::N if pause => {
                            take_snapshot(&mut history, vm, self.renderer.as_gfx());
                            vm.update_input(&input);
                            vm.process(self.renderer.as_gfx_mut());
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

            // Decrease keypress cooldown if we just gained focus.
            if keypress_cooldown > 0 {
                keypress_cooldown -= 1;
            }

            // Update VM state
            vm.update_input(&input);
            let cycles = if pause {
                0
            } else if fast_mode {
                8
            } else {
                1
            };
            for _ in 0..cycles {
                snapshot_cpt += 1;
                if snapshot_cpt == TICKS_PER_SNAPSHOT {
                    take_snapshot(&mut history, &vm, self.renderer.as_gfx());
                    snapshot_cpt = 0;
                }

                if ticks_to_wait == 0 {
                    if !vm.process(self.renderer.as_gfx_mut()) {
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

            // Wait until the time slice for the current game tick is elapsed
            // TODO wait for ticks_to_wait?
            let duration_since_last_tick = Instant::now().duration_since(last_tick_time);
            if duration_since_last_tick < DURATION_PER_TICK {
                thread::sleep(DURATION_PER_TICK - duration_since_last_tick);
            }
            last_tick_time = Instant::now();

            // Compute destination rectangle of game screen
            let viewport = {
                let (w, h) = self.renderer.window().size();
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

            self.renderer.blit_game(&viewport_dst);
            self.renderer.present();
        }
    }
}
