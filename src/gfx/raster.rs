use std::{
    cell::{Ref, RefCell},
    cmp::{max, min},
};

use anyhow::{anyhow, Result};

use crate::{
    gfx::{IndexedRenderer, Point, SCREEN_RESOLUTION},
    sys::Snapshotable,
};

/// Apply the zoom function on a point's coordinate `p`: multiply it by `zoom`,
/// then divide by 64.
fn scale(p: i16, zoom: u16) -> i16 {
    ((p as i32 * zoom as i32) / 64) as i16
}

/// Compute the slope between `p1` and `p2`, i.e. the amount of sub-pixels one must move
/// horizontally on each y step in order to draw a connected line between the points.
fn slope_step(p1: &Point<i32>, p2: &Point<i32>) -> i32 {
    let dy = p2.y - p1.y;
    let dx = p2.x - p1.x;

    if dy != 0 {
        dx / dy
    } else {
        0
    }
}

/// Iterator over all the lines of a trapezoid defined by its four points.
///
/// The returned item is ((left_x, right_x), y).
fn trapezoid_line_iterator(
    p1: Point<i32>,
    p2: Point<i32>,
    next_p1: Point<i32>,
    next_p2: Point<i32>,
) -> impl Iterator<Item = ((i16, i16), i16)> {
    // Vertical range of the quad.
    let v_range = max(p1.y, p2.y) as i16..min(next_p1.y, next_p2.y) as i16;
    let slope1 = slope_step(&p1, &next_p1);
    let slope2 = slope_step(&p2, &next_p2);

    v_range.scan((p1.x, p2.x), move |(x1, x2), y| {
        // Center the leftmost pixel and scale back.
        let start_x = ((min(*x1, *x2) + 0x8000) >> 16) as i16;
        // Include the rightmost pixel in the line, center it, and scale back.
        let end_x = ((max(*x1, *x2) + 0x18000) >> 16) as i16;
        *x1 += slope1;
        *x2 += slope2;
        Some(((start_x, end_x), y))
    })
}

#[derive(Clone)]
pub struct IndexedImage([u8; SCREEN_RESOLUTION[0] * SCREEN_RESOLUTION[1]]);

impl Default for IndexedImage {
    fn default() -> Self {
        IndexedImage([0u8; SCREEN_RESOLUTION[0] * SCREEN_RESOLUTION[1]])
    }
}

impl IndexedImage {
    pub fn set_content(&mut self, buffer: &[u8]) -> Result<()> {
        const EXPECTED_LENGTH: usize = SCREEN_RESOLUTION[0] * SCREEN_RESOLUTION[1] / 2;
        if buffer.len() != EXPECTED_LENGTH {
            return Err(anyhow!(
                "Invalid buffer length {}, expected {}",
                buffer.len(),
                EXPECTED_LENGTH
            ));
        }

        let planes: Vec<&[u8]> = buffer.chunks(8000).collect();

        for (i, pixel) in self.0.iter_mut().enumerate() {
            let idx = i / 8;
            let bit = 7 - (i % 8);
            *pixel = (planes[0][idx] >> bit) & 0b1
                | ((planes[1][idx] >> bit) & 0b1) << 1
                | ((planes[2][idx] >> bit) & 0b1) << 2
                | ((planes[3][idx] >> bit) & 0b1) << 3;
        }

        Ok(())
    }

    fn offset(x: i16, y: i16) -> Result<usize, ()> {
        if x < 0 || x >= SCREEN_RESOLUTION[0] as i16 || y < 0 || y >= SCREEN_RESOLUTION[1] as i16 {
            Err(())
        } else {
            Ok((y as usize * SCREEN_RESOLUTION[0]) + x as usize)
        }
    }

    #[allow(dead_code)]
    fn set_pixel(&mut self, x: i16, y: i16, color: u8) {
        if color > 0xf {
            return;
        }

        if let Ok(offset) = IndexedImage::offset(x, y) {
            self.0[offset] = color;
        }
    }

    #[allow(dead_code)]
    fn get_pixel(&mut self, x: i16, y: i16) -> Result<u8, ()> {
        match IndexedImage::offset(x, y) {
            Ok(offset) => Ok(self.0[offset]),
            Err(_) => Err(()),
        }
    }

