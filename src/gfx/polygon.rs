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
use super::slope;
use super::Point;

use std::cmp::Ordering;
use std::default::Default;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt::Result;
use std::ops::Add;
use std::ops::Div;
use std::ops::Mul;
use std::ops::Sub;
use std::slice::Iter;

#[derive(Clone)]
pub struct Polygon {
    pub bbw: u8,
    pub bbh: u8,
    pub points: Vec<Point<u8>>,
}

impl Debug for Polygon {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "({},{}) [{:?}]", self.bbw, self.bbh, self.points)
    }
}

impl Polygon {
    pub fn new(bb: (u8, u8), points: Vec<Point<u8>>) -> Polygon {
        Polygon {
            bbw: bb.0,
            bbh: bb.1,
            points,
        }
    }

    #[allow(dead_code)]
    pub fn line_iter<T>(&self) -> PolygonIter<u8, T>
    where
        T: Copy + Default + PartialEq + PartialOrd,
        T: Add<T, Output = T> + Sub<T, Output = T> + Mul<T, Output = T> + Div<T, Output = T>,
        Point<T>: From<Point<u8>>,
    {
        PolygonIter::<_, T>::new(self.points.iter())
    }
}

/// An iterator that returns all the horizontal lines from which we can infer the shape of the
/// polygon, from top to bottom.
///
/// These lines can be connected together in order to produce a set of quads that fills the polygon.
///
/// `U` is the original type of the points in the polygon, whereas `T` is the type on which we want
/// to perform the operations. It can be different if e.g. `U` is an integer type, in which case we
/// will likely want `T` to be some kind of floating point in order to get good precision.
///
// TODO: Windows type implements DoubleEndedIterator! We can use that. Maybe
//       we need to reverse the back iterator to get the correct lines though.
pub struct PolygonIter<'a, U, T> {
    iter: Iter<'a, Point<U>>,
    // Next point when going clockwise.
    next_cw: Option<&'a Point<U>>,
    // Next point when going counter-clockwise.
    next_ccw: Option<&'a Point<U>>,
    // Line to return on the next call to next().
    next_line: Option<(Point<T>, Point<T>)>,
}

impl<'a, U, T> PolygonIter<'a, U, T>
where
    T: Copy + Default + PartialEq + PartialOrd,
    T: Add<T, Output = T> + Sub<T, Output = T> + Mul<T, Output = T> + Div<T, Output = T>,
    Point<T>: From<Point<U>>,
    U: Copy,
{
    pub fn new(mut iter: Iter<'a, Point<U>>) -> Self {
        let next_line = match (iter.next(), iter.next_back()) {
            (Some(cw), Some(ccw)) => Some((Point::<T>::from(*ccw), Point::<T>::from(*cw))),
            (Some(cw), None) => Some((Point::<T>::from(*cw), Point::<T>::from(*cw))),
            _ => None,
        };

        PolygonIter::<_, T> {
            next_cw: iter.next(),
            next_ccw: iter.next_back(),
            iter,
            next_line,
        }
    }
}

