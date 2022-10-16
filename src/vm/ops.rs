use std::convert::TryInto;

use super::*;
use crate::res;

use log::{error, warn};

pub fn op_seti(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap();
    let value = cursor.read_i16::<BE>().unwrap();

    seti(state, var_id, value);

    false
}

fn seti(state: &mut VmState, var_id: u8, value: i16) {
    state.regs[var_id as usize] = value;
}

pub fn op_set(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let dst_id = cursor.read_u8().unwrap();
    let src_id = cursor.read_u8().unwrap();

    set(state, dst_id, src_id);

    false
}

fn set(state: &mut VmState, dst_id: u8, src_id: u8) {
    state.regs[dst_id as usize] = state.regs[src_id as usize];
}

pub fn op_add(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let dst_id = cursor.read_u8().unwrap();
    let src_id = cursor.read_u8().unwrap();

    add(state, dst_id, src_id);

    false
}

fn add(state: &mut VmState, dst_id: u8, src_id: u8) {
    let src_val = state.regs[src_id as usize];
    let dst_val = &mut state.regs[dst_id as usize];
    *dst_val = dst_val.wrapping_add(src_val);
}

pub fn op_addi(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let dst_id = cursor.read_u8().unwrap();
    let value = cursor.read_i16::<BE>().unwrap();

    addi(state, dst_id, value);

    false
}

fn addi(state: &mut VmState, dst_id: u8, value: i16) {
    let dst_val = &mut state.regs[dst_id as usize];

    *dst_val = dst_val.wrapping_add(value);
}

pub fn op_jsr(thread: &mut Thread, cursor: &mut Cursor<&[u8]>) -> bool {
    let target = cursor.read_u16::<BE>().unwrap();
    let pc = cursor.position();

    jsr(thread, cursor, pc, target);

    false
}

fn jsr(thread: &mut Thread, cursor: &mut Cursor<&[u8]>, pc: u64, target: u16) {
    thread.call_stack.push(pc);
    cursor.set_position(target as u64);
}

pub fn op_return(thread: &mut Thread, cursor: &mut Cursor<&[u8]>) -> bool {
    let target = thread.call_stack.pop().unwrap();

    r#return(cursor, target);

    false
}

fn r#return(cursor: &mut Cursor<&[u8]>, target: u64) {
    cursor.set_position(target);
}

pub fn op_break(thread: &mut Thread, cursor: &mut Cursor<&[u8]>) -> bool {
    let pc = cursor.position();

    r#break(thread, pc);

    true
}

fn r#break(thread: &mut Thread, pc: u64) {
    thread.state = ThreadState::Active(pc);
    // Kind of a hack. We may have met a resetthread op that has set this
    // thread to the Paused state, with its PC of that time. But now the thread
    // has run some more and the PC is different - unless we update the PC of
    // the requested state, we will play the same state again and again.
    if let Some(ThreadState::Paused(_)) = thread.requested_state {
        thread.requested_state = Some(ThreadState::Paused(pc));
    }
}

pub fn op_jmp(_op: u8, cursor: &mut Cursor<&[u8]>, _state: &mut VmState) -> bool {
    let target = cursor.read_u16::<BE>().unwrap();

    jmp(cursor, target);

    false
}

fn jmp(cursor: &mut Cursor<&[u8]>, target: u16) {
    cursor.seek(SeekFrom::Start(target as u64)).unwrap();
}

pub fn op_setvec(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let thread_id = cursor.read_u8().unwrap();
    let target = cursor.read_u16::<BE>().unwrap();

    setvec(state, thread_id, target);

    false
}

fn setvec(state: &mut VmState, thread_id: u8, target: u16) {
    state.threads[thread_id as usize].requested_state = Some(ThreadState::Active(target as u64));
}

// Originally called "dbra"
pub fn op_jnz(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap();
    let target = cursor.read_u16::<BE>().unwrap();

    jnz(state, cursor, var_id, target);

    false
}

fn jnz(state: &mut VmState, cursor: &mut Cursor<&[u8]>, var_id: u8, target: u16) {
    let var_id = var_id as usize;

    state.regs[var_id] -= 1;
    if state.regs[var_id] != 0 {
        cursor.seek(SeekFrom::Start(target as u64)).unwrap();
    }
}

