//! Utilities to manipulate polygons.
//!
//! The vertices of the polygons are given in a particular order: always
//! clockwise, starting with the upper-right vertex. The last vertex, which is
//! always the top-left one, is also guaranteed to have the same `y` as the
//! first one. Similarly, there are two consecutive points with the same `y` at
//! the bottom of the polygon (identical if need be). These properties are
//! useful to raster the polygons quickly.
//!
//! They are also useful when using modern rendering APIs that do not support
//! concave polygons: by parsing the list of polygons from both ends, we can
//! generate intermediate points and quads with horizontal top and botton lines.
//! These quads are guaranteed to be convex.
use super::{slope, Point};

use std::cmp::Ordering;
use std::default::Default;
use std::fmt::{Debug, Formatter, Result};
use std::ops::{Add, Div, Mul, Sub};
use std::slice::Iter;

pub struct Polygon {
    pub bbw: u16,
    pub bbh: u16,
    pub points: Vec<Point<u16>>,
}

impl Debug for Polygon {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "({},{}) [{:?}]", self.bbw, self.bbh, self.points)
    }
}

impl Polygon {
    pub fn new(bb: (u16, u16), points: Vec<Point<u16>>) -> Polygon {
        Polygon {
            bbw: bb.0,
            bbh: bb.1,
            points,
        }
    }

    pub fn line_iter<T>(&self) -> PolygonIter<T>
    where
        Point<T>: From<Point<u16>>,
    {
        let mut iter = self.points.iter();
        let next_line = match (iter.next(), iter.next_back()) {
            (Some(cw), Some(ccw)) => Some((Point::<T>::from(*cw), Point::<T>::from(*ccw))),
            (Some(cw), None) => Some((Point::<T>::from(*cw), Point::<T>::from(*cw))),
            _ => None,
        };

        PolygonIter::<T> {
            next_cw: iter.next(),
            next_ccw: iter.next_back(),
            iter,
            next_line,
            phantom: std::marker::PhantomData,
        }
    }
}

/// An iterator that returns all the horizontal lines from which we can infer
/// the shape of the polygon, from top to bottom.
/// These lines can be connected together in order to produce a set of quads
/// that fills the polygon.
// TODO: Windows type implements DoubleEndedIterator! We can use that. Maybe
//       we need to reverse the back iterator to get the correct lines though.
pub struct PolygonIter<'a, T> {
    iter: Iter<'a, Point<u16>>,
    next_cw: Option<&'a Point<u16>>,
    next_ccw: Option<&'a Point<u16>>,
    // The line to return on the next call to next().
    next_line: Option<(Point<T>, Point<T>)>,

    phantom: std::marker::PhantomData<T>,
}

