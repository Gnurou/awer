mod game_renderer;

pub use game_renderer::RasterGameRenderer;

use anyhow::anyhow;
use anyhow::Result;

use crate::gfx::polygon::Polygon;
use crate::gfx::polygon::Trapezoid;
use crate::gfx::polygon::TrapezoidLine;
use crate::gfx::SCREEN_RESOLUTION;

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

    #[allow(dead_code)]
    pub fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }
}