    /// Draw a horizontal line at ordinate y, between x1 (included) and x2 (excluded)
    /// If x1 >= x2, nothing is drawn.
    fn draw_hline<F>(&mut self, y: i16, x1: i16, x2: i16, draw_func: F)
    where
        F: Fn(&mut [u8], usize),
    {
        let line_offset = match IndexedImage::offset(0, y) {
            Ok(offset) => offset,
            // Line is not on screen.
            Err(_) => return,
        };

        // Limit x_start and x_stop to [0..SCREEN_RESOLUTION[0]].
        let x_start = min(max(x1, 0) as usize, SCREEN_RESOLUTION[0]);
        let x_stop = min(max(x2, 0) as usize, SCREEN_RESOLUTION[0]);

        let slice = &mut self.0[line_offset + x_start..line_offset + x_stop];
        draw_func(slice, line_offset + x_start);
    }

    fn fill_polygon<F>(
        &mut self,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
        bb: (u8, u8),
        points: &[Point<u8>],
        draw_func: F,
    ) where
        F: Fn(&mut [u8], usize),
    {
        assert!(points.len() >= 4);
        assert!(points.len() % 2 == 0);

        // Optimization for single-pixel polygons
        if bb.0 == 0 && bb.1 == 0 {
            if let Ok(offset) = IndexedImage::offset(pos.0, pos.1) {
                draw_func(&mut self.0[offset..offset + 1], offset);
            }
            return;
        }

        // Offset x and y by the polygon center.
        let bbox_offset = (scale(bb.0 as i16, zoom) / 2, scale(bb.1 as i16, zoom) / 2);
        let x = pos.0 - bbox_offset.0;
        let y = pos.1 - bbox_offset.1;

        // The first and last points are always at the top. We will fill
        // the polygon line by line starting from them, and stop when the front
        // and back join at the bottom of the polygon.
        let mut points = points
            .iter()
            // Add the x and y offsets.
            .map(|p| {
                // It is tempting to simplify this into
                // "scale(p.x + offset.0, zoom) + x" but this results in some
                // objects appearing larger than they should. The game does the
                // scaling separately on these two parameters, so we need to do
                // the same in order to obtain the same rendering.
                Point::new(
                    scale(p.x as i16, zoom) + scale(offset.0, zoom) + x,
                    scale(p.y as i16, zoom) + scale(offset.1, zoom) + y,
                )
            })
            // Turn the point into i32 and add 16 bits of fixed decimals to x to
            // add some precision when computing the slope.
            .map(|p| Point::<i32>::new((p.x as i32) << 16, p.y as i32));
        // We have at least 4 points in the polygon, so these unwraps() are safe.
        let mut p1 = points.next().unwrap();
        let mut p2 = points.next_back().unwrap();
        let mut next_p1 = points.next().unwrap();
        let mut next_p2 = points.next_back().unwrap();

        // Loop over all the lines of the polygon.
        loop {
            for ((x_start, x_end), y) in trapezoid_line_iterator(p1, p2, next_p1, next_p2) {
                self.draw_hline(y, x_start, x_end, &draw_func);
            }

            // On to the next quad.
            if next_p1.y < next_p2.y {
                p1 = next_p1;
                next_p1 = match points.next() {
                    Some(next) => next,
                    None => break,
                }
            } else {
                p2 = next_p2;
                next_p2 = match points.next_back() {
                    Some(next) => next,
                    None => break,
                }
            }
        }
    }

    pub fn pixels(&self) -> &[u8; SCREEN_RESOLUTION[0] * SCREEN_RESOLUTION[1]] {
        &self.0
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }
}

#[derive(Clone)]
pub struct RasterRenderer {
    /// We Box the buffers to make sure they end up in the heap. Without this objects embedding
    /// this renderer may end up being build on the stack, which may not appreciate having 256KB
    /// requested from it.
    buffers: Box<[RefCell<IndexedImage>; 4]>,
}

impl RasterRenderer {
    pub fn new() -> RasterRenderer {
        RasterRenderer {
            buffers: Box::new([
                RefCell::new(Default::default()),
                RefCell::new(Default::default()),
                RefCell::new(Default::default()),
                RefCell::new(Default::default()),
            ]),
        }
    }

    pub fn get_buffer(&self, page_id: usize) -> Ref<'_, IndexedImage> {
        self.buffers[page_id].borrow()
    }
}

