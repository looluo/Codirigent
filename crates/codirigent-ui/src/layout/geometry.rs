//! Geometric primitives for the layout system.
//!
//! Provides [`Point`], [`Size`], and [`Bounds`] types used throughout
//! the layout module for positioning and sizing UI elements.

/// A point in 2D space with pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// X coordinate in pixels.
    pub x: f32,
    /// Y coordinate in pixels.
    pub y: f32,
}

impl Point {
    /// Create a new point.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Origin point (0, 0).
    pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

/// A size with width and height in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
}

impl Size {
    /// Create a new size.
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// Zero size.
    pub const fn zero() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
        }
    }
}

/// A rectangular bounds with origin and size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Top-left origin point.
    pub origin: Point,
    /// Size of the bounds.
    pub size: Size,
}

impl Bounds {
    /// Create new bounds from origin and size.
    pub const fn from_origin_size(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    /// Create new bounds from coordinates.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    /// Create bounds at origin with given size.
    pub fn from_size(width: f32, height: f32) -> Self {
        Self::new(0.0, 0.0, width, height)
    }

    /// Get the right edge x coordinate.
    pub fn right(&self) -> f32 {
        self.origin.x + self.size.width
    }

    /// Get the bottom edge y coordinate.
    pub fn bottom(&self) -> f32 {
        self.origin.y + self.size.height
    }

    /// Check if this bounds contains a point.
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.origin.x
            && point.x <= self.right()
            && point.y >= self.origin.y
            && point.y <= self.bottom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Point tests
    #[test]
    fn test_point_new() {
        let p = Point::new(10.0, 20.0);
        assert_eq!(p.x, 10.0);
        assert_eq!(p.y, 20.0);
    }

    #[test]
    fn test_point_zero() {
        let p = Point::zero();
        assert_eq!(p.x, 0.0);
        assert_eq!(p.y, 0.0);
    }

    // Size tests
    #[test]
    fn test_size_new() {
        let s = Size::new(100.0, 200.0);
        assert_eq!(s.width, 100.0);
        assert_eq!(s.height, 200.0);
    }

    #[test]
    fn test_size_zero() {
        let s = Size::zero();
        assert_eq!(s.width, 0.0);
        assert_eq!(s.height, 0.0);
    }

    // Bounds tests
    #[test]
    fn test_bounds_new() {
        let b = Bounds::new(10.0, 20.0, 100.0, 200.0);
        assert_eq!(b.origin.x, 10.0);
        assert_eq!(b.origin.y, 20.0);
        assert_eq!(b.size.width, 100.0);
        assert_eq!(b.size.height, 200.0);
    }

    #[test]
    fn test_bounds_from_size() {
        let b = Bounds::from_size(100.0, 200.0);
        assert_eq!(b.origin.x, 0.0);
        assert_eq!(b.origin.y, 0.0);
        assert_eq!(b.size.width, 100.0);
        assert_eq!(b.size.height, 200.0);
    }

    #[test]
    fn test_bounds_edges() {
        let b = Bounds::new(10.0, 20.0, 100.0, 200.0);
        assert_eq!(b.right(), 110.0);
        assert_eq!(b.bottom(), 220.0);
    }

    #[test]
    fn test_bounds_contains() {
        let b = Bounds::new(10.0, 20.0, 100.0, 200.0);
        assert!(b.contains(Point::new(50.0, 100.0)));
        assert!(b.contains(Point::new(10.0, 20.0)));
        assert!(b.contains(Point::new(110.0, 220.0)));
        assert!(!b.contains(Point::new(5.0, 100.0)));
        assert!(!b.contains(Point::new(50.0, 250.0)));
    }
}
