//! Module containing the traits and implementations for rendering the game.
//!
//! The whole game is rendered using a handful of VM graphics operations.

use std::ops::DerefMut;

use crate::gfx::PolySegment;

/// Trait for rendering the game from the VM graphics operations.
///
/// Implementors receive the VM rendering commands as-is. They are also expected to implement
/// [`InitForScene`] in order to receive and process the graphics segments which the rendering
/// commands depend on.
pub trait GameRenderer {
    /// Fill video page `page_id` entirely with color `color_idx`.
    fn fillvideopage(&mut self, page_id: usize, color_idx: u8);
    /// Copy video page `src_page_id` into `dst_page_id`. `vscroll` is a vertical offset
    /// for the copy.
    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16);
    /// Draw the polygons which data starts at `offset` of `segment`.
    ///
    /// `pos` is the coordinates of the center of the polygon on the page. `zoom` is a zoom factor
    /// by which every point of the polygon must be multiplied by, and then divided by 64.
    fn draw_polygons(
        &mut self,
        segment: PolySegment,
        start_offset: u16,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
    );
    /// Draw character `c` at position `pos` of page `dst_page_id` with color `color_idx`.
    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8);
    /// Blit `buffer` (a bitmap of full screen size) into `dst_page_id`. This is used
    /// as an alternative to creating scenes using polys notably in later scenes of
    /// the game (maybe because of lack of time?).
    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]);
}

/// Proxy implementation for containers of `GameRenderer`.
impl<R: GameRenderer + ?Sized, C: DerefMut<Target = R>> GameRenderer for C {
    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        self.deref_mut().fillvideopage(page_id, color_idx)
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        self.deref_mut()
            .copyvideopage(src_page_id, dst_page_id, vscroll)
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
        self.deref_mut()
            .draw_polygons(segment, start_offset, dst_page_id, pos, offset, zoom)
    }

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8) {
        self.deref_mut().draw_char(dst_page_id, pos, color_idx, c)
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.deref_mut().blit_buffer(dst_page_id, buffer)
    }
}
