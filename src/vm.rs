#![allow(dead_code)]

mod ops;

use self::ops::*;

use std::any::Any;
use std::fmt;
use std::io::Cursor;
use std::io::Result;
use std::io::Seek;
use std::io::SeekFrom;
use std::mem::{transmute, MaybeUninit};
use std::ops::Deref;

use log::{debug, info};

use super::gfx;

use super::res::ResourceManager;

use crate::scenes;
use crate::sys::Snapshotable;
use crate::{
    input::*,
    strings::{self, GameStrings},
};

use byteorder::{ReadBytesExt, BE};

const VM_NUM_THREADS: usize = 64;
const VM_NUM_VARIABLES: usize = 256;

const VM_VARIABLE_RANDOM_SEED: usize = 0x3c; // 60
const VM_VARIABLE_LAST_KEYCHAR: usize = 0xda; // 218
const VM_VARIABLE_HERO_POS_UPDOWN: usize = 0xe5; // 229
const VM_VARIABLE_SND_SYNC: usize = 0xf4; // 244
                                          // 0: max details.
                                          // 1: remove reflections. (?)
const VM_VARIABLE_GFX_DETAIL: usize = 0xf6; // 246
                                            // Design doc is not very legible. This may be used to control how many frames
                                            // we took to render and run the game logic, as a way to pace the game?
const VM_VARIABLE_SLICES_USED: usize = 0xf7; // 247
const VM_VARIABLE_SCROLL_Y: usize = 0xf9; // 249
const VM_VARIABLE_HERO_ACTION: usize = 0xfa; // 250
const VM_VARIABLE_HERO_POS_JUMP_DOWN: usize = 0xfb; // 251
const VM_VARIABLE_HERO_POS_LEFT_RIGHT: usize = 0xfc; // 252
const VM_VARIABLE_HERO_POS_MASK: usize = 0xfd; // 253
const VM_VARIABLE_HERO_ACTION_POS_MASK: usize = 0xfe; // 254
const VM_VARIABLE_PAUSE_SLICES: usize = 0xff; // 255

#[derive(Clone, Copy)]
enum ThreadState {
    Inactive,
    Active(u64),
    Paused(u64),
}

#[derive(Clone)]
pub struct Thread {
    state: ThreadState,
    // State to set this thread into for the next cycle.
    requested_state: Option<ThreadState>,
    // Return address for jsr/return ops
    call_stack: Vec<u64>,
}

// TODO move into own module?
// We should be able to replace this state with an earlier state (from the same
// scene) and have the game catch up painlessly.
#[derive(Clone)]
pub struct VmState {
    // TODO looks like registers should be initialized with random values
    // to give a random seed?
    regs: [i16; VM_NUM_VARIABLES],
    threads: [Thread; VM_NUM_THREADS],
    // Whether we need to load a new scene during the next cycle.
    requested_scene: Option<usize>,

    /// Current target of draw operations.
    render_buffer: usize,
    /// Buffer we prepare for displaying next.
    back_buffer: usize,
    /// Buffer currently on display.
    front_buffer: usize,
}

pub struct VmSys {
    resman: ResourceManager,
    palette: Vec<u8>,
    cinematic: Vec<u8>,
    video: Vec<u8>,
    strings: GameStrings,
}

struct VmCode {
    code: Vec<u8>,
}

impl VmCode {
    fn new(code: Vec<u8>) -> VmCode {
        Self { code }
    }

    // TODO return result, no unwrap
    // TODO we should update the pc when the cursor is destroyed!
    // and grab a mutable reference to the thread so no one else
    // can modify it.
    // Need our own Cursor class built from a Thread for that?
    // Or implement the read interface for a thread so we update pc directly?
    fn get_cursor(&self, pc: u64) -> Cursor<&[u8]> {
        let mut ret = Cursor::new(&self.code[..]);
        ret.seek(SeekFrom::Start(pc)).unwrap();

        ret
    }
}

pub struct Vm {
    state: VmState,
    code: VmCode,
    sys: VmSys,
    round: u64,
}

pub struct VmSnapshot {
    vm_state: VmState,
    gfx_state: Box<dyn Any>,
}

impl VmSnapshot {
    /// Create a new snapshot from the game's Vm and Renderer.
    pub fn new<T: AsRef<dyn gfx::Renderer> + ?Sized>(vm: &Vm, gfx: &T) -> Self {
        VmSnapshot {
            vm_state: vm.take_snapshot(),
            gfx_state: gfx.as_ref().take_snapshot(),
        }
    }

    /// Restore a previously captured snapshot into `vm` and `gfx`.
    pub fn restore<T: AsMut<dyn gfx::Renderer> + ?Sized>(self, vm: &mut Vm, gfx: &mut T) {
        vm.restore_snapshot(self.vm_state);
        gfx.as_mut().restore_snapshot(self.gfx_state);
    }
}

