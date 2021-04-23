use std::convert::TryInto;

use super::*;
use crate::gfx::{polygon::Polygon, Point};
use crate::res;

use log::{trace, warn};

pub fn op_seti(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap() as usize;
    let value = cursor.read_i16::<BE>().unwrap();

    trace!("op_seti [{:02x}] <- {:04x}", var_id, value);

    state.regs[var_id] = value;
    false
}

pub fn op_set(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let dst_id = cursor.read_u8().unwrap() as usize;
    let src_id = cursor.read_u8().unwrap() as usize;

    trace!("op_set[{:02x}] <- [{:02x}]", dst_id, src_id);

    state.regs[dst_id] = state.regs[src_id];
    false
}

pub fn op_add(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let dst_id = cursor.read_u8().unwrap() as usize;
    let src_id = cursor.read_u8().unwrap() as usize;

    trace!(
        "op_add[{:02x}]({:02x}) <+- [{:02x}]({:02x})",
        dst_id,
        state.regs[dst_id],
        src_id,
        state.regs[src_id]
    );

    state.regs[dst_id] = state.regs[dst_id].wrapping_add(state.regs[src_id]);
    false
}

pub fn op_addi(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap() as usize;
    let value = cursor.read_i16::<BE>().unwrap();

    trace!(
        "op_addconst [{:02x}]({:02x}) <+= {:04x}",
        var_id,
        state.regs[var_id],
        value
    );
    state.regs[var_id] = state.regs[var_id].wrapping_add(value);
    false
}

pub fn op_jsr(thread: &mut Thread, cursor: &mut Cursor<&[u8]>) -> bool {
    let offset = cursor.read_u16::<BE>().unwrap();
    trace!("op_jsr, offset 0x{:04x}", offset);

    thread.call_stack.push(cursor.position());
    cursor.set_position(offset as u64);
    false
}

pub fn op_return(thread: &mut Thread, cursor: &mut Cursor<&[u8]>) -> bool {
    let ret_addr = thread.call_stack.pop().unwrap();
    trace!("op_return <- {:04x}", ret_addr);

    cursor.set_position(ret_addr);
    false
}

pub fn op_break(thread: &mut Thread, cursor: &mut Cursor<&[u8]>) -> bool {
    trace!("op_break");

    thread.state = ThreadState::Active(cursor.position());
    // Kind of a hack. We may have met a resetthread op that has set this
    // thread to the Paused state, with its PC of that time. But now the thread
    // has run some more and the PC is different - unless we update the PC of
    // the requested state, we will play the same state again and again.
    if let Some(ThreadState::Paused(_)) = thread.requested_state {
        thread.requested_state = Some(ThreadState::Paused(cursor.position()));
    }
    true
}

pub fn op_jmp(_op: u8, cursor: &mut Cursor<&[u8]>, _state: &mut VmState) -> bool {
    let dst = cursor.read_u16::<BE>().unwrap() as u64;
    trace!("op_jmp {:x}", dst);

    cursor.seek(SeekFrom::Start(dst)).unwrap();
    false
}

pub fn op_setvec(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let thread_id = cursor.read_u8().unwrap() as usize;
    let pc_offset = cursor.read_u16::<BE>().unwrap();
    trace!("op_setvec |{:02x}| <- {:04x}", thread_id, pc_offset);

    state.threads[thread_id].requested_state = Some(ThreadState::Active(pc_offset as u64));

    false
}

// Originally called "dbra"
pub fn op_jnz(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap() as usize;
    let dst = cursor.read_u16::<BE>().unwrap() as u64;

    state.regs[var_id] -= 1;
    trace!("op_jnz [{:02x}] {}", var_id, state.regs[var_id] != 0);
    if state.regs[var_id] != 0 {
        cursor.seek(SeekFrom::Start(dst)).unwrap();
    }

    false
}