// Originally called "si" ("if" in French)
pub fn op_condjmp(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let op = cursor.read_u8().unwrap();
    let b_id = cursor.read_u8().unwrap();
    let a_id = match op {
        op if op & 0x80 != 0 => CondJmpA::Register(cursor.read_u8().unwrap()),
        op if op & 0x40 != 0 => CondJmpA::Value(cursor.read_i16::<BE>().unwrap()),
        _ => CondJmpA::Value(cursor.read_u8().unwrap() as i16),
    };

    let target = cursor.read_u16::<BE>().unwrap();

    condjmp(state, cursor, op, b_id, a_id, target);

    false
}

#[derive(Debug)]
enum CondJmpA {
    Register(u8),
    Value(i16),
}

fn condjmp(
    state: &mut VmState,
    cursor: &mut Cursor<&[u8]>,
    op: u8,
    b_id: u8,
    a_id: CondJmpA,
    target: u16,
) {
    let b = state.regs[b_id as usize];
    let a = match a_id {
        CondJmpA::Register(r) => state.regs[r as usize],
        CondJmpA::Value(v) => v,
    };

    let (cmp_op, res) = match op & 0x7 {
        0 => ("==", b == a),
        1 => ("!=", b != a),
        2 => (">", b > a),
        3 => (">=", b >= a),
        4 => ("<", b < a),
        5 => ("<=", b <= a),
        _ => panic!("undefined condjmp!"),
    };

    if res {
        cursor.seek(SeekFrom::Start(target as u64)).unwrap();
    }
}

#[derive(PartialEq, Debug)]
enum ResetThreadOp {
    Activate,
    Pause,
    Reset,
}

// Pauses/activates/resets a series of threads from a given index.
pub fn op_resetthread(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let first_thread = cursor.read_u8().unwrap();
    let last_thread = cursor.read_u8().unwrap();
    //let last_thread = (cursor.read_u8().unwrap() & 0x3f) as usize;
    // Local enum describing the valid operations on the threads.
    let op = match cursor.read_u8().unwrap() {
        0 => ResetThreadOp::Activate,
        1 => ResetThreadOp::Pause,
        2 => ResetThreadOp::Reset,
        val => panic!("impossible reset thread op 0x{:x}", val),
    };

    resetthread(state, first_thread, last_thread, op);

    false
}

fn resetthread(state: &mut VmState, first_thread: u8, last_thread: u8, op: ResetThreadOp) {
    let first_thread = first_thread as usize;
    let last_thread = last_thread as usize;

    if last_thread >= super::VM_NUM_THREADS || last_thread < first_thread {
        panic!("invalid upper thread index!");
    }

    for thread in &mut state.threads[first_thread..=last_thread] {
        let pc = match thread.state {
            // Do not activate already active threads or the PC will be invalid.
            ThreadState::Active(pc) if op != ResetThreadOp::Activate => pc,
            // TODO BUG: if a thread is active but has not run yet, and we pause
            // it here, wouldn't the PC be the one pre-run?
            ThreadState::Paused(pc) => pc,
            _ => {
                // We must switch the thread to inactive regardless of whether
                // we have a PC or not!
                if let ResetThreadOp::Reset = op {
                    thread.requested_state = Some(ThreadState::Inactive);
                }
                continue;
            }
        };
        thread.requested_state = Some(match op {
            ResetThreadOp::Activate => ThreadState::Active(pc),
            ResetThreadOp::Pause => ThreadState::Paused(pc),
            ResetThreadOp::Reset => ThreadState::Inactive,
        });
    }
}

pub fn op_and(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap();
    let val = cursor.read_i16::<BE>().unwrap();

    and(state, var_id, val);

    false
}

fn and(state: &mut VmState, var_id: u8, val: i16) {
    let dst_val = &mut state.regs[var_id as usize];

    *dst_val &= val;
}

pub fn op_or(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap();
    let val = cursor.read_i16::<BE>().unwrap();

    or(state, var_id, val);

    false
}

fn or(state: &mut VmState, var_id: u8, val: i16) {
    let dst_val = &mut state.regs[var_id as usize];

    *dst_val |= val;
}

pub fn op_shl(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap();
    let val = cursor.read_u16::<BE>().unwrap();

    shl(state, var_id, val);

    false
}

fn shl(state: &mut VmState, var_id: u8, val: u16) {
    let dst_val = &mut state.regs[var_id as usize];

    *dst_val <<= val;
}

pub fn op_shr(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap();
    let val = cursor.read_u16::<BE>().unwrap();

    shr(state, var_id, val);

    false
}

