//! Utilities to manipulate polygons.
//!
//! The vertices of the polygons are given in a particular order: always
//! clockwise, starting with the upper-right vertex. The last vertex, which is
//! always the top-left one, is also guaranteed to have the same `y` as the
//! first one. Similarly, there are two consecutive points with the same `y` at
//! the bottom of the polygon (identical if need be). This gives us another
//! guarantee: that each polygon is made of at least 4 vertices.
//!
//! These properties are useful for rasterizing the polygons quickly.
//!
//! They are also useful when using modern rendering APIs that do not support
//! concave polygons: by parsing the list of polygons from both ends, we can
//! generate intermediate points and quads with horizontal top and botton lines.
//! These quads are guaranteed to be convex.
use std::borrow::Borrow;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt::Result;
use std::ops::Deref;
use std::ops::RangeInclusive;

use zerocopy::FromBytes;
use zerocopy::Immutable;
use zerocopy::IntoBytes;
use zerocopy::KnownLayout;
use zerocopy::Unaligned;

/// Apply the zoom function on a point's coordinate `p`: multiply it by `zoom`,
/// then divide by 64.
fn coord_scale(p: i16, zoom: u16) -> i16 {
    ((p as i32 * zoom as i32) / 64) as i16
}

/// A point as described in the game's resources for polygons.
///
/// When `T` is `u8` this corresponds to the native format of a point in the game's graphics
/// segment, hence the use of C representation.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Immutable, FromBytes, IntoBytes, Unaligned)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

impl From<Point<u8>> for Point<f64> {
    fn from(p: Point<u8>) -> Self {
        Point {
            x: p.x.into(),
            y: p.y.into(),
        }
    }
}

/// Data describing a polygon in the graphics segment.
///
/// Polygons are defined by a set of [`Point`]s, and a bounding box including them all.
///
/// The points define a series of [`Trapezoid`]s, and the following invariants are always true:
///
/// - There are at least 4 points per polygon,
/// - The total number of points is a multiple of 2,
/// - Points define the shape of the polygon, starting from the top, and going either clockwise or
///   counter-clockwise.
/// - Opposite points (e.g. the first and last point, or the second and second-to-last point, etc.)
///   have the same `y` coordinate.
///
/// These invariants make it very easy to rasterize the polygons, as opposite points can be used to
/// create the top and bottom lines of a trapezoid, which can then easily be filled by filling its
/// lines one by one. The only difficulty being that since the order of the points can be clockwise
/// or counter-clockwise, we need to compare them in order to find the left and right one.
///
/// This dynamically-sized type is designed to be used as a direct reference to the segment, not as
/// an owned version of the data, hence the packed C representation. [`OwnedPolygon`] can be used
/// whenever these is a need to store the polygon data somewhere.
#[repr(C, packed)]
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
pub struct Polygon {
    /// Bounding box of the polygon, including all of its points. This allows us to quickly compute
    /// the center of the polygon.
    pub bb: [u8; 2],
    /// Number of [`Point`]s in the `point` member below.
    ///
    /// This is normally never used and is only here because it is part of the graphics segment
    /// layout.
    _nb_points: u8,
    /// Array of the points making this polygon.
    pub points: [Point<u8>],
}

impl Debug for Polygon {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let points_slice = &self.points;
        f.debug_struct("Polygon")
            .field("bb", &self.bb)
            .field("points", &points_slice)
            .finish()
    }
}

impl ToOwned for Polygon {
    type Owned = OwnedPolygon;

    fn to_owned(&self) -> Self::Owned {
        OwnedPolygon {
            data: self.as_bytes().to_owned(),
        }
    }
}

impl Polygon {
    pub fn bb(&self) -> (u8, u8) {
        (self.bb[0], self.bb[1])
    }

    pub fn points_iter(&self) -> impl DoubleEndedIterator<Item = Point<u8>> + '_ {
        self.points.iter().cloned()
    }

    pub fn line_iter(&self) -> impl Iterator<Item = TrapezoidLine<u8>> + '_ {
        TrapezoidLineIterator {
            iter: self.points_iter(),
        }
    }

    pub fn trapezoid_iter(&self) -> impl Iterator<Item = Trapezoid<u8>> + '_ {
        let mut iter = self.line_iter();
        TrapezoidIterator {
            cur_line: iter.next().unwrap_or(TrapezoidLine {
                x_range: 0..=0,
                y: 0,
            }),
            iter,
        }
    }
}