// Originally called "si" ("if" in French)
pub fn op_condjmp(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let op = cursor.read_u8().unwrap();
    let reg = cursor.read_u8().unwrap() as usize;
    let b = state.regs[reg];
    let a = match op {
        op if op & 0x80 != 0 => state.regs[cursor.read_u8().unwrap() as usize],
        op if op & 0x40 != 0 => cursor.read_i16::<BE>().unwrap(),
        _ => cursor.read_u8().unwrap() as i16,
    };

    let dst = cursor.read_u16::<BE>().unwrap() as u64;

    let expr_str;
    let expr = match op & 0x7 {
        0 => {
            expr_str = "==";
            b == a
        }
        1 => {
            expr_str = "!=";
            b != a
        }
        2 => {
            expr_str = ">";
            b > a
        }
        3 => {
            expr_str = ">=";
            b >= a
        }
        4 => {
            expr_str = "<";
            b < a
        }
        5 => {
            expr_str = "<=";
            b <= a
        }
        _ => panic!("undefined condjmp!"),
    };

    trace!(
        "op_condjmp {:02x} if {:x}({:x}) {} {:x} -> {:x} {}",
        op,
        b,
        reg,
        expr_str,
        a,
        dst,
        expr
    );

    if expr {
        cursor.seek(SeekFrom::Start(dst)).unwrap();
    }

    false
}

// Pauses/activates/resets a series of threads from a given index.
pub fn op_resetthread(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let first_thread = cursor.read_u8().unwrap() as usize;
    let last_thread = cursor.read_u8().unwrap() as usize;
    //let last_thread = (cursor.read_u8().unwrap() & 0x3f) as usize;
    // Local enum describing the valid operations on the threads.
    #[derive(PartialEq, Debug)]
    enum ResetThreadOp {
        Activate,
        Pause,
        Reset,
    }
    let op = match cursor.read_u8().unwrap() {
        0 => ResetThreadOp::Activate,
        1 => ResetThreadOp::Pause,
        2 => ResetThreadOp::Reset,
        val => panic!("impossible reset thread op 0x{:x}", val),
    };

    trace!(
        "op_resetthread {:?} [{}..{}]",
        op,
        first_thread,
        last_thread
    );

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
            _ => continue,
        };
        thread.requested_state = Some(match op {
            ResetThreadOp::Activate => ThreadState::Active(pc),
            ResetThreadOp::Pause => ThreadState::Paused(pc),
            ResetThreadOp::Reset => ThreadState::Inactive,
        });
    }

    false
}

pub fn op_and(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap() as usize;
    let val = cursor.read_i16::<BE>().unwrap();

    state.regs[var_id] &= val;
    trace!("op_and [{:02x}] &= 0x{:x}", var_id, val);

    false
}

pub fn op_or(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap() as usize;
    let val = cursor.read_i16::<BE>().unwrap();

    state.regs[var_id] |= val;
    trace!("op_or [{:02x}] |= 0x{:x}", var_id, val);

    false
}

pub fn op_shl(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap() as usize;
    let val = cursor.read_u16::<BE>().unwrap();

    state.regs[var_id] <<= val;
    trace!("op_shl [{:02x}] <<= 0x{:x}", var_id, val);

    false
}

pub fn op_shr(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let var_id = cursor.read_u8().unwrap() as usize;
    let val = cursor.read_u16::<BE>().unwrap();

    state.regs[var_id] >>= val;
    trace!("op_shr [{:02x}] >>= 0x{:x}", var_id, val);

    false
}

pub fn op_killthread(thread: &mut Thread, _cursor: &mut Cursor<&[u8]>) -> bool {
    trace!("op_killthread");

    thread.state = ThreadState::Inactive;

    true
}

pub fn op_sub(_op: u8, cursor: &mut Cursor<&[u8]>, state: &mut VmState) -> bool {
    let dst_id = cursor.read_u8().unwrap() as usize;
    let src_id = cursor.read_u8().unwrap() as usize;

    trace!(
        "op_sub[{:02x}]({:02x}) <+- [{:02x}]({:02x})",
        dst_id,
        state.regs[dst_id],
        src_id,
        state.regs[src_id]
    );

    state.regs[dst_id] = state.regs[dst_id].wrapping_sub(state.regs[src_id]);
    false
}