impl fmt::Debug for Vm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, r) in self.state.regs.iter().enumerate() {
            if i != 0 && i % 16 == 0 {
                writeln!(f)?;
            }
            write!(f, "{:04x?}, ", r)?;
        }
        Ok(())
    }
}

impl Vm {
    fn init_threads() -> [Thread; VM_NUM_THREADS] {
        // Really surprised that there is no better way of doing this. Since
        // Vec is not copyable, we cannot initialize the threads array using
        // the [Thread {...}; VM_NUM_THREADS] syntax. So we leave that area
        // uninitialized and iterate over it to initialize the threads.
        let mut threads: [MaybeUninit<Thread>; VM_NUM_THREADS];
        threads = unsafe { MaybeUninit::uninit().assume_init() };
        for thread in threads.iter_mut() {
            *thread = MaybeUninit::new(Thread {
                state: ThreadState::Inactive,
                requested_state: None,
                call_stack: Vec::new(),
            });
        }
        unsafe { transmute::<_, [Thread; VM_NUM_THREADS]>(threads) }
    }

    pub fn new() -> Result<Vm> {
        let resman = ResourceManager::new()?;

        Ok(Vm {
            state: VmState {
                regs: [0; VM_NUM_VARIABLES],
                threads: Vm::init_threads(),
                requested_scene: None,
                render_buffer: 0,
                back_buffer: 0,
                front_buffer: 0,
            },
            code: VmCode::new(Vec::new()),
            sys: VmSys {
                resman,
                palette: Vec::new(),
                cinematic: Vec::new(),
                video: Vec::new(),
                strings: strings::load_strings().unwrap_or_default(),
            },
            round: 0,
        })
    }

    pub fn get_reg(&self, i: usize) -> i16 {
        self.state.regs[i]
    }

    pub fn set_reg(&mut self, i: usize, v: i16) {
        self.state.regs[i] = v;
    }

    fn process_thread(&mut self, cur_thread: usize, pc: u64, gfx: &mut dyn gfx::Renderer) {
        let mut cursor = self.code.get_cursor(pc);

        loop {
            let opcode = cursor.read_u8().unwrap();

            // State op - change the current state.
            type StateOp = fn(u8, &mut Cursor<&[u8]>, &mut VmState) -> bool;
            let op: Option<StateOp> = match opcode {
                0x00 => Some(op_seti),
                0x01 => Some(op_set),
                0x02 => Some(op_add),
                0x03 => Some(op_addi),
                0x07 => Some(op_jmp),
                0x08 => Some(op_setvec),
                0x09 => Some(op_jnz),
                0x0a => Some(op_condjmp),
                0x0c => Some(op_resetthread),
                0x13 => Some(op_sub),
                0x14 => Some(op_and),
                0x15 => Some(op_or),
                0x16 => Some(op_shl),
                0x17 => Some(op_shr),
                0x18 => Some(op_playsound),
                0x1a => Some(op_playmusic),
                _ => None,
            };
            if let Some(op) = op {
                if op(opcode, &mut cursor, &mut self.state) {
                    break;
                } else {
                    continue;
                }
            }

            // Thread op - change the flow of the current thread.
            type ThreadOp = fn(&mut Thread, &mut Cursor<&[u8]>) -> bool;
            let op: Option<ThreadOp> = match opcode {
                0x04 => Some(op_jsr),
                0x05 => Some(op_return),
                0x06 => Some(op_break),
                0x11 => Some(op_killthread),
                _ => None,
            };
            if let Some(op) = op {
                if op(&mut self.state.threads[cur_thread], &mut cursor) {
                    break;
                } else {
                    continue;
                }
            }

            // Gfx op - display stuff on screen.
            type GfxOp =
                fn(u8, &mut Cursor<&[u8]>, &mut VmState, &VmSys, &mut dyn gfx::Renderer) -> bool;
            let op: Option<GfxOp> = match opcode {
                op if op & 0x80 == 0x80 => Some(op_sprs),
                op if op & 0xc0 == 0x40 => Some(op_sprl),
                0x0b => Some(op_setpalette),
                0x0d => Some(op_selectvideopage),
                0x0e => Some(op_fillvideopage),
                0x0f => Some(op_copyvideopage),
                0x10 => Some(op_blitframebuffer),
                0x12 => Some(op_drawstring),
                _ => None,
            };
            if let Some(op) = op {
                if op(opcode, &mut cursor, &mut self.state, &self.sys, gfx) {
                    break;
                } else {
                    continue;
                }
            }

            // Resource op - can do anything, really.
            if opcode == 0x19 {
                if op_loadresource(opcode, &mut cursor, &mut self.state, &mut self.sys, gfx) {
                    break;
                } else {
                    continue;
                }
            }

            panic!("Unknown opcode {:02x}!", opcode);
        }
    }

