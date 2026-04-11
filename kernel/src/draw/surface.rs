use core::alloc::Layout;

use x86_64::VirtAddr;

use crate::{
    draw::geometry::{Color, Point, Rect},
    memory::{kernel_alloc, kernel_free},
};

pub struct Surface {
    image: *mut Color,
    rect: Rect,
    stride: usize,
    fill: Color,
}

unsafe impl Send for Surface {}

impl Surface {
    pub fn new(rect: Rect, fill: Color) -> Surface {
        let (layout, _) = Layout::new::<Color>()
            .repeat(rect.width() as usize * rect.height() as usize)
            .unwrap();
        let stride = rect.width() as usize;
        let image = kernel_alloc(layout).unwrap().as_mut_ptr();
        let mut surface = Surface {
            image,
            rect,
            stride,
            fill,
        };
        unsafe { surface.fill_unsafe(rect, fill) };
        surface
    }
    // SAFETY: image must point to suitably sized area of memory, Surface takes ownership of that memory
    pub unsafe fn from_raw(image: *mut Color, rect: Rect, stride: usize, fill: Color) -> Surface {
        Surface {
            image,
            rect,
            stride,
            fill,
        }
    }
    pub fn rect(&self) -> Rect {
        self.rect
    }
    pub unsafe fn image_ptr_unsafe(&self, p: Point) -> *const Color {
        unsafe {
            self.image.add(
                (p.y - self.rect.tl.y) as usize * self.stride + (p.x - self.rect.tl.x) as usize,
            )
        }
    }
    pub unsafe fn image_slice_unsafe(&self, p: Point, width: i32) -> &[Color] {
        unsafe { core::slice::from_raw_parts(self.image_ptr_unsafe(p), width as usize) }
    }
    pub unsafe fn image_ptr_mut_unsafe(&mut self, p: Point) -> *mut Color {
        unsafe {
            self.image.add(
                (p.y - self.rect.tl.y) as usize * self.stride + (p.x - self.rect.tl.x) as usize,
            )
        }
    }
    pub unsafe fn image_slice_mut_unsafe(&mut self, p: Point, width: i32) -> &mut [Color] {
        unsafe { core::slice::from_raw_parts_mut(self.image_ptr_mut_unsafe(p), width as usize) }
    }
    // SAFETY: dst_rect is entirely within self.rect
    pub unsafe fn fill_unsafe(&mut self, dst_rect: Rect, color: Color) {
        unsafe {
            for y in dst_rect.tl.y..dst_rect.br.y {
                self.image_slice_mut_unsafe(Point::new(dst_rect.tl.x, y), dst_rect.width())
                    .fill(color);
            }
        }
    }
    pub fn fill(&mut self, dst_rect: Rect, color: Color) {
        if let Some(rect) = dst_rect.intersect(self.rect) {
            unsafe { self.fill_unsafe(rect, color) };
        }
    }
    // SAFETY: dst rectangle is entirely within self.rect, src rectangle is entirely within src.rect, width, height >= 0
    pub unsafe fn blit_unsafe(
        &mut self,
        dst_start: Point,
        src: &Surface,
        src_start: Point,
        width: i32,
        height: i32,
    ) {
        unsafe {
            if self.image == src.image && dst_start.y > src_start.y {
                for y in (0..height).rev() {
                    let dst_row =
                        self.image_ptr_mut_unsafe(Point::new(dst_start.x, dst_start.y + y));
                    let src_row = src.image_ptr_unsafe(Point::new(src_start.x, src_start.y + y));
                    core::ptr::copy(src_row, dst_row, width as usize);
                }
            } else {
                for y in 0..height {
                    let dst_row =
                        self.image_ptr_mut_unsafe(Point::new(dst_start.x, dst_start.y + y));
                    let src_row = src.image_ptr_unsafe(Point::new(src_start.x, src_start.y + y));
                    core::ptr::copy(src_row, dst_row, width as usize);
                }
            }
        }
    }
    pub fn blit(&mut self, mut dst_rect: Rect, src: &Surface, mut src_start: Point) {
        let Some(clamped_dst_rect) = dst_rect.intersect(self.rect) else {
            return;
        };
        src_start += clamped_dst_rect.tl - dst_rect.tl;
        dst_rect = clamped_dst_rect;
        let Some(blit_src) = Rect {
            tl: src_start,
            br: src_start + dst_rect.size(),
        }
        .intersect(src.rect) else {
            unsafe {
                self.fill_unsafe(dst_rect, src.fill);
            };
            return;
        };
        let blit_dst_tl = dst_rect.tl + (blit_src.tl - src_start);
        let blit_dst_br = blit_dst_tl + blit_src.size();
        unsafe {
            self.blit_unsafe(
                blit_dst_tl,
                src,
                blit_src.tl,
                blit_src.width(),
                blit_src.height(),
            )
        };
        if dst_rect.tl.y < blit_dst_tl.y {
            unsafe {
                self.fill_unsafe(
                    Rect::new(dst_rect.tl.x, dst_rect.tl.y, dst_rect.br.x, blit_dst_tl.y),
                    src.fill,
                )
            };
        }
        if dst_rect.tl.x < blit_dst_tl.x {
            unsafe {
                self.fill_unsafe(
                    Rect::new(dst_rect.tl.x, blit_dst_tl.y, blit_dst_tl.x, blit_dst_br.y),
                    src.fill,
                )
            };
        }
        if blit_dst_br.x < dst_rect.br.x {
            unsafe {
                self.fill_unsafe(
                    Rect::new(blit_dst_br.x, blit_dst_tl.y, dst_rect.br.x, blit_dst_br.y),
                    src.fill,
                )
            };
        }
        if blit_dst_br.y < dst_rect.br.y {
            unsafe {
                self.fill_unsafe(
                    Rect::new(dst_rect.tl.x, blit_dst_br.y, dst_rect.br.x, dst_rect.br.y),
                    src.fill,
                )
            };
        }
    }
    pub fn self_blit(&mut self, dst_rect: Rect, src_start: Point) {
        // don't do this at home kids :)
        let src = Surface {
            image: self.image,
            rect: self.rect,
            stride: self.stride,
            fill: self.fill,
        };
        self.blit(dst_rect, &src, src_start);
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { kernel_free(VirtAddr::from_ptr(self.image)) };
    }
}