pub fn op_setpalette(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    _state: &mut VmState,
    sys: &VmSys,
    gfx: &mut dyn gfx::Backend,
) -> bool {
    // Why the right shift here?
    let palette_id = cursor.read_u8().unwrap() as usize;
    // This byte is marked as unused in the technical docs.
    // Always seems to be 255.
    let fade_speed = cursor.read_u8().unwrap();
    let palette_data = &sys.palette[palette_id * 32..(palette_id + 1) * 32];

    trace!("op_setpalette {:x}@{}", palette_id, fade_speed);

    gfx.set_palette(palette_data.try_into().unwrap());

    false
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
        // 0xc0, "just update video address for this frame" (?)
        buffer_id if buffer_id & 0xfc == 0xc0 => (buffer_id & 0x3) as usize,
        _ => {
            warn!("unmanaged buffer ID {:x}!", buffer_id);
            buffer_id as usize & 0x3
        }
    }
}

pub fn op_selectvideopage(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    _sys: &VmSys,
    _gfx: &mut dyn gfx::Backend,
) -> bool {
    let buffer_id = cursor.read_u8().unwrap();
    let buffer_id_resolved = lookup_buffer(state, buffer_id);
    trace!(
        "op_selectvideopage {:x} ({:x})",
        buffer_id,
        buffer_id_resolved
    );

    state.render_buffer = buffer_id_resolved;

    false
}

pub fn op_fillvideopage(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    _sys: &VmSys,
    gfx: &mut dyn gfx::Backend,
) -> bool {
    let page_id = cursor.read_u8().unwrap();
    let color = cursor.read_u8().unwrap();
    trace!("op_fillvideopage {:x} <- {:02x}", page_id, color);

    gfx.fillvideopage(lookup_buffer(state, page_id), color);

    false
}

pub fn op_copyvideopage(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    _sys: &VmSys,
    gfx: &mut dyn gfx::Backend,
) -> bool {
    // TODO source buffer sometimes have bit 0x40 set. Why?
    let src_page_id = cursor.read_u8().unwrap();
    let dst_page_id = cursor.read_u8().unwrap();
    let src_page_id_resolved = lookup_buffer(state, src_page_id);
    let dst_page_id_resolved = lookup_buffer(state, dst_page_id);
    let vscroll = if dst_page_id >= 0xfe {
        0
    } else {
        state.regs[VM_VARIABLE_SCROLL_Y]
    };
    trace!(
        "op_copyvideopage {:x} ({:x}) -> {:x} ({:x}) @{}",
        src_page_id,
        src_page_id_resolved,
        dst_page_id,
        dst_page_id_resolved,
        vscroll
    );

    gfx.copyvideopage(src_page_id_resolved, dst_page_id_resolved, vscroll);

    // If we have a display list in the source buffer, we need to copy its
    // content to the dst, not just reference it.
    // Or, just copy everything. It will be more accurate anyway.

    false
}

pub fn op_blitframebuffer(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    _sys: &VmSys,
    gfx: &mut dyn gfx::Backend,
) -> bool {
    let page_id = cursor.read_u8().unwrap();
    trace!("op_blitframebuffer {:x}", page_id);

    // Whatever we want to display is the new front buffer
    let new_front = lookup_buffer(state, page_id);

    // If we passed 0xff, the front and back buffers are swapped.
    if page_id == 0xff {
        let tmp = state.back_buffer;
        state.back_buffer = state.front_buffer;
        state.front_buffer = tmp;
    }

    state.front_buffer = new_front;
    gfx.blitframebuffer(state.front_buffer);

    // Assume that we render very fast, which should be the case.
    state.regs[VM_VARIABLE_SLICES_USED] = 1;

    false
}

pub fn op_drawstring(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    _sys: &VmSys,
    _gfx: &mut dyn gfx::Backend,
) -> bool {
    let string_id = cursor.read_u16::<BE>().unwrap();
    let x = cursor.read_u8().unwrap();
    let y = cursor.read_u8().unwrap();
    let color = cursor.read_u8().unwrap();

    warn!(
        "op_drawstring 0x{:04x}, ({}, {}) 0x{:x} -> {} - not yet implemented",
        string_id, x, y, color, state.render_buffer
    );

    false
}

