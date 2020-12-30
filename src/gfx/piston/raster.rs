use super::PistonBackend;
use crate::gfx::{Backend, Color, Palette, Point, Polygon};

use std::any::Any;
use std::cell::RefCell;
use std::cmp::{max, min};

use image as im;
use piston::input::RenderArgs;

use opengl_graphics as gl;

use super::super::GfxSnapshot;
use super::super::SCREEN_RESOLUTION;

#[derive(Clone)]
struct IndexedImage([u8; SCREEN_RESOLUTION[0] * SCREEN_RESOLUTION[1]]);

fn slope_step(p1: &Point<i32>, p2: &Point<i32>) -> i32 {
    let dy = p2.y - p1.y;
    let dx = p2.x - p1.x;

    if dy != 0 {
        dx / dy
    } else {
        0
    }
}

impl IndexedImage {
    fn new() -> IndexedImage {
        IndexedImage([0u8; SCREEN_RESOLUTION[0] * SCREEN_RESOLUTION[1]])
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
    fn draw_hline<F>(&mut self, y: i16, x1: i16, x2: i16, draw_func: &F)
    where
        F: Fn(&mut u8, usize),
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
        let mut offset = line_offset + x_start;
        for pixel in slice {
            draw_func(pixel, offset);
            offset += 1;
        }
    }