fn shr(state: &mut VmState, var_id: u8, val: u16) {
    let dst_val = &mut state.regs[var_id as usize];

    *dst_val >>= val;
}

pub fn op_killthread(thread: &mut Thread, _cursor: &mut Cursor<&[u8]>) -> bool {
    killthread(thread);

    true
}

fn killthread(thread: &mut Thread) {
    thread.state = ThreadState::Inactive;
}

pub fn op_sub(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let dst_id = cursor.read_u8().unwrap();
    let src_id = cursor.read_u8().unwrap();

    sub(state, dst_id, src_id);

    false
}

fn sub(state: &mut VmState, dst_id: u8, src_id: u8) {
    let src_val = state.regs[src_id as usize];
    let dst_val = &mut state.regs[dst_id as usize];
    *dst_val = dst_val.wrapping_sub(src_val);
}

pub fn op_setpalette<G: gfx::Gfx + ?Sized>(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    sys: &VmSys,
    _gfx: &mut G,
) -> bool {
    // Why the right shift here?
    let palette_id = cursor.read_u8().unwrap();
    // This byte is marked as unused in the technical docs.
    // Always seems to be 255.
    let _fade_speed = cursor.read_u8().unwrap();

    setpalette(state, sys, palette_id);

    false
}

fn setpalette(state: &mut VmState, sys: &VmSys, palette_id: u8) {
    let palette_id = palette_id as usize;
    let palette_data = &sys.palette[palette_id * 32..(palette_id + 1) * 32];
    state.palette.set(palette_data.try_into().unwrap());
}

/// Returns a buffer index between 0 and 3 depending on the optional
/// opcode.
fn lookup_buffer(state: &VmState, buffer_id: u8) -> usize {
    match buffer_id {
        // 0xff means the back buffer, currently being rendered.
        0xff => state.back_buffer,
        // 0xfe means the front buffer, currently being displayed.
        0xfe => state.front_buffer,
        // direct buffer is specified?
        0..=3 => buffer_id as usize,
        // 0x40, "only restore touched areas" (?)
        buffer_id if buffer_id & 0xfc == 0x40 => (buffer_id & 0x3) as usize,
        // 0x80 is used when copying with a vscroll, e.g. during earthquakes of first part.
        buffer_id if buffer_id & 0xf8 == 0x80 => (buffer_id & 0x3) as usize,
        _ => {
            error!("unmanaged buffer ID {:x}!", buffer_id);
            (buffer_id & 0x3) as usize
        }
    }
}

pub fn op_selectvideopage<G: gfx::Gfx + ?Sized>(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    _sys: &VmSys,
    _gfx: &mut G,
) -> bool {
    let page_id = cursor.read_u8().unwrap();

    selectvideopage(state, page_id);

    false
}

fn selectvideopage(state: &mut VmState, page_id: u8) {
    let resolved_page_id = lookup_buffer(state, page_id);

    state.render_buffer = resolved_page_id;
}

pub fn op_fillvideopage<G: gfx::Gfx + ?Sized>(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    _sys: &VmSys,
    gfx: &mut G,
) -> bool {
    let page_id = cursor.read_u8().unwrap();
    let color = cursor.read_u8().unwrap();

    fillvideopage(state, gfx, page_id, color);

    false
}

fn fillvideopage<G: gfx::Gfx + ?Sized>(state: &mut VmState, gfx: &mut G, page_id: u8, color: u8) {
    let resolved_page_id = lookup_buffer(state, page_id);

    gfx.fillvideopage(resolved_page_id, color);
}

pub fn op_copyvideopage<G: gfx::Gfx + ?Sized>(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    _sys: &VmSys,
    gfx: &mut G,
) -> bool {
    // TODO source buffer sometimes have bit 0x40 set. Why?
    let src_page_id = cursor.read_u8().unwrap();
    let dst_page_id = cursor.read_u8().unwrap();
    // Bit 0x80 indicates that we are interested in vscroll, only if we are
    // copying from a regular page.
    let vscroll = if src_page_id >= 0xfe || src_page_id & 0x80 == 0 {
        0
    } else {
        state.regs[VM_VARIABLE_SCROLL_Y as usize]
    };

    copyvideopage(state, gfx, src_page_id, dst_page_id, vscroll);

    false
}