const DEFAULT_ZOOM: u16 = 0x40;

pub fn op_sprs(
    op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    sys: &VmSys,
    gfx: &mut dyn gfx::Backend,
) -> bool {
    let offset = (((((op & 0x7f) as u16) << 8) | cursor.read_u8().unwrap() as u16) * 2) as usize;
    let mut x = cursor.read_u8().unwrap() as u16;
    let mut y = cursor.read_u8().unwrap() as u16;

    if y > 199 {
        y = 199;
        x += y - 199;
    }

    trace!("op_sprs@0x{:x} {:?} | {}", offset, (x, y), DEFAULT_ZOOM);

    let mut data_cursor = Cursor::new(&sys.cinematic[..]);
    data_cursor.seek(SeekFrom::Start(offset as u64)).unwrap();
    draw_polygon(
        state.render_buffer,
        (x as i16, y as i16),
        DEFAULT_ZOOM,
        None,
        data_cursor,
        &sys.cinematic,
        gfx,
    );

    false
}

pub fn op_sprl(
    op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    sys: &VmSys,
    gfx: &mut dyn gfx::Backend,
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

    trace!("op_sprl@0x{:x} {:?} | {}", offset, (x, y), zoom);

    let mut data_cursor = Cursor::new(segment);
    data_cursor.seek(SeekFrom::Start(offset as u64)).unwrap();
    draw_polygon(
        state.render_buffer,
        (x, y),
        zoom,
        None,
        data_cursor,
        segment,
        gfx,
    );

    false
}

fn coord_zoom(x: u16, y: u16, zoom: u16) -> (u16, u16) {
    (
        (x as u32 * zoom as u32 / 64) as u16,
        (y as u32 * zoom as u32 / 64) as u16,
    )
}

fn read_polygon(mut data_cursor: Cursor<&[u8]>) -> Polygon {
    let bbw = data_cursor.read_u8().unwrap() as u16;
    let bbh = data_cursor.read_u8().unwrap() as u16;
    let nb_vertices = data_cursor.read_u8().unwrap();

    let mut points = Vec::new();
    for _i in 0..nb_vertices {
        let (x, y) = (
            data_cursor.read_u8().unwrap() as u16,
            data_cursor.read_u8().unwrap() as u16,
        );
        points.push(Point { x, y });
    }

    Polygon::new((bbw, bbh), points)
}

#[allow(clippy::too_many_arguments)]
fn draw_polygon(
    render_buffer: usize,
    pos: (i16, i16),
    zoom: u16,
    color: Option<u8>,
    mut data_cursor: Cursor<&[u8]>,
    segment: &[u8],
    gfx: &mut dyn gfx::Backend,
) {
    let op = data_cursor.read_u8().unwrap();

    match op {
        op if op & 0xc0 == 0xc0 => {
            // TODO match other properties of the color (e.g. blend) from op
            let color = match color {
                // If we already have a color set, use it.
                Some(color) => color,
                // Otherwise take the color from the op.
                None => op & 0x3f,
            };

            let polygon = read_polygon(data_cursor);
            gfx.fillpolygon(render_buffer, pos, color, zoom, &polygon);
        }
        op if op == 0x02 => {
            draw_polygon_hierarchy(render_buffer, pos, zoom, color, data_cursor, segment, gfx);
        }
        _ => panic!("invalid draw_polygon op 0x{:x}", op),
    };
}

