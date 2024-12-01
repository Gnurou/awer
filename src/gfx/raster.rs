use std::cell::Ref;
use std::cell::RefCell;

use anyhow::anyhow;
use anyhow::Result;

use crate::gfx::polygon::Trapezoid;
use crate::gfx::IndexedRenderer;
use crate::gfx::SCREEN_RESOLUTION;
use crate::scenes::InitForScene;
use crate::sys::Snapshotable;

use super::polygon::Polygon;
use super::polygon::TrapezoidLine;
use super::PolygonFiller;
use super::SimplePolygonRenderer;

/// Apply the zoom function on a point's coordinate `p`: multiply it by `zoom`,
/// then divide by 64.
fn scale(p: i16, zoom: u16) -> i16 {
    ((p as i32 * zoom as i32) / 64) as i16
}

/// Rasterizer implementation for a `Trapezoid<i16>`.
///
/// `i16` is a good type for screen coordinates, as it covers any realistic display resolution
/// while allowing negative coordinates that occur as we translate and scale the points.
/// Moreover it lets us use 32-bit fixed-point arithmetic to precisely interpolate the start and
/// end of each line.
impl Trapezoid<i16> {
    /// Returns an iterator over all the pixel-lines of this trapezoid.
    ///
    /// Filling each returned line with the color of the polygon will result in it being properly
    /// rendered.
    pub fn raster_iterator(&self) -> impl Iterator<Item = TrapezoidLine<i16>> {
        // Interestingly the `y` range does not seem to be inclusive?
        let v_range = self.top.y..self.bot.y;
        let dy = v_range.len() as i32;

        let x_top_start = (*self.top.x_range.start() as i32) << 16;
        let x_top_end = (*self.top.x_range.end() as i32) << 16;
        let x_bot_start = (*self.bot.x_range.start() as i32) << 16;
        let x_bot_end = (*self.bot.x_range.end() as i32) << 16;

        // How many units we should move on the `x` axis per vertical line on the left and right
        // side.
        let slope_left = (x_bot_start - x_top_start).checked_div(dy).unwrap_or(0);
        let slope_right = (x_bot_end - x_top_end).checked_div(dy).unwrap_or(0);

        v_range.scan((x_top_start, x_top_end), move |(left, right), y| {
            // Center the leftmost pixel and scale back.
            let start_x = ((*left + 0x7fff) >> 16) as i16;
            // Center the rightmost pixel and scale back.
            let end_x = ((*right + 0x8000) >> 16) as i16;
            *left += slope_left;
            *right += slope_right;
            Some(TrapezoidLine {
                x_range: start_x..=end_x,
                y,
            })
        })
    }
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

    /// Draw a horizontal line at ordinate `y`, between `x_range`.
    fn draw_hline<F>(&mut self, x_range: std::ops::RangeInclusive<i16>, y: i16, draw_func: F)
    where
        F: Fn(&mut [u8], usize),
    {
        let line_offset = match IndexedImage::offset(0, y) {
            Ok(offset) => offset,
            // Line is not on screen.
            Err(_) => return,
        };

        // Limit x_start and x_stop to [0..SCREEN_RESOLUTION[0]].
        let x_start = ((*x_range.start()).clamp(0, SCREEN_RESOLUTION[0] as i16 - 1)) as usize;
        let x_stop = ((*x_range.end()).clamp(0, SCREEN_RESOLUTION[0] as i16 - 1)) as usize;

        let slice = &mut self.0[line_offset + x_start..=line_offset + x_stop];
        draw_func(slice, line_offset + x_start);
    }