fn copyvideopage<G: gfx::Gfx + ?Sized>(
    state: &mut VmState,
    gfx: &mut G,
    src_page_id: u8,
    dst_page_id: u8,
    vscroll: i16,
) {
    let resolved_src_page_id = lookup_buffer(state, src_page_id);
    let resolved_dst_page_id = lookup_buffer(state, dst_page_id);
    gfx.copyvideopage(
        resolved_src_page_id as usize,
        resolved_dst_page_id as usize,
        vscroll,
    );

    // If we have a display list in the source buffer, we need to copy its
    // content to the dst, not just reference it.
    // Or, just copy everything. It will be more accurate anyway.
}

pub fn op_blitframebuffer<G: gfx::Gfx + ?Sized>(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    _sys: &VmSys,
    gfx: &mut G,
) -> bool {
    let page_id = cursor.read_u8().unwrap();

    blitframebuffer(state, gfx, page_id);

    false
}

fn blitframebuffer<G: gfx::Gfx + ?Sized>(state: &mut VmState, gfx: &mut G, page_id: u8) {
    // Whatever we want to display is the new front buffer
    let resolved_page_id = lookup_buffer(state, page_id);

    // If we passed 0xff, the front and back buffers are swapped.
    // `resolved_page_id` will already contain the index of the back buffer, so we just need
    // to set the back buffer to the front buffer for the swap to be done.
    if page_id == 0xff {
        state.back_buffer = state.front_buffer;
    }
    state.front_buffer = resolved_page_id;

    gfx.blitframebuffer(state.front_buffer, &state.palette);

    // TODO this doesn't seem to ever be used?
    state.regs[VM_VARIABLE_SLICES_USED as usize] = 1;
}

pub fn op_drawstring<G: gfx::Gfx + ?Sized>(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    sys: &VmSys,
    gfx: &mut G,
) -> bool {
    use crate::font::CHAR_WIDTH;

    let string_id = cursor.read_u16::<BE>().unwrap();
    let x = cursor.read_u8().unwrap() as i16 * CHAR_WIDTH as i16;
    let y = cursor.read_u8().unwrap() as i16;
    let color = cursor.read_u8().unwrap();

    let string = match sys.strings.get(&(string_id as usize)) {
        None => {
            error!("cannot find string 0x{:04x}", string_id);
            return false;
        }
        Some(string) => string,
    };

    drawstring((x, y), string, color, state.render_buffer, gfx);

    false
}

fn drawstring<G: gfx::Gfx + ?Sized>(
    pos: (i16, i16),
    string: &str,
    color: u8,
    dst_page_id: usize,
    gfx: &mut G,
) {
    use crate::font::{CHAR_HEIGHT, CHAR_WIDTH};
    let (mut x, mut y) = pos;

    for c in string.chars() {
        match c {
            '\n' => {
                y += CHAR_HEIGHT as i16;
                x = pos.0;
            }
            c if c.is_ascii() => {
                gfx.draw_char(dst_page_id, (x, y), color, c as u8);
                x += CHAR_WIDTH as i16;
            }
            c => error!("invalid non-ASCII character '{}' in string {}", c, string),
        }
    }
}

const DEFAULT_ZOOM: u16 = 0x40;

pub fn op_sprs<G: gfx::Gfx + ?Sized>(
    op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    sys: &VmSys,
    gfx: &mut G,
) -> bool {
    let offset = (((((op & 0x7f) as u16) << 8) | cursor.read_u8().unwrap() as u16) * 2) as usize;
    let mut x = cursor.read_u8().unwrap() as i16;
    let mut y = cursor.read_u8().unwrap() as i16;

    if y > 199 {
        y = 199;
        x += y - 199;
    }

    draw_polygon(
        state.render_buffer,
        (x, y),
        (0, 0),
        DEFAULT_ZOOM,
        None,
        &sys.cinematic,
        offset,
        gfx,
    );

    false
}

pub fn op_sprl<G: gfx::Gfx + ?Sized>(
    op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    sys: &VmSys,
    gfx: &mut G,
) -> bool {
    let offset = (cursor.read_u16::<BE>().unwrap() * 2) as usize;
    let x = match op & 0x30 {
        0x00 => cursor.read_i16::<BE>().unwrap(),
        0x10 => state.regs[cursor.read_u8().unwrap() as usize],
        0x30 => cursor.read_u8().unwrap() as i16 + 0x100,
        _ => cursor.read_u8().unwrap() as i16,
    };
    let y = match op & 0xc {
        0x00 => cursor.read_i16::<BE>().unwrap(),
        0x04 => state.regs[cursor.read_u8().unwrap() as usize],
        _ => cursor.read_u8().unwrap() as i16,
    };
    let (zoom, segment) = match op & 0x3 {
        0x0 => (DEFAULT_ZOOM, sys.cinematic.deref()),
        0x1 => (
            state.regs[cursor.read_u8().unwrap() as usize] as u16,
            sys.cinematic.deref(),
        ),
        0x2 => (cursor.read_u8().unwrap() as u16, sys.cinematic.deref()),
        0x3 => (DEFAULT_ZOOM, sys.video.deref()),
        _ => panic!("invalid zoom factor!"),
    };

    draw_polygon(
        state.render_buffer,
        (x, y),
        (0, 0),
        zoom,
        None,
        segment,
        offset,
        gfx,
    );

    false
}