#[allow(clippy::too_many_arguments)]
fn draw_polygon_hierarchy(
    render_buffer: usize,
    pos: (i16, i16),
    zoom: u16,
    color: Option<u8>,
    mut data_cursor: Cursor<&[u8]>,
    segment: &[u8],
    gfx: &mut dyn gfx::Backend,
) {
    // TODO not great - we are forcing this zoom operation to be done on integer
    // operands, which results in quite some precision loss. It would be better to
    // pass it down to the gfx backend which could use the best suited type.
    let p = coord_zoom(
        data_cursor.read_u8().unwrap() as u16,
        data_cursor.read_u8().unwrap() as u16,
        zoom,
    );
    let x = pos.0 - p.0 as i16;
    let y = pos.1 - p.1 as i16;
    let nb_childs = data_cursor.read_u8().unwrap() + 1;

    trace!(
        "draw_polygon_hierarchy ({}, {})x{}, {} childs",
        x,
        y,
        zoom,
        nb_childs
    );

    for _i in 0..nb_childs {
        let (read_color, offset) = match data_cursor.read_u16::<BE>().unwrap() {
            word if word & 0x8000 != 0 => (true, word & 0x7fff),
            word => (false, word),
        };
        let p = coord_zoom(
            data_cursor.read_u8().unwrap() as u16,
            data_cursor.read_u8().unwrap() as u16,
            zoom,
        );
        let px = x + p.0 as i16;
        let py = y + p.1 as i16;

        let color = if read_color {
            Some(data_cursor.read_u8().unwrap() & 0x7f)
        } else {
            color
        };

        // TODO: We have a dead byte after the color?
        // Nope, this is a "mask number" apparently
        if read_color {
            data_cursor.read_u8().unwrap();
        }

        trace!(
            "  child at ({}, {}), offset 0x{:x} color {:?}",
            px,
            py,
            offset,
            color
        );

        let mut new_cursor = Cursor::new(segment);
        new_cursor.seek(SeekFrom::Start(offset as u64 * 2)).unwrap();
        draw_polygon(
            render_buffer,
            (px, py),
            zoom,
            color,
            new_cursor,
            segment,
            gfx,
        );
    }
}

pub fn op_playsound(_op: u8, cursor: &mut Cursor<&[u8]>, _state: &mut VmState) -> bool {
    let res_id = cursor.read_u16::<BE>().unwrap();
    let freq = cursor.read_u8().unwrap();
    let vol = cursor.read_u8().unwrap();
    let channel = cursor.read_u8().unwrap();

    warn!(
        "op_playsound: {} {} {} {} - not yet implemented",
        res_id, freq, vol, channel
    );

    false
}
pub fn op_playmusic(_op: u8, cursor: &mut Cursor<&[u8]>, _state: &mut VmState) -> bool {
    let res_id = cursor.read_u16::<BE>().unwrap();
    let delay = cursor.read_u16::<BE>().unwrap();
    let pos = cursor.read_u8().unwrap();

    warn!(
        "op_playmusic: {} {} {} - not yet implemented",
        res_id, delay, pos
    );

    false
}

// Asks the resource manager to load a resource from disk.
// This is apparently used to trigger the loading of sounds and musics as
// they become needed. Other resources like palettes, bytecode and video
// segments are specified from the scenes list.
//
// This opcode made sense on systems with very little memory. On modern
// hardware we can just keep everything in memory, however we can always ping
// the resource manager and let it decide how assets should be managed.
//
// This opcode is also used to switch between scenes. In such cases, |res_id|
// will be 0x3e8x, where x is the number of the scene to load.
pub fn op_loadresource(
    _op: u8,
    cursor: &mut Cursor<&[u8]>,
    state: &mut VmState,
    sys: &mut VmSys,
    gfx: &mut dyn gfx::Backend,
) -> bool {
    let res_id = cursor.read_u16::<BE>().unwrap() as usize;

    // In the original game, this meant "free all memory". Since we don't have
    // to manage memory ourselves, we don't need to do that - just stopping
    // any activity is enough.
    if res_id == 0 {
        // TODO just stop sound and music?
        warn!("op_loadresource(0) - not yet implemented!");
        return false;
    }

    // Switch to a new scene.
    const LOAD_SCENE_OFFSET: usize = 16000;
    if res_id >= LOAD_SCENE_OFFSET {
        state.requested_scene = Some(res_id - LOAD_SCENE_OFFSET);
        return false;
    }

    // Just load a resource.
    let res = sys.resman.get_resource(res_id).unwrap();
    match res.res_type {
        // Bitmap resources are always loaded into buffer 0. Emulate this
        // behavior.
        res::ResType::Bitmap => gfx.blit_buffer(0, &res.data),
        res_type => warn!("op_loadresource not implemented for type {}", res_type),
    };

    false
}