    fn fill_polygon<F>(
        &mut self,
        poly: &Polygon,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
        draw_func: F,
    ) where
        F: Fn(&mut [u8], usize),
    {
        let bb = poly.bb();

        // Optimization for single-pixel polygons
        if bb == (0, 0) {
            if let Ok(offset) = IndexedImage::offset(pos.0, pos.1) {
                draw_func(&mut self.0[offset..offset + 1], offset);
            }
            return;
        }

        // Offset x and y by the polygon center.
        let bbox_offset = (scale(bb.0 as i16, zoom) / 2, scale(bb.1 as i16, zoom) / 2);
        let offset = (scale(offset.0, zoom), scale(offset.1, zoom));
        let tx = pos.0 + offset.0 - bbox_offset.0;
        let ty = pos.1 + offset.1 - bbox_offset.1;

        let trapezoids = poly
            .trapezoid_iter()
            // Use `i16` as the scaling and translate operations might move our points out of the
            // `u8` range.
            .map(|t| Trapezoid::<i16>::from(&t))
            .map(|t| t.scale(zoom))
            .map(|t| t.translate((tx, ty)));

        for trapezoid in trapezoids {
            for line in trapezoid.raster_iterator() {
                self.draw_hline(line.x_range, line.y, &draw_func);
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
pub struct RasterRendererBuffers(Box<[RefCell<IndexedImage>; 4]>);

impl PolygonFiller for RasterRendererBuffers {
    #[tracing::instrument(level = "trace", skip(self))]
    fn fill_polygon(
        &mut self,
        poly: &Polygon,
        color: u8,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
    ) {
        let mut dst = self.0[dst_page_id].borrow_mut();

        match color {
            // Direct indexed color - fill the buffer with that color.
            0x0..=0xf => dst.fill_polygon(poly, pos, offset, zoom, |line, _off| line.fill(color)),
            // 0x10 special color - set the MSB of the current color to create
            // transparency effect.
            0x10 => dst.fill_polygon(poly, pos, offset, zoom, |line, _off| {
                for pixel in line {
                    *pixel |= 0x8
                }
            }),
            // 0x11 special color - copy the same pixel of buffer 0.
            0x11 => {
                // Do not try to copy page 0 into itself - not only the page won't change,
                // but this will actually panic as we try to double-borrow the page.
                if dst_page_id != 0 {
                    let src = self.0[0].borrow();
                    dst.fill_polygon(poly, pos, offset, zoom, |line, off| {
                        line.copy_from_slice(&src.0[off..off + line.len()]);
                    });
                }
            }
            color => panic!("Unexpected color 0x{:x}", color),
        };
    }
}

#[derive(Clone)]
pub struct RasterRenderer {
    renderer: SimplePolygonRenderer,
    buffers: RasterRendererBuffers,
}

impl RasterRenderer {
    pub fn new() -> RasterRenderer {
        RasterRenderer {
            renderer: Default::default(),
            buffers: RasterRendererBuffers(Box::new([
                RefCell::new(Default::default()),
                RefCell::new(Default::default()),
                RefCell::new(Default::default()),
                RefCell::new(Default::default()),
            ])),
        }
    }

    pub fn get_buffer(&self, page_id: usize) -> Ref<'_, IndexedImage> {
        self.buffers.0[page_id].borrow()
    }
}

impl InitForScene for RasterRenderer {
    #[tracing::instrument(skip(self, resman))]
    fn init_from_scene(
        &mut self,
        resman: &crate::res::ResourceManager,
        scene: &crate::scenes::Scene,
    ) {
        self.renderer.init_from_scene(resman, scene)
    }
}

// The poly operation is the only one depending on the renderer AND the buffers. All the others
// only need the buffers.
impl IndexedRenderer for RasterRenderer {
    fn fillvideopage(&mut self, dst_page_id: usize, color_idx: u8) {
        let mut dst = self.buffers.0[dst_page_id].borrow_mut();

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

        let src = &self.buffers.0[src_page_id].borrow_mut();
        let src_len = src.0.len();
        let dst = &mut self.buffers.0[dst_page_id].borrow_mut();
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

        let mut dst = self.buffers.0[dst_page_id].borrow_mut();
        for (i, char_line) in char_bitmap.iter().map(|b| b.reverse_bits()).enumerate() {
            dst.draw_hline(pos.0..=(pos.0 + 7), pos.1 + i as i16, |slice, off| {
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
        let mut dst = self.buffers.0[dst_page_id].borrow_mut();
        dst.set_content(buffer)
            .unwrap_or_else(|e| tracing::error!("blit_buffer failed: {}", e));
    }

    fn draw_polygons(
        &mut self,
        segment: super::PolySegment,
        start_offset: u16,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
    ) {
        self.renderer.draw_polygons(
            segment,
            start_offset,
            dst_page_id,
            pos,
            offset,
            zoom,
            &mut self.buffers,
        )
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