#[allow(clippy::too_many_arguments)]
fn draw_polygon<G: gfx::Gfx + ?Sized>(
    render_buffer: usize,
    pos: (i16, i16),
    offset: (i16, i16),
    zoom: u16,
    color: Option<u8>,
    segment: &[u8],
    start_offset: usize,
    gfx: &mut G,
) {
    let mut cursor = Cursor::new(segment);
    match cursor.seek(SeekFrom::Start(start_offset as u64)) {
        Ok(_) => (),
        Err(e) => {
            error!("error while seeking to draw polygon: {}", e);
            return;
        }
    }

    let op = cursor.read_u8().unwrap();
    match op {
        op if op & 0xc0 == 0xc0 => {
            // TODO match other properties of the color (e.g. blend) from op
            let color = match color {
                // If we already have a color set, use it.
                Some(color) => color,
                // Otherwise take the color from the op.
                None => op & 0x3f,
            };

            let bb = (cursor.read_u8().unwrap(), cursor.read_u8().unwrap());
            let nb_points = cursor.read_u8().unwrap() as usize;
            let points_start = cursor.position() as usize;
            let points = unsafe {
                std::slice::from_raw_parts(
                    segment[points_start..points_start + (nb_points * 2)].as_ptr()
                        as *const gfx::Point<u8>,
                    nb_points,
                )
            };
            gfx.fillpolygon(render_buffer, pos, offset, color, zoom, bb, points);
        }
        op if op == 0x02 => {
            draw_polygon_hierarchy(
                render_buffer,
                pos,
                offset,
                zoom,
                color,
                cursor,
                segment,
                gfx,
            );
        }
        _ => panic!("invalid draw_polygon op 0x{:x}", op),
    };
}

#[allow(clippy::too_many_arguments)]
fn draw_polygon_hierarchy<G: gfx::Gfx + ?Sized>(
    render_buffer: usize,
    pos: (i16, i16),
    offset: (i16, i16),
    zoom: u16,
    color: Option<u8>,
    mut cursor: Cursor<&[u8]>,
    segment: &[u8],
    gfx: &mut G,
) {
    let offset = (
        offset.0 - cursor.read_u8().unwrap() as i16,
        offset.1 - cursor.read_u8().unwrap() as i16,
    );
    let nb_childs = cursor.read_u8().unwrap() + 1;

    for _i in 0..nb_childs {
        let word = cursor.read_u16::<BE>().unwrap();
        let (read_color, poly_offset) = (word & 0x8000 != 0, word & 0x7fff);
        let offset = (
            offset.0 + cursor.read_u8().unwrap() as i16,
            offset.1 + cursor.read_u8().unwrap() as i16,
        );

        let color = if read_color {
            let color = Some(cursor.read_u8().unwrap() & 0x7f);
            // This is a "mask number" apparently?
            cursor.read_u8().unwrap();
            color
        } else {
            color
        };

        draw_polygon(
            render_buffer,
            pos,
            offset,
            zoom,
            color,
            segment,
            poly_offset as usize * 2,
            gfx,
        );
    }
}

pub fn op_playsound<A: audio::Mixer + ?Sized>(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    _state: &mut VmState,
    _sys: &VmSys,
    audio: &mut A,
) -> bool {
    let res_id = cursor.read_u16::<BE>().unwrap() as u8;
    let freq_index = cursor.read_u8().unwrap();
    let volume = std::cmp::min(cursor.read_u8().unwrap(), 0x3f);
    let channel = cursor.read_u8().unwrap();

    playsound(audio, res_id, channel, freq_index, volume);

    false
}