    fn fill_polygon<F>(&mut self, x: i16, y: i16, polygon: &Polygon, draw_func: F)
    where
        F: Fn(&mut u8, usize),
    {
        assert!(polygon.points.len() >= 4);

        // Optimization for single-pixel polygons
        if polygon.bbw == 0 && polygon.bbh == 0 {
            if let Ok(offset) = IndexedImage::offset(x, y) {
                draw_func(&mut self.0[offset], offset);
            }
            return;
        }

        // Offset x and y by the polygon center.
        let offset = (polygon.bbw / 2, polygon.bbh / 2);
        let x = x - offset.0 as i16;
        let y = y - offset.1 as i16;

        // The first and last points are always at the top. We will fill
        // the polygon line by line starting from them, and stop when the front
        // and back join at the bottom of the polygon.
        let mut points = polygon
            .points
            .iter()
            // Add the x and y offsets.
            .map(|p| Point::from((p.x as i16 + x, p.y as i16 + y)))
            // Turn the point into i32 and add 16 bits of fixed decimals to x to
            // add some precision when computing the slope.
            .map(|p| Point::<i32>::from(((p.x as i32) << 16, p.y as i32)));
        // We have at least 4 points in the polygon, so these unwraps() are safe.
        let mut p1 = points.next().unwrap();
        let mut p2 = points.next_back().unwrap();
        let mut next_p1 = points.next().unwrap();
        let mut next_p2 = points.next_back().unwrap();

        // Loop over all the points of the polygon.
        loop {
            // Vertical range of the quad.
            let v_range = max(p1.y, p2.y)..min(next_p1.y, next_p2.y);
            let slope1 = slope_step(&p1, &next_p1);
            let slope2 = slope_step(&p2, &next_p2);

            // For each vertical line, add the slope factor to x.
            for (x1, x2, y) in v_range.scan((p1.x, p2.x), |state, y| {
                let ret = (state.0, state.1, y);
                state.0 += slope1;
                state.1 += slope2;
                Some(ret)
            }) {
                // Center the leftmost pixel and scale back.
                let x_start = ((min(x1, x2) + 0x7fff) >> 16) as i16;
                // Include the rightmost pixel in the line, center it, and scale back.
                let x_end = ((max(x1, x2) + 0x18000) >> 16) as i16;

                self.draw_hline(y as i16, x_start, x_end, &draw_func);
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
}

/// A software backend that aims at rendering the game identically to what
/// the original does.
pub struct PistonRasterBackend {
    gl: gl::GlGraphics,
    palette: Palette,
    texture: gl::Texture,
    buffers: [RefCell<IndexedImage>; 4],
    framebuffer: im::RgbaImage,
    framebuffer_index: usize,
}

pub fn new() -> PistonRasterBackend {
    // Black screen by default.
    let framebuffer = im::RgbaImage::from_pixel(
        SCREEN_RESOLUTION[0] as u32,
        SCREEN_RESOLUTION[1] as u32,
        im::Rgba([0, 0, 0, 255]),
    );

    let texture = gl::Texture::from_image(
        &framebuffer,
        &gl::TextureSettings::new().filter(gl::Filter::Nearest),
    );

    PistonRasterBackend {
        gl: gl::GlGraphics::new(super::OPENGL_VERSION),
        palette: Default::default(),
        texture,
        buffers: [
            RefCell::new(IndexedImage::new()),
            RefCell::new(IndexedImage::new()),
            RefCell::new(IndexedImage::new()),
            RefCell::new(IndexedImage::new()),
        ],
        framebuffer,
        framebuffer_index: 0,
    }
}

fn lookup_palette(palette: &Palette, color: u8) -> im::Rgba<u8> {
    let &Color { r, g, b } = palette.lookup(color);

    im::Rgba([r, g, b, 255])
}

impl Backend for PistonRasterBackend {
    fn set_palette(&mut self, palette: &[u8; 32]) {
        self.palette.set(palette);
    }

    fn fillvideopage(&mut self, dst_page_id: usize, color_idx: u8) {
        let mut dst = self.buffers[dst_page_id].borrow_mut();

        for pixel in dst.0.iter_mut() {
            *pixel = color_idx;
        }
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        let src = &self.buffers[src_page_id].borrow_mut();
        let src_len = src.0.len();
        let dst = &mut self.buffers[dst_page_id].borrow_mut();
        let dst_len = dst.0.len();

        let src_start = if vscroll > 0 {
            vscroll.abs() as usize * SCREEN_RESOLUTION[0]
        } else {
            0
        };
        let dst_start = if vscroll < 0 {
            vscroll.abs() as usize * SCREEN_RESOLUTION[0]
        } else {
            0
        };
        let src_slice = &src.0[src_start..src_len - dst_start];
        let dst_slice = &mut dst.0[dst_start..dst_len - src_start];

        dst_slice.copy_from_slice(src_slice);
    }

    fn fillpolygon(&mut self, dst_page_id: usize, x: i16, y: i16, color: u8, polygon: &Polygon) {
        let mut dst = self.buffers[dst_page_id].borrow_mut();

        match color {
            // Direct indexed color - fill the buffer with that color.
            0x0..=0xf => dst.fill_polygon(x, y, polygon, |pixel, _off| *pixel = color),
            // 0x10 special color - set the MSB of the current color to create
            // transparency effect.
            0x10 => dst.fill_polygon(x, y, polygon, |pixel, _off| *pixel |= 0x8),
            // 0x11 special color - copy the same pixel of buffer 0.
            0x11 => {
                let src = self.buffers[0].borrow();
                dst.fill_polygon(x, y, polygon, |pixel, off| *pixel = src.0[off]);
            }
            color => panic!("Unexpected color 0x{:x}", color),
        };
    }

    fn blitframebuffer(&mut self, page_id: usize) {
        self.framebuffer_index = page_id;
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        assert_eq!(buffer.len(), 32000);
        let mut dst = self.buffers[dst_page_id].borrow_mut();
        let planes: Vec<&[u8]> = buffer.chunks(8000).collect();

        for (i, pixel) in dst.0.iter_mut().enumerate() {
            let idx = i / 8;
            let bit = 7 - (i % 8);
            *pixel = (planes[0][idx] >> bit) & 0b1
                | ((planes[1][idx] >> bit) & 0b1) << 1
                | ((planes[2][idx] >> bit) & 0b1) << 2
                | ((planes[3][idx] >> bit) & 0b1) << 3;
        }
    }

    fn get_snapshot(&self) -> Box<dyn Any> {
        Box::new(RasterGfxSnapshot {
            palette: self.palette.clone(),
            buffers: self.buffers.clone(),
            framebuffer: self.framebuffer.clone(),
        })
    }

    fn set_snapshot(&mut self, snapshot: Box<dyn Any>) {
        if let Ok(snapshot) = snapshot.downcast::<RasterGfxSnapshot>() {
            self.palette = snapshot.palette;
            self.buffers = snapshot.buffers;
            self.framebuffer = snapshot.framebuffer;
        } else {
            eprintln!("Attempting to restore invalid gfx snapshot, ignoring");
        }
    }
}

struct RasterGfxSnapshot {
    palette: Palette,
    buffers: [RefCell<IndexedImage>; 4],
    framebuffer: im::RgbaImage,
}
impl GfxSnapshot for RasterGfxSnapshot {}

impl PistonBackend for PistonRasterBackend {
    fn render(&mut self, args: &RenderArgs) {
        use crate::gfx::SCREEN_RATIO;
        use graphics::*;

        let window_w = args.window_size[0];
        let window_h = args.window_size[1];

        let image = Image::new().rect(if window_w / SCREEN_RATIO < window_h {
            let w = window_w;
            let h = window_w / SCREEN_RATIO;
            [0.0, (window_h - h) / 2.0, w, h]
        } else {
            let w = window_h * SCREEN_RATIO;
            let h = window_h;
            [(window_w - w) / 2.0, 0.0, w, h]
        });

        // Translate the indexed pixels into RGBA values using the palette.
        for pixel in self
            .framebuffer
            .pixels_mut()
            .zip(self.buffers[self.framebuffer_index].borrow().0.iter())
        {
            *pixel.0 = lookup_palette(&self.palette, *pixel.1);
        }

        self.texture.update(&self.framebuffer);

        let context = self.gl.draw_begin(args.viewport());
        clear([0.0, 0.0, 0.0, 1.0], &mut self.gl);
        image.draw(
            &self.texture,
            &DrawState::default(),
            context.transform,
            &mut self.gl,
        );
        self.gl.draw_end();
    }

    fn as_gfx(&mut self) -> &mut dyn Backend {
        self
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    /// Check that a newly created image is all blank.
    fn test_new_image() {
        let image = IndexedImage::new();

        for pixel in image.0.iter() {
            assert_eq!(*pixel, 0);
        }
    }

    #[test]
    fn test_set_get_pixel() {
        let mut image = IndexedImage::new();

        assert_eq!(image.get_pixel(10, 27), Ok(0x0));

        image.set_pixel(10, 27, 0xe);
        assert_eq!(image.get_pixel(10, 27), Ok(0xe));

        // Should do nothing
        image.set_pixel(1000, 1000, 0x1);
        assert_eq!(image.get_pixel(1000, 1000), Err(()));
    }
}
