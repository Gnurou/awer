use std::cell::Ref;
use std::cell::RefCell;

use crate::gfx::polygon::Polygon;
use crate::gfx::sw::IndexedImage;
use crate::gfx::GameRenderer;
use crate::gfx::PolySegment;
use crate::gfx::PolygonFiller;
use crate::gfx::SimplePolygonRenderer;
use crate::gfx::SCREEN_RESOLUTION;
use crate::scenes::InitForScene;
use crate::sys::Snapshotable;

#[derive(Clone)]
struct RasterRendererBuffers(Box<[RefCell<IndexedImage>; 4]>);

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
/// CPU renderer for the game.
///
/// This is the renderer closest to the original game. It uses the CPU for rasterizing each polygon
/// and filling its lines.
#[derive(Clone)]
pub struct RasterGameRenderer {
    renderer: SimplePolygonRenderer,
    buffers: RasterRendererBuffers,
}

impl RasterGameRenderer {
    pub fn new() -> RasterGameRenderer {
        RasterGameRenderer {
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

impl InitForScene for RasterGameRenderer {
    #[tracing::instrument(skip(self, resman))]
    fn init_from_scene(
        &mut self,
        resman: &crate::res::ResourceManager,
        scene: &crate::scenes::Scene,
    ) -> std::io::Result<()> {
        self.renderer.init_from_scene(resman, scene)
    }
}

// The poly operation is the only one depending on the renderer AND the buffers. All the others
// only need the buffers.
impl GameRenderer for RasterGameRenderer {
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
        segment: PolySegment,
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

impl Snapshotable for RasterGameRenderer {
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