impl IndexedRenderer for RasterRenderer {
    fn fillvideopage(&mut self, dst_page_id: usize, color_idx: u8) {
        let mut dst = self.buffers[dst_page_id].borrow_mut();

        for pixel in dst.0.iter_mut() {
            *pixel = color_idx;
        }
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        if src_page_id == dst_page_id {
            tracing::warn!("cannot copy video page into itself");
            return;
        }

        if !(-199..=199).contains(&vscroll) {
            tracing::warn!("out-of-range vscroll for copyvideopage: {}", vscroll);
            return;
        }

        let src = &self.buffers[src_page_id].borrow_mut();
        let src_len = src.0.len();
        let dst = &mut self.buffers[dst_page_id].borrow_mut();
        let dst_len = dst.0.len();

        let src_start = if vscroll < 0 {
            vscroll.unsigned_abs() as usize * SCREEN_RESOLUTION[0]
        } else {
            0
        };
        let dst_start = if vscroll > 0 {
            vscroll.unsigned_abs() as usize * SCREEN_RESOLUTION[0]
        } else {
            0
        };
        let src_slice = &src.0[src_start..src_len - dst_start];
        let dst_slice = &mut dst.0[dst_start..dst_len - src_start];

        dst_slice.copy_from_slice(src_slice);
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        color: u8,
        zoom: u16,
        bb: (u8, u8),
        points: &[Point<u8>],
    ) {
        let mut dst = self.buffers[dst_page_id].borrow_mut();

        match color {
            // Direct indexed color - fill the buffer with that color.
            0x0..=0xf => {
                dst.fill_polygon(pos, offset, zoom, bb, points, |line, _off| line.fill(color))
            }
            // 0x10 special color - set the MSB of the current color to create
            // transparency effect.
            0x10 => dst.fill_polygon(pos, offset, zoom, bb, points, |line, _off| {
                for pixel in line {
                    *pixel |= 0x8
                }
            }),
            // 0x11 special color - copy the same pixel of buffer 0.
            0x11 => {
                // Do not try to copy page 0 into itself - not only the page won't change,
                // but this will actually panic as we try to double-borrow the page.
                if dst_page_id != 0 {
                    let src = self.buffers[0].borrow();
                    dst.fill_polygon(pos, offset, zoom, bb, points, |line, off| {
                        line.copy_from_slice(&src.0[off..off + line.len()]);
                    });
                }
            }
            color => panic!("Unexpected color 0x{:x}", color),
        };
    }

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color: u8, c: u8) {
        use crate::font::*;

        // Only direct colors are valid for fonts.
        if color > 0xf {
            tracing::error!("Unexpected font color 0x{:x}", color);
            return;
        }

        // Our font starts at the first supported character.
        let font_offset = match c {
            FONT_FIRST_CHAR..=FONT_LAST_CHAR => c - FONT_FIRST_CHAR,
            c => {
                tracing::error!(
                    "Character '{}' (0x{:x}) is not covered by font!",
                    c as char,
                    c
                );
                return;
            }
        } as usize
            * CHAR_HEIGHT;

        // Each character is encoded with 8 bytes, 1 byte per line.
        let char_bitmap = &FONT[font_offset..font_offset + CHAR_HEIGHT];

        let mut dst = self.buffers[dst_page_id].borrow_mut();
        for (i, char_line) in char_bitmap.iter().map(|b| b.reverse_bits()).enumerate() {
            dst.draw_hline(pos.1 + i as i16, pos.0, pos.0 + 8, |slice, off| {
                for (i, pixel) in slice.iter_mut().enumerate() {
                    if (char_line >> ((off + i) & 0x7) & 0x1) == 1 {
                        *pixel = color
                    }
                }
            })
        }
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        assert_eq!(buffer.len(), 32000);
        let mut dst = self.buffers[dst_page_id].borrow_mut();
        dst.set_content(buffer)
            .unwrap_or_else(|e| tracing::error!("blit_buffer failed: {}", e));
    }
}

impl Snapshotable for RasterRenderer {
    type State = Self;

    fn take_snapshot(&self) -> Self::State {
        self.clone()
    }

    fn restore_snapshot(&mut self, snapshot: &Self::State) -> bool {
        *self = snapshot.clone();
        true
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    /// Check that a newly created image is all blank.
    fn test_new_image() {
        let image: IndexedImage = Default::default();

        for pixel in image.0.iter() {
            assert_eq!(*pixel, 0);
        }
    }

    #[test]
    fn test_set_get_pixel() {
        let mut image: IndexedImage = Default::default();

        assert_eq!(image.get_pixel(10, 27), Ok(0x0));

        image.set_pixel(10, 27, 0xe);
        assert_eq!(image.get_pixel(10, 27), Ok(0xe));

        // Should do nothing
        image.set_pixel(1000, 1000, 0x1);
        assert_eq!(image.get_pixel(1000, 1000), Err(()));
    }
}
