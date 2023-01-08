pub mod gl;
pub mod polygon;
pub mod raster;

#[cfg(feature = "sdl2-sys")]
pub mod sdl2;

use std::{
    any::Any,
    fmt::{self, Debug, Formatter, Result},
    ops::DerefMut,
};

use tracing::debug;

use crate::sys::Snapshotable;

/// Native screen resolution of the game.
pub const SCREEN_RESOLUTION: [usize; 2] = [320, 200];

/// Trait for rendering the game using four 16-color indexed buffers.
pub trait IndexedRenderer {
    /// Fill video page `page_id` entirely with color `color_idx`.
    fn fillvideopage(&mut self, page_id: usize, color_idx: u8);
    /// Copy video page `src_page_id` into `dst_page_id`. `vscroll` is a vertical offset
    /// for the copy.
    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16);
    /// Draw `polygon` with color index `color_idx` on page `dst_page_id`. `x` and `y` are the
    /// coordinates of the center of the polygon on the page. `zoom` is a zoom factor by which
    /// every point of the polygon must be multiplied by, and then divided by 64.
    ///
    /// This has too many arguments, but we are going to fix this later as we switch to a
    /// higher-level method for polygon filling.
    #[allow(clippy::too_many_arguments)]
    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        color_idx: u8,
        zoom: u16,
        bb: (u8, u8),
        points: &[Point<u8>],
    );
    /// Draw character `c` at position `pos` of page `dst_page_id` with color `color_idx`.
    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8);
    /// Blit `buffer` (a bitmap of full screen size) into `dst_page_id`. This is used
    /// as an alternative to creating scenes using polys notably in later scenes of
    /// the game (maybe because of lack of time?).
    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]);
}

/// Proxy implementation for containers of `IndexedRenderer`.
impl<R: IndexedRenderer + ?Sized, C: DerefMut<Target = R>> IndexedRenderer for C {
    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        self.deref_mut().fillvideopage(page_id, color_idx)
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        self.deref_mut()
            .copyvideopage(src_page_id, dst_page_id, vscroll)
    }

    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        color_idx: u8,
        zoom: u16,
        bb: (u8, u8),
        points: &[Point<u8>],
    ) {
        self.deref_mut()
            .fillpolygon(dst_page_id, pos, offset, color_idx, zoom, bb, points)
    }

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8) {
        self.deref_mut().draw_char(dst_page_id, pos, color_idx, c)
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.deref_mut().blit_buffer(dst_page_id, buffer)
    }
}

/// Trait for displaying an indexed-color buffer using a given palette on the screen.
pub trait Display {
    /// Show `page_id` on the screen, using `palette` to render its actual colors.
    fn blitframebuffer(&mut self, page_id: usize, palette: &Palette);
}

/// Proxy implementation for containers of `Display`.
impl<D: Display + ?Sized, C: DerefMut<Target = D>> Display for C {
    fn blitframebuffer(&mut self, page_id: usize, palette: &Palette) {
        self.deref_mut().blitframebuffer(page_id, palette)
    }
}

/// Trait providing the methods necessary for the VM to render the game.
pub trait Gfx: IndexedRenderer + Display + Snapshotable<State = Box<dyn Any>> {}

/// Proxy implementation for containers of `Gfx`.
impl<
        G: Gfx + ?Sized,
        C: DerefMut<Target = G> + IndexedRenderer + Display + Snapshotable<State = Box<dyn Any>>,
    > Gfx for C
{
}

/// A point as described in the game's resources for polygons.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

impl<T: fmt::Display> Debug for Point<T> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

impl From<Point<i16>> for Point<f64> {
    fn from(p: Point<i16>) -> Self {
        Point {
            x: p.x.into(),
            y: p.y.into(),
        }
    }
}

impl<T> Point<T> {
    pub fn new(x: T, y: T) -> Self {
        Point { x, y }
    }
}

/// A single color from a game's palette which components have been normalized to cover the u8
/// range.
///
/// We use a C representation aligned to 32 bits so this can safely be passed to shaders.
#[repr(C, align(4))]
#[derive(Debug, Default, Clone)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub const PALETTE_SIZE: usize = 16;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct Palette([Color; PALETTE_SIZE]);

impl Palette {
    /// Sets the current |palette| from a raw PALETTE resource.
    pub fn set(&mut self, palette: &[u8; 32]) {
        for i in 0..16 {
            let c1 = palette[i * 2];
            let c2 = palette[i * 2 + 1];

            let b = c2 & 0x0f;
            let g = (c2 & 0xf0) >> 4;
            let r = c1 & 0x0f;

            let col = &mut self.0[i];
            // We only have 4 bits worth of intensity per color. Copy them to
            // the high bits so we have enough luminosity.
            col.r = (r << 4) | r;
            col.g = (g << 4) | g;
            col.b = (b << 4) | b;

            debug!("palette[{:x}] = {:x?}", i, col);
        }
    }

    /// Return the RGB color corresponding to |color_idx|.
    /// A palette only has 16 colors, so this method will panic if |color_idx|
    /// is bigger than 0xf.
    pub fn lookup(&self, color_idx: u8) -> &Color {
        assert!(color_idx <= 0xf);

        &self.0[color_idx as usize]
    }

    pub fn as_ptr(&self) -> *const Color {
        self.0.as_ptr()
    }
}

/// Returns how many units of x we need to move per unit of y in order to follow
/// the slope defined by the vector (p1, p2).
/// Returns None if the line is horizontal (i.e. there is no slope)
fn slope<T, U>(p1: &Point<T>, p2: &Point<T>) -> Option<U>
where
    T: Copy,
    T: std::cmp::PartialEq,
    T: std::default::Default,
    T: std::ops::Sub<Output = T>,
    T::Output: Into<U>,
    U: std::ops::Div<Output = U>,
    U: std::default::Default,
{
    let vx = p2.x - p1.x;
    let vy = p2.y - p1.y;

    if vy == T::default() {
        None
    } else if vx == T::default() {
        Some(U::default())
    } else {
        Some(vx.into() / vy.into())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_slope() {
        let p1 = Point::new(0, 0);
        let p2 = Point::new(1, 2);
        let res: Option<f64> = slope(&p1, &p2);
        assert_eq!(res, Some(0.5));

        let p1 = Point::new(0, 0);
        let p2 = Point::new(-1, 2);
        let res: Option<f64> = slope(&p1, &p2);
        assert_eq!(res, Some(-0.5));

        let p1 = Point::new(0, 0);
        let p2 = Point::new(0, 2);
        let res: Option<f64> = slope(&p1, &p2);
        assert_eq!(res, Some(0.0));

        let p1 = Point::new(0, 0);
        let p2 = Point::new(5, 0);
        let res: Option<f64> = slope(&p1, &p2);
        assert_eq!(res, None);
    }
}