impl<'a, T> Iterator for PolygonIter<'a, T>
where
    T: Copy + Default + PartialEq,
    T: Add<T, Output = T> + Sub<T, Output = T> + Mul<T, Output = T> + Div<T, Output = T>,
    Point<T>: From<Point<u16>>,
{
    type Item = (Point<T>, Point<T>);

    fn next(&mut self) -> Option<Self::Item> {
        let (p1, p2, cur_line, p1_1, p2_1) = match (self.next_cw, self.next_ccw, self.next_line) {
            (Some(cw), Some(ccw), Some((p1, p2))) => (
                cw,
                ccw,
                (p1, p2),
                Point::<T>::from(*cw),
                Point::<T>::from(*ccw),
            ),
            // We parsed all the points.
            _ => return self.next_line.take(),
        };

        self.next_line = match p1.y.cmp(&p2.y) {
            // Create a new point on the line connecting p2 to its next point.
            Ordering::Less => {
                self.next_cw = self.iter.next();
                let x = p2_1.x
                    - ((p1_1.y - cur_line.1.y) * slope(&cur_line.1, &p2_1).unwrap_or_default());
                Some((p1_1, Point::new(x, p1_1.y)))
            }
            // Create a new point on the line connecting p1 to its next point.
            Ordering::Greater => {
                self.next_ccw = self.iter.next_back();
                let x = p1_1.x
                    - ((p2_1.y - cur_line.0.y) * slope(&cur_line.0, &p1_1).unwrap_or_default());
                Some((Point::new(x, p2_1.y), p2_1))
            }
            // Point share same y, return the line connecting them.
            Ordering::Equal => {
                self.next_cw = self.iter.next();
                self.next_ccw = self.iter.next_back();
                Some((p1_1, p2_1))
            }
        };

        Some(cur_line)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn polygon_new() {
        let poly = Polygon::new(
            (4, 7),
            vec![
                Point::new(0, 0),
                Point::new(4, 0),
                Point::new(0, 7),
                Point::new(1, 0),
            ],
        );

        assert_eq!(poly.bbw, 4);
        assert_eq!(poly.bbh, 7);
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
    fn iterator_point() {
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new((0, 0), vec![Point::new(2, 3)])
            .line_iter()
            .collect();

        assert_eq!(lines, vec![(Point::new(2.0, 3.0), Point::new(2.0, 3.0)),]);
    }

    #[test]
    fn iterator_line() {
        let lines: Vec<(Point<f64>, Point<f64>)> =
            Polygon::new((0, 0), vec![Point::new(2, 3), Point::new(7, 3)])
                .line_iter()
                .collect();

        assert_eq!(lines, vec![(Point::new(2.0, 3.0), Point::new(7.0, 3.0)),]);
    }

    #[test]
    fn iterator_triangle() {
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new(
            (0, 0),
            vec![
                Point::new(1, 0),
                Point::new(2, 2),
                Point::new(0, 2),
                Point::new(1, 0),
            ],
        )
        .line_iter()
        .collect();

        assert_eq!(
            lines,
            vec![
                (Point::new(1.0, 0.0), Point::new(1.0, 0.0)),
                (Point::new(2.0, 2.0), Point::new(0.0, 2.0)),
            ]
        );
    }

    #[test]
    fn iterator_square() {
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new(
            (0, 0),
            vec![
                Point::new(2, 0),
                Point::new(2, 2),
                Point::new(0, 2),
                Point::new(0, 0),
            ],
        )
        .line_iter()
        .collect();

        assert_eq!(
            lines,
            vec![
                (Point::new(2.0, 0.0), Point::new(0.0, 0.0)),
                (Point::new(2.0, 2.0), Point::new(0.0, 2.0)),
            ]
        );
    }

    #[test]
    fn iterator_hexagon() {
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new(
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
        )
        .line_iter()
        .collect();

        assert_eq!(
            lines,
            vec![
                (Point::new(2.0, 0.0), Point::new(1.0, 0.0)),
                (Point::new(3.0, 1.0), Point::new(0.0, 1.0)),
                (Point::new(3.0, 2.0), Point::new(0.0, 2.0)),
                (Point::new(2.0, 3.0), Point::new(1.0, 3.0)),
            ]
        );
    }

    #[test]
    fn iterator_unbalanced_poly() {
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new(
            (0, 0),
            vec![
                Point::new(2, 0),
                Point::new(3, 1),
                Point::new(2, 2),
                Point::new(3, 3),
                Point::new(0, 3),
                Point::new(0, 0),
            ],
        )
        .line_iter()
        .collect();

        assert_eq!(
            lines,
            vec![
                (Point::new(2.0, 0.0), Point::new(0.0, 0.0)),
                (Point::new(3.0, 1.0), Point::new(0.0, 1.0)),
                (Point::new(2.0, 2.0), Point::new(0.0, 2.0)),
                (Point::new(3.0, 3.0), Point::new(0.0, 3.0)),
            ]
        );
    }

    #[test]
    fn iterator_unbalanced_poly_rev() {
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new(
            (0, 0),
            vec![
                Point::new(2, 0),
                Point::new(2, 3),
                Point::new(0, 3),
                Point::new(1, 2),
                Point::new(0, 1),
                Point::new(0, 0),
            ],
        )
        .line_iter()
        .collect();

        assert_eq!(
            lines,
            vec![
                (Point::new(2.0, 0.0), Point::new(0.0, 0.0)),
                (Point::new(2.0, 1.0), Point::new(0.0, 1.0)),
                (Point::new(2.0, 2.0), Point::new(1.0, 2.0)),
                (Point::new(2.0, 3.0), Point::new(0.0, 3.0)),
            ]
        );
    }

    #[test]
    fn iterator_unbalanced_poly_slope() {
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new(
            (0, 0),
            vec![
                Point::new(2, 0),
                Point::new(3, 1),
                Point::new(2, 2),
                Point::new(0, 2),
                Point::new(1, 0),
            ],
        )
        .line_iter()
        .collect();

        assert_eq!(
            lines,
            vec![
                (Point::new(2.0, 0.0), Point::new(1.0, 0.0)),
                (Point::new(3.0, 1.0), Point::new(0.5, 1.0)),
                (Point::new(2.0, 2.0), Point::new(0.0, 2.0)),
            ]
        );
    }

    #[test]
    fn iterator_unbalanced_poly_slope_rev() {
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new(
            (0, 0),
            vec![
                Point::new(1, 0),
                Point::new(2, 2),
                Point::new(0, 2),
                Point::new(1, 1),
                Point::new(0, 0),
            ],
        )
        .line_iter()
        .collect();

        assert_eq!(
            lines,
            vec![
                (Point::new(1.0, 0.0), Point::new(0.0, 0.0)),
                (Point::new(1.5, 1.0), Point::new(1.0, 1.0)),
                (Point::new(2.0, 2.0), Point::new(0.0, 2.0)),
            ]
        );
    }
}