    pub fn update_input(&mut self, input: &InputState) {
        let mut mask = 0i16;

        // TODO
        // self.state.regs[0xda] = last pressed character.

        self.set_reg(
            VM_VARIABLE_HERO_POS_UPDOWN,
            match input.vertical {
                UpDownDir::Up => {
                    mask |= 0x8;
                    -1
                }
                UpDownDir::Neutral => 0,
                UpDownDir::Down => {
                    mask |= 0x4;
                    1
                }
            },
        );
        self.set_reg(
            VM_VARIABLE_HERO_POS_JUMP_DOWN,
            self.get_reg(VM_VARIABLE_HERO_POS_UPDOWN),
        );

        self.set_reg(
            VM_VARIABLE_HERO_POS_LEFT_RIGHT,
            match input.horizontal {
                LeftRightDir::Left => {
                    mask |= 0x2;
                    -1
                }
                LeftRightDir::Neutral => 0,
                LeftRightDir::Right => {
                    mask |= 0x1;
                    1
                }
            },
        );
        self.set_reg(VM_VARIABLE_HERO_POS_MASK, mask);

        self.set_reg(
            VM_VARIABLE_HERO_ACTION,
            match input.button {
                ButtonState::Released => 0,
                ButtonState::Pushed => {
                    mask |= 0x80;
                    1
                }
            },
        );
        self.set_reg(VM_VARIABLE_HERO_ACTION_POS_MASK, mask);
    }

    pub fn process_step(&mut self, gfx: &mut dyn gfx::Renderer) -> usize {
        // Check if we need to switch to a new part of the game.
        if let Some(requested_scene) = self.state.requested_scene.take() {
            info!("Loading scene {}", requested_scene);
            self.init(&scenes::SCENES[requested_scene]);
        }

        let mut actionable_threads = Vec::<(usize, u64)>::new();
        // Build the list of actionable threads for this round
        for i in 0..VM_NUM_THREADS {
            let thread = &mut self.state.threads[i];

            // First move the requested state (if any) to be current.
            if let Some(requested_state) = thread.requested_state {
                thread.state = requested_state;
                thread.requested_state = None;
            }

            if let ThreadState::Active(pc) = thread.state {
                actionable_threads.push((i, pc));
            }
        }

        let nb_threads = actionable_threads.len();
        debug!("{} threads running this round: ", nb_threads);
        debug!(
            "{}",
            actionable_threads
                .iter()
                .map(|x| format!("{:02x}", x.0))
                .collect::<Vec<String>>()
                .join(", ")
        );

        for (thread_id, pc) in actionable_threads {
            debug!("Running thread {:02x}@{:04x}", thread_id, pc);
            debug!("---------------------");

            self.process_thread(thread_id, pc, gfx);

            match self.state.threads[thread_id].state {
                ThreadState::Inactive => debug!("Thread {:02x} ended", thread_id),
                ThreadState::Active(_) => debug!("Thread {:02x} yielded", thread_id),
                ThreadState::Paused(_) => debug!("Thread {:02x} paused", thread_id),
            };
        }

        nb_threads
    }

    pub fn process(&mut self, gfx: &mut dyn gfx::Renderer) -> bool {
        debug!("===================");
        debug!("Starting round {}", self.round);
        debug!("===================");
        let nb_threads = self.process_step(gfx);
        debug!(
            "Ending round {}: {} threads have run",
            self.round, nb_threads
        );
        self.round += 1;
        nb_threads != 0
    }

    pub fn init(&mut self, scene: &scenes::Scene) {
        self.code.code = self
            .sys
            .resman
            .get_resource(scene.code)
            .unwrap()
            .data
            .clone();
        self.sys.palette = self
            .sys
            .resman
            .get_resource(scene.palette)
            .unwrap()
            .data
            .clone();
        self.sys.cinematic = self
            .sys
            .resman
            .get_resource(scene.video1)
            .unwrap()
            .data
            .clone();
        if scene.video2 != 0 {
            self.sys.video = self
                .sys
                .resman
                .get_resource(scene.video2)
                .unwrap()
                .data
                .clone();
        }
        // Random seed
        self.set_reg(VM_VARIABLE_RANDOM_SEED, 0xbeefu16 as i16);

        // Seems to be necessary for scene 2.
        self.set_reg(0xbc, 0x10);
        self.set_reg(0xf2, 0xfa0);
        self.set_reg(0xdc, 0x21);

        // TODO is this really needed?
        //self.set_reg(0x54, 0x81);
        //self.set_reg(0xe4, 0x14);

        // Reset all threads
        self.state.threads = Vm::init_threads();
        self.state.threads[0].state = ThreadState::Active(0);
    }

    pub fn get_resman(&mut self) -> &mut ResourceManager {
        &mut self.sys.resman
    }

    pub fn get_frames_to_wait(&self) -> usize {
        self.state.regs[VM_VARIABLE_PAUSE_SLICES] as usize
    }
}

impl Snapshotable for Vm {
    type State = VmState;

    fn take_snapshot(&self) -> Self::State {
        self.state.clone()
    }

    fn restore_snapshot(&mut self, state: Self::State) -> bool {
        self.state = state;
        true
    }
}