fn playsound<A: audio::Mixer + ?Sized>(
    audio: &mut A,
    res_id: u8,
    channel: u8,
    freq_index: u8,
    volume: u8,
) {
    match audio::PLAYBACK_FREQUENCY.get(freq_index as usize) {
        None => error!("invalid frequency index {}", freq_index),
        Some(&freq) => audio.play(res_id, channel, freq, volume),
    };
}

fn delay_to_tempo(delay: u16) -> usize {
    delay as usize * 60 / 7050
}

pub fn op_playmusic<A: audio::Mixer + audio::MusicPlayer + ?Sized>(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    _state: &mut VmState,
    sys: &VmSys,
    audio: &mut A,
) -> bool {
    let res_id = cursor.read_u16::<BE>().unwrap();
    let delay = cursor.read_u16::<BE>().unwrap();
    let pos = cursor.read_u8().unwrap();

    playmusic(res_id, delay, pos, sys, audio);

    false
}

fn playmusic<A: audio::Mixer + audio::MusicPlayer + ?Sized>(
    res_id: u16,
    delay: u16,
    pos: u8,
    sys: &VmSys,
    audio: &mut A,
) {
    match (res_id, delay) {
        // Stop the player.
        (0, 0) => audio.stop_music(),
        // Update the playback speed.
        (0, new_delay) => {
            let new_tempo = delay_to_tempo(new_delay);
            audio.update_tempo(new_tempo);
        }
        // Load new music module and start playback.
        (res_id, delay) => match sys
            .resman
            // TODO mmm we are probably preloading the music, right? In that case this should just
            // retrieve it, or probably a Rc to it...
            .load_resource(res_id as usize)
            .ok()
            .and_then(|r| r.into_music())
        {
            None => {
                error!("failed to obtain music resource 0x{:02x}", res_id);
            }
            Some(music) => {
                // Take the default delay of the music if none is specified.
                let delay = if delay == 0 {
                    music.header.delay
                } else {
                    delay
                };
                let tempo = delay_to_tempo(delay);
                audio.play_music(music, tempo, pos as u16)
            }
        },
    };
}

/// Asks the resource manager to load a resource from disk.
///
/// This is apparently used to trigger the loading of sounds and musics at the beginning of a scene.
/// Other resources like palettes, bytecode and video segments are specified from the scenes list
/// and loaded with it.
//
/// This opcode made sense on systems with very little memory. On modern hardware we can just keep
/// everything in memory, however we can always ping the resource manager and let it decide how
/// assets should be managed.
//
/// This opcode is also used to switch between scenes. In such cases, |res_id|
/// will be 0x3e8x, where x is the number of the scene to load.
pub fn op_loadresource<G: gfx::Gfx + ?Sized, A: audio::Mixer + ?Sized>(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    sys: &mut VmSys,
    gfx: &mut G,
    audio: &mut A,
) -> bool {
    let res_id = cursor.read_u16::<BE>().unwrap();

    loadresource(res_id, state, sys, gfx, audio);

    false
}

fn loadresource<G: gfx::Gfx + ?Sized, A: audio::Mixer + ?Sized>(
    res_id: u16,
    state: &mut VmState,
    sys: &mut VmSys,
    gfx: &mut G,
    audio: &mut A,
) {
    use res::ResType;

    let res_id = res_id as usize;

    // In the original game, this meant "free all memory". Since we don't have
    // to manage memory ourselves, we don't need to do that - just stopping
    // any activity is enough.
    if res_id == 0 {
        // TODO just stop sound and music?
        warn!("op_loadresource(0) - not yet implemented!");
        return;
    }

    // Switch to a new scene.
    const LOAD_SCENE_OFFSET: usize = 0x3e80;
    if res_id >= LOAD_SCENE_OFFSET {
        state.requested_scene = Some(res_id - LOAD_SCENE_OFFSET);
        return;
    }

    // Just load a resource.
    let res = match sys.resman.load_resource(res_id) {
        Ok(res) => res,
        Err(e) => {
            error!("error while loading resource: {:#}", e);
            return;
        }
    };

    match res.res_type {
        // Load sounds into our mixer so they can be played back later.
        ResType::Sound => {
            let sample = match res.into_sound() {
                Some(sample) => sample,
                None => {
                    error!(
                        "failed to convert resource {:02x} into a sound sample",
                        res_id
                    );
                    return;
                }
            };
            audio.add_sample(res_id as u8, sample);
        }
        // Bitmap resources are always loaded into buffer 0. Emulate this
        // behavior.
        ResType::Bitmap => gfx.blit_buffer(0, &res.data),
        _ => (),
    }
}