/// Owned version of [`Polygon`]. Useful for renderers that need to put polygon data aside.
#[derive(Clone)]
pub struct OwnedPolygon {
    data: Vec<u8>,
}

impl Borrow<Polygon> for OwnedPolygon {
    fn borrow(&self) -> &Polygon {
        // SAFETY: guaranteed to succeed because we have been constructed from a valid [`Polygon`].
        Polygon::ref_from_bytes(&self.data).unwrap()
    }
}

impl Deref for OwnedPolygon {
    type Target = Polygon;

    fn deref(&self) -> &Self::Target {
        self.borrow()
    }
}

impl Debug for OwnedPolygon {
    fn fmt(&self, f: &mut Formatter) -> Result {
        self.deref().fmt(f)
    }
}

/// A line of a trapezoid.
///
/// [`Polygon`]s are aggregates of trapezoids that can be represented by their top and bottom line.
/// A line is defined by its range on the X axis and its Y position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrapezoidLine<T>
where
    T: Debug + Eq + Copy + PartialOrd + Ord,
{
    pub x_range: RangeInclusive<T>,
    pub y: T,
}

impl<T, U> From<&TrapezoidLine<T>> for TrapezoidLine<U>
where
    T: Debug + Eq + Copy + PartialOrd + Ord,
    U: Debug + Eq + Copy + PartialOrd + Ord + From<T>,
{
    fn from(t: &TrapezoidLine<T>) -> Self {
        TrapezoidLine {
            x_range: U::from(*t.x_range.start())..=U::from(*t.x_range.end()),
            y: U::from(t.y),
        }
    }
}

impl TrapezoidLine<i16> {
    pub fn scale(&self, zoom: u16) -> Self {
        Self {
            x_range: coord_scale(*self.x_range.start(), zoom)
                ..=coord_scale(*self.x_range.end(), zoom),
            y: coord_scale(self.y, zoom),
        }
    }

    pub fn translate(&self, t: (i16, i16)) -> Self {
        let start = *self.x_range.start() + t.0;
        let end = *self.x_range.end() + t.0;
        Self {
            x_range: start..=end,
            y: self.y + t.1,
        }
    }
}

pub struct TrapezoidLineIterator<T, I>
where
    I: DoubleEndedIterator<Item = Point<T>>,
{
    iter: I,
}

impl<T, I> Iterator for TrapezoidLineIterator<T, I>
where
    I: DoubleEndedIterator<Item = Point<T>>,
    T: Debug + Eq + Copy + PartialOrd + Ord,
{
    type Item = TrapezoidLine<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let (p1, p2) = (self.iter.next_back()?, self.iter.next()?);

        // Opposite points are supposed to have the same `y` coordinate.
        assert_eq!(p1.y, p2.y);
        let y = p1.y;

        let (left, right) = if p1.x <= p2.x {
            (p1.x, p2.x)
        } else {
            (p2.x, p1.x)
        };

        Some(TrapezoidLine {
            x_range: left..=right,
            y,
        })
    }
}

/// A trapezoid representation.
///
/// Polygons in Another World are exclusively made of trapezoid, and they are also used to make
/// rasterization fast and easy. A trapezoid is represented by its top and bottom lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trapezoid<T>
where
    T: Debug + Eq + Copy + PartialOrd + Ord,
{
    pub top: TrapezoidLine<T>,
    pub bot: TrapezoidLine<T>,
}

impl<T, U> From<&Trapezoid<T>> for Trapezoid<U>
where
    T: Debug + Eq + Copy + PartialOrd + Ord,
    U: Debug + Eq + Copy + PartialOrd + Ord + From<T>,
{
    fn from(t: &Trapezoid<T>) -> Self {
        Self {
            top: TrapezoidLine::<U>::from(&t.top),
            bot: TrapezoidLine::<U>::from(&t.bot),
        }
    }
}

