use core::cmp::{max, min};

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0xff,
    };
    pub const WHITE: Color = Color {
        r: 0xff,
        g: 0xff,
        b: 0xff,
        a: 0xff,
    };
    pub const RED: Color = Color {
        r: 0xff,
        g: 0,
        b: 0,
        a: 0xff,
    };
    pub const GREEN: Color = Color {
        r: 0,
        g: 0xff,
        b: 0,
        a: 0xff,
    };
    pub const BLUE: Color = Color {
        r: 0,
        g: 0,
        b: 0xff,
        a: 0xff,
    };
}

#[derive(Copy, Clone, Debug)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Copy, Clone, Debug)]
pub struct Rect {
    pub tl: Point,
    pub br: Point,
}

impl Point {
    pub const ZERO: Point = Point { x: 0, y: 0 };
    pub fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }
}

impl core::ops::Add<Point> for Point {
    type Output = Point;

    fn add(self, rhs: Self) -> Self::Output {
        Point {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl core::ops::Sub<Point> for Point {
    type Output = Point;

    fn sub(self, rhs: Self) -> Self::Output {
        Point {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl core::ops::AddAssign<Point> for Point {
    fn add_assign(&mut self, rhs: Point) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl core::ops::SubAssign<Point> for Point {
    fn sub_assign(&mut self, rhs: Point) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Rect {
    pub const EMPTY: Rect = Rect {
        tl: Point::ZERO,
        br: Point::ZERO,
    };
    pub fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Rect {
        Rect {
            tl: Point { x: x0, y: y0 },
            br: Point { x: x1, y: y1 },
        }
    }
    pub fn width(&self) -> i32 {
        self.br.x - self.tl.x
    }
    pub fn height(&self) -> i32 {
        self.br.y - self.tl.y
    }
    pub fn size(&self) -> Point {
        Point::new(self.width(), self.height())
    }
    pub fn contains_point(&self, pt: Point) -> bool {
        self.tl.x <= pt.x && pt.x < self.br.x && self.tl.y <= pt.y && pt.y < self.br.y
    }
    pub fn is_empty(&self) -> bool {
        self.br.x <= self.tl.x || self.br.y <= self.tl.y
    }
    pub fn intersect(self, boundary: Rect) -> Option<Rect> {
        let tl_x = self.tl.x.max(boundary.tl.x);
        let tl_y = self.tl.y.max(boundary.tl.y);
        let br_x = self.br.x.min(boundary.br.x);
        let br_y = self.br.y.min(boundary.br.y);
        (tl_x < br_x && tl_y < br_y).then_some(Rect::new(tl_x, tl_y, br_x, br_y))
    }
    pub fn merge(self, other: Rect) -> Rect {
        if self.is_empty() {
            other
        } else if other.is_empty() {
            self
        } else {
            Rect::new(
                min(self.tl.x, other.tl.x),
                min(self.tl.y, other.tl.y),
                max(self.br.x, other.br.x),
                max(self.br.y, other.br.y),
            )
        }
    }
}