impl<'a, U, T> Iterator for PolygonIter<'a, U, T>
where
    T: Copy + Default + PartialEq + PartialOrd,
    T: Add<T, Output = T> + Sub<T, Output = T> + Mul<T, Output = T> + Div<T, Output = T>,
    Point<T>: From<Point<U>>,
    U: Copy,
{
    type Item = (Point<T>, Point<T>);

    fn next(&mut self) -> Option<Self::Item> {
        let (cur_line, p1, p2) = match (self.next_ccw, self.next_cw, self.next_line) {
            (Some(ccw), Some(cw), Some((p1, p2))) => {
                ((p1, p2), Point::<T>::from(*ccw), Point::<T>::from(*cw))
            }
            // We parsed all the points.
            _ => return self.next_line.take(),
        };

        self.next_line = match p1.y.partial_cmp(&p2.y) {
            // Create a new point on the line connecting p2 to its next point.
            Some(Ordering::Less) => {
                self.next_ccw = self.iter.next_back();
                let x =
                    p2.x - ((p1.y - cur_line.1.y) * slope(&cur_line.1, &p2).unwrap_or_default());
                Some((p1, Point::new(x, p1.y)))
            }
            // Create a new point on the line connecting p1 to its next point.
            Some(Ordering::Greater) => {
                self.next_cw = self.iter.next();
                let x =
                    p1.x - ((p2.y - cur_line.0.y) * slope(&cur_line.0, &p1).unwrap_or_default());
                Some((Point::new(x, p2.y), p2))
            }
            // Point share same y, return the line connecting them.
            Some(Ordering::Equal) => {
                self.next_cw = self.iter.next();
                self.next_ccw = self.iter.next_back();
                Some((p1, p2))
            }
            // We are in NaN territory, so something must have gone pretty wrong. Let's stop here.
            None => None,
        };

        // We may return the same line twice in a row in case of a point or horizontal line.
        if Some(cur_line) == self.next_line {
            self.next()
        } else {
            Some(cur_line)
        }
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
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new(
            (0, 0),
            vec![
                Point::new(2, 3),
                Point::new(2, 3),
                Point::new(2, 3),
                Point::new(2, 3),
            ],
        )
        .line_iter()
        .collect();

        assert_eq!(lines, vec![(Point::new(2.0, 3.0), Point::new(2.0, 3.0))]);
    }

    #[test]
    fn iterator_hline() {
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new(
            (0, 0),
            vec![
                Point::new(7, 3),
                Point::new(7, 3),
                Point::new(2, 3),
                Point::new(2, 3),
            ],
        )
        .line_iter()
        .collect();

        assert_eq!(lines, vec![(Point::new(2.0, 3.0), Point::new(7.0, 3.0))]);
    }

    #[test]
    fn iterator_vline() {
        let lines: Vec<(Point<f64>, Point<f64>)> = Polygon::new(
            (0, 0),
            vec![
                Point::new(3, 3),
                Point::new(3, 10),
                Point::new(3, 10),
                Point::new(3, 3),
            ],
        )
        .line_iter()
        .collect();

        assert_eq!(
            lines,
            vec![
                (Point::new(3.0, 3.0), Point::new(3.0, 3.0)),
                (Point::new(3.0, 10.0), Point::new(3.0, 10.0))
            ]
        );
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
                (Point::new(0.0, 2.0), Point::new(2.0, 2.0)),
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
                (Point::new(0.0, 0.0), Point::new(2.0, 0.0)),
                (Point::new(0.0, 2.0), Point::new(2.0, 2.0)),
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
                (Point::new(1.0, 0.0), Point::new(2.0, 0.0)),
                (Point::new(0.0, 1.0), Point::new(3.0, 1.0)),
                (Point::new(0.0, 2.0), Point::new(3.0, 2.0)),
                (Point::new(1.0, 3.0), Point::new(2.0, 3.0)),
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
                (Point::new(0.0, 0.0), Point::new(2.0, 0.0)),
                (Point::new(0.0, 1.0), Point::new(3.0, 1.0)),
                (Point::new(0.0, 2.0), Point::new(2.0, 2.0)),
                (Point::new(0.0, 3.0), Point::new(3.0, 3.0)),
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
                (Point::new(0.0, 0.0), Point::new(2.0, 0.0)),
                (Point::new(0.0, 1.0), Point::new(2.0, 1.0)),
                (Point::new(1.0, 2.0), Point::new(2.0, 2.0)),
                (Point::new(0.0, 3.0), Point::new(2.0, 3.0)),
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
                (Point::new(1.0, 0.0), Point::new(2.0, 0.0)),
                (Point::new(0.5, 1.0), Point::new(3.0, 1.0)),
                (Point::new(0.0, 2.0), Point::new(2.0, 2.0)),
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
                (Point::new(0.0, 0.0), Point::new(1.0, 0.0)),
                (Point::new(1.0, 1.0), Point::new(1.5, 1.0)),
                (Point::new(0.0, 2.0), Point::new(2.0, 2.0)),
            ]
        );
    }
}