impl Trapezoid<i16> {
    pub fn scale(&self, zoom: u16) -> Self {
        Self {
            top: self.top.scale(zoom),
            bot: self.bot.scale(zoom),
        }
    }

    pub fn translate(&self, t: (i16, i16)) -> Self {
        Self {
            top: self.top.translate(t),
            bot: self.bot.translate(t),
        }
    }
}

pub struct TrapezoidIterator<T, I>
where
    I: Iterator<Item = TrapezoidLine<T>>,
    T: Debug + Eq + Copy + PartialOrd + Ord,
{
    cur_line: TrapezoidLine<T>,
    iter: I,
}

impl<T, I> Iterator for TrapezoidIterator<T, I>
where
    I: Iterator<Item = TrapezoidLine<T>>,
    T: Debug + Eq + Copy + PartialOrd + Ord,
{
    type Item = Trapezoid<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let next_line = self.iter.next()?;
        let top_line = std::mem::replace(&mut self.cur_line, next_line);
        let ret = Trapezoid {
            top: top_line,
            bot: self.cur_line.clone(),
        };

        Some(ret)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl<T> Point<T> {
        pub fn new(x: T, y: T) -> Self {
            Point { x, y }
        }
    }

    impl<T> TrapezoidLine<T>
    where
        T: Debug + Eq + Copy + PartialOrd + Ord,
    {
        pub fn new(x_range: RangeInclusive<T>, y: T) -> Self {
            Self { x_range, y }
        }
    }

    impl OwnedPolygon {
        fn new(bb: (u8, u8), points: Vec<Point<u8>>) -> OwnedPolygon {
            let mut data = vec![bb.0, bb.1, 0];
            data.extend(points.as_bytes());
            OwnedPolygon { data }
        }
    }

    #[test]
    fn polygon_new() {
        let poly = OwnedPolygon::new(
            (4, 7),
            vec![
                Point::new(0, 0),
                Point::new(4, 0),
                Point::new(0, 7),
                Point::new(1, 0),
            ],
        );

        assert_eq!(poly.bb().0, 4);
        assert_eq!(poly.bb().1, 7);
        assert_eq!(
            poly.points,
            vec![
                Point::new(0, 0),
                Point::new(4, 0),
                Point::new(0, 7),
                Point::new(1, 0),
            ]
        );
    }

    #[test]
    fn polygon_point() {
        let poly = OwnedPolygon::new(
            (0, 0),
            vec![
                Point::new(2, 3),
                Point::new(2, 3),
                Point::new(2, 3),
                Point::new(2, 3),
            ],
        );
        let expected_lines = vec![TrapezoidLine::new(2..=2, 3), TrapezoidLine::new(2..=2, 3)];

        let lines: Vec<_> = poly.line_iter().collect();
        assert_eq!(lines, expected_lines,);

        let trapezoids: Vec<_> = poly.trapezoid_iter().collect();
        assert_eq!(
            trapezoids,
            vec![Trapezoid {
                top: expected_lines[0].clone(),
                bot: expected_lines[1].clone(),
            }]
        )
    }

    #[test]
    fn polygon_hline() {
        let poly = OwnedPolygon::new(
            (0, 0),
            vec![
                Point::new(7, 3),
                Point::new(7, 3),
                Point::new(2, 3),
                Point::new(2, 3),
            ],
        );
        let expected_lines = vec![TrapezoidLine::new(2..=7, 3), TrapezoidLine::new(2..=7, 3)];

        let lines: Vec<_> = poly.line_iter().collect();
        assert_eq!(lines, expected_lines,);

        let trapezoids: Vec<_> = poly.trapezoid_iter().collect();
        assert_eq!(
            trapezoids,
            vec![Trapezoid {
                top: expected_lines[0].clone(),
                bot: expected_lines[1].clone(),
            }]
        )
    }

    #[test]
    fn polygon_vline() {
        let poly = OwnedPolygon::new(
            (0, 0),
            vec![
                Point::new(3, 3),
                Point::new(3, 10),
                Point::new(3, 10),
                Point::new(3, 3),
            ],
        );
        let expected_lines = vec![TrapezoidLine::new(3..=3, 3), TrapezoidLine::new(3..=3, 10)];

        let lines: Vec<_> = poly.line_iter().collect();
        assert_eq!(lines, expected_lines,);

        let trapezoids: Vec<_> = poly.trapezoid_iter().collect();
        assert_eq!(
            trapezoids,
            vec![Trapezoid {
                top: expected_lines[0].clone(),
                bot: expected_lines[1].clone(),
            }]
        )
    }

    #[test]
    fn polygon_triangle() {
        let poly = OwnedPolygon::new(
            (0, 0),
            vec![
                Point::new(1, 0),
                Point::new(2, 2),
                Point::new(0, 2),
                Point::new(1, 0),
            ],
        );
        let expected_lines = vec![TrapezoidLine::new(1..=1, 0), TrapezoidLine::new(0..=2, 2)];

        let lines: Vec<_> = poly.line_iter().collect();
        assert_eq!(lines, expected_lines);

        let trapezoids: Vec<_> = poly.trapezoid_iter().collect();
        assert_eq!(
            trapezoids,
            vec![Trapezoid {
                top: expected_lines[0].clone(),
                bot: expected_lines[1].clone(),
            }]
        )
    }

    #[test]
    fn polygon_triangle_ccw() {
        let poly = OwnedPolygon::new(
            (0, 0),
            vec![
                Point::new(1, 0),
                Point::new(0, 2),
                Point::new(2, 2),
                Point::new(1, 0),
            ],
        );
        let expected_lines = vec![TrapezoidLine::new(1..=1, 0), TrapezoidLine::new(0..=2, 2)];

        let lines: Vec<_> = poly.line_iter().collect();
        assert_eq!(lines, expected_lines);

        let trapezoids: Vec<_> = poly.trapezoid_iter().collect();
        assert_eq!(
            trapezoids,
            vec![Trapezoid {
                top: expected_lines[0].clone(),
                bot: expected_lines[1].clone(),
            }]
        )
    }

    #[test]
    fn polygon_square() {
        let poly = OwnedPolygon::new(
            (0, 0),
            vec![
                Point::new(2, 0),
                Point::new(2, 2),
                Point::new(0, 2),
                Point::new(0, 0),
            ],
        );
        let expected_lines = vec![TrapezoidLine::new(0..=2, 0), TrapezoidLine::new(0..=2, 2)];

        let lines: Vec<_> = poly.line_iter().collect();
        assert_eq!(lines, expected_lines);

        let trapezoids: Vec<_> = poly.trapezoid_iter().collect();
        assert_eq!(
            trapezoids,
            vec![Trapezoid {
                top: expected_lines[0].clone(),
                bot: expected_lines[1].clone(),
            }]
        )
    }

    #[test]
    fn polygon_hexagon() {
        let poly = OwnedPolygon::new(
            (0, 0),
            vec![
                Point::new(2, 0),
                Point::new(3, 1),
                Point::new(3, 2),
                Point::new(2, 3),
                Point::new(1, 3),
                Point::new(0, 2),
                Point::new(0, 1),
                Point::new(1, 0),
            ],
        );
        let expected_lines = vec![
            TrapezoidLine::new(1..=2, 0),
            TrapezoidLine::new(0..=3, 1),
            TrapezoidLine::new(0..=3, 2),
            TrapezoidLine::new(1..=2, 3),
        ];

        let lines: Vec<_> = poly.line_iter().collect();
        assert_eq!(lines, expected_lines);

        let trapezoids: Vec<_> = poly.trapezoid_iter().collect();
        assert_eq!(
            trapezoids,
            vec![
                Trapezoid {
                    top: expected_lines[0].clone(),
                    bot: expected_lines[1].clone(),
                },
                Trapezoid {
                    top: expected_lines[1].clone(),
                    bot: expected_lines[2].clone(),
                },
                Trapezoid {
                    top: expected_lines[2].clone(),
                    bot: expected_lines[3].clone(),
                },
            ]
        )
    }
}
