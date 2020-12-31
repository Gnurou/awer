pub mod polygon;
pub mod raster;

#[cfg(feature = "piston-sys")]
pub mod piston;

use log::debug;
use polygon::Polygon;
use std::any::Any;
use std::cmp::max;
use std::fmt::{Debug, Display, Formatter, Result};

pub const SCREEN_RESOLUTION: [usize; 2] = [320, 200];

// A 4:3 screen ratio should reproduce the 1991 experience.
const SCREEN_RATIO: f64 = 4.0 / 3.0;

pub trait GfxSnapshot: Any {}

impl GfxSnapshot for () {}

pub trait Backend {
    fn set_palette(&mut self, palette: &[u8; 32]);
    fn fillvideopage(&mut self, page_id: usize, color_idx: u8);
    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16);
    fn fillpolygon(&mut self, dst_page_id: usize, x: i16, y: i16, color_idx: u8, polygon: &Polygon);
    fn blitframebuffer(&mut self, page_id: usize);
    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]);

    /// Get a snapshot object from the state of the backend. The `set_snapshot()`
    /// method must be able to restore the exact current state when given this
    /// object back.
    ///
    /// The default implementation returns an empty object.
    fn get_snapshot(&self) -> Box<dyn Any> {
        Box::new(())
    }
    /// Restore a previously saved state.
    ///
    /// The default implementation does nothing, which means glitches are to
    /// be expected for backends that do not override this method.
    fn set_snapshot(&mut self, _snapshot: Box<dyn Any>) {}
}

#[derive(Clone, Copy)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

impl<T: Display> Display for Point<T> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

impl<T: Display> Debug for Point<T> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

impl<T: PartialEq> PartialEq for Point<T> {
    fn eq(&self, p: &Self) -> bool {
        self.x == p.x && self.y == p.y
    }
}

impl<T> From<(T, T)> for Point<T> {
    fn from(p: (T, T)) -> Self {
        Self { x: p.0, y: p.1 }
    }
}

impl<T: Copy> From<[T; 2]> for Point<T> {
    fn from(p: [T; 2]) -> Self {
        Self { x: p[0], y: p[1] }
    }
}

impl From<Point<u16>> for Point<f64> {
    fn from(p: Point<u16>) -> Self {
        Point {
            x: p.x.into(),
            y: p.y.into(),
        }
    }
}

impl<T> Point<T> {
    fn new(x: T, y: T) -> Self {
        Point { x, y }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    fn blend(c1: &Color, c2: &Color) -> Color {
        Color {
            r: max(c2.r as i16 - c1.r as i16, 0) as u8,
            g: max(c2.g as i16 - c1.g as i16, 0) as u8,
            b: max(c2.b as i16 - c1.b as i16, 0) as u8,
        }
    }

    fn mix(c1: &Color, c2: &Color) -> Color {
        Color {
            r: ((c1.r as u16 + c2.r as u16) / 2) as u8,
            g: ((c1.g as u16 + c2.g as u16) / 2) as u8,
            b: ((c1.b as u16 + c2.b as u16) / 2) as u8,
        }
    }

    fn normalize(&self, alpha: f32) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            alpha,
        ]
    }
}

pub const PALETTE_SIZE: usize = 16;

#[derive(Debug, Default, Clone)]
pub struct Palette([Color; PALETTE_SIZE]);

impl Palette {
    /// Sets the current |palette| from a raw PALETTE resource.
    fn set(&mut self, palette: &[u8; 32]) {
        for i in 0..16 {
            let c1 = palette[i * 2];
            let c2 = palette[i * 2 + 1];

            let b = (c2 & 0x0f) as u8;
            let g = ((c2 & 0xf0) >> 4) as u8;
            let r = (c1 & 0x0f) as u8;

            let col = &mut self.0[i];
            // We only have 4 bits worth of intensity per color. Copy them to
            // the high bits so we have enough luminosity.
            col.r = (r << 4) | r;
            col.g = (g << 4) | g;
            col.b = (b << 4) | b;

            debug!("set palette[{:x}]: {:x?}", i, col);
        }
    }

    /// Return the RGB color corresponding to |color_idx|.
    /// A palette only has 16 colors, so this method will panic if |color_idx|
    /// is bigger than 0xf.
    pub fn lookup(&self, color_idx: u8) -> &Color {
        assert!(color_idx <= 0xf);

        &self.0[color_idx as usize]
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
