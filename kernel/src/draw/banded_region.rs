use core::cmp::{Ordering, max, min};

use crate::{
    draw::geometry::{Point, Rect},
    memory::rbtree::{OwnedRbNode, RbNodeRef, RbTree},
    println,
};

#[derive(Clone, PartialEq, Eq)]
struct Region<V> {
    x_start: i32,
    x_end: i32,
    value: V,
}

#[derive(Clone)]
struct Band<V> {
    y_start: i32,
    y_end: i32,
    regions: RbTree<Region<V>, ()>,
}

#[derive(Clone)]
pub struct BandedRegion<V> {
    bands: RbTree<Band<V>, ()>,
}

impl<V> BandedRegion<V> {
    pub fn new() -> Self {
        BandedRegion {
            bands: RbTree::new(),
        }
    }
}

impl<V> Region<V> {
    fn new(x_start: i32, x_end: i32, value: V) -> Self {
        Region {
            x_start,
            x_end,
            value,
        }
    }
    fn cmp(a: &Self, b: &Self) -> Ordering {
        a.x_start.cmp(&b.x_start)
    }
}

impl<V> Band<V> {
    fn cmp(a: &Self, b: &Self) -> Ordering {
        a.y_start.cmp(&b.y_start)
    }
    fn new(y_start: i32, y_end: i32) -> Self {
        Band {
            y_start,
            y_end,
            regions: RbTree::new(),
        }
    }
}

impl<V: Clone> Band<V> {
    fn update(
        &mut self,
        mut x_start: i32,
        x_end: i32,
        update_fn: impl Fn(Option<&V>) -> Option<V>,
    ) {
        // TODO: missing coalescing
        while let Some(node) = self
            .regions
            .lower_bound_mut(|region, _| x_start < region.x_end)
        {
            if x_end <= node.value().x_start {
                break;
            }
            let region = node.remove().into_value();
            if x_start < region.x_start {
                if let Some(value) = update_fn(None) {
                    let new_region = Region::new(x_start, region.x_start, value);
                    self.regions
                        .insert(OwnedRbNode::new(new_region), Region::cmp);
                }
                x_start = region.x_start;
            } else if x_start > region.x_start {
                let new_region = Region::new(region.x_start, x_start, region.value.clone());
                self.regions
                    .insert(OwnedRbNode::new(new_region), Region::cmp);
            }
            if let Some(value) = update_fn(Some(&region.value)) {
                let new_region = Region::new(x_start, x_end.min(region.x_end), value);
                self.regions
                    .insert(OwnedRbNode::new(new_region), Region::cmp);
            }
            if x_end < region.x_end {
                let new_region = Region::new(x_end, region.x_end, region.value.clone());
                self.regions
                    .insert(OwnedRbNode::new(new_region), Region::cmp);
            }
            x_start = region.x_end;
        }
        if x_start < x_end {
            if let Some(value) = update_fn(None) {
                let new_region = Region::new(x_start, x_end, value);
                self.regions
                    .insert(OwnedRbNode::new(new_region), Region::cmp);
            }
        }
    }
}

impl<V: Clone + Eq> Band<V> {
    fn coalesce(&mut self) {
        unsafe {
            self.regions.pairwise_merge(|left, right| {
                if left.x_end == right.x_start && left.value == right.value {
                    left.x_end = right.x_end;
                    true
                } else {
                    false
                }
            })
        }
    }
}

impl<V: Clone> BandedRegion<V> {
    pub fn update(&mut self, rect: Rect, update_fn: impl Fn(Option<&V>) -> Option<V>) {
        let mut y_start = rect.tl.y;
        let y_end = rect.br.y;
        // TODO: missing coalescing
        while let Some(node) = self.bands.lower_bound_mut(|band, _| y_start < band.y_end) {
            if y_end <= node.value().y_start {
                break;
            }
            // we are being inefficient here: could reuse node and the band
            let band = node.remove().into_value();
            if y_start < band.y_start {
                let mut new_band = Band::new(y_start, band.y_start);
                new_band.update(rect.tl.x, rect.br.x, &update_fn);
                if !new_band.regions.is_empty() {
                    self.bands.insert(OwnedRbNode::new(new_band), Band::cmp);
                }
                y_start = band.y_start;
            } else if y_start > band.y_start {
                let mut new_band = band.clone();
                new_band.y_end = y_start;
                self.bands.insert(OwnedRbNode::new(new_band), Band::cmp);
            }
            let mut new_band = band.clone();
            new_band.y_start = y_start;
            new_band.y_end = y_end.min(band.y_end);
            new_band.update(rect.tl.x, rect.br.x, &update_fn);
            if !new_band.regions.is_empty() {
                self.bands.insert(OwnedRbNode::new(new_band), Band::cmp);
            }
            if y_end < band.y_end {
                let mut new_band = band.clone();
                new_band.y_start = y_end;
                self.bands.insert(OwnedRbNode::new(new_band), Band::cmp);
            }
            y_start = band.y_end;
        }
        if y_start < y_end {
            let mut new_band = Band::new(y_start, y_end);
            new_band.update(rect.tl.x, rect.br.x, &update_fn);
            self.bands.insert(OwnedRbNode::new(new_band), Band::cmp);
        }
    }
    pub fn insert(&mut self, rect: Rect, value: V) {
        self.update(rect, |_| Some(value.clone()));
    }
    pub fn remove(&mut self, rect: Rect) {
        self.update(rect, |_| None);
    }
    pub fn query(&self, boundary: Rect) -> BandedRegionIterator<'_, V> {
        BandedRegionIterator {
            banded_region: self,
            boundary,
            cursor: boundary.tl,
        }
    }
}

impl<V: Clone + Eq> BandedRegion<V> {
    pub fn drain(&mut self, boundary: Rect) -> BandedRegionDrainingIterator<'_, V> {
        BandedRegionDrainingIterator {
            banded_region: self,
            boundary,
            cursor: boundary.tl,
        }
    }
}

impl<V: Clone + Eq> BandedRegion<V> {
    pub fn coalesce(&mut self) {
        unsafe {
            self.bands.update_values(|band| band.coalesce());
            self.bands.pairwise_merge(|left, right| {
                if left.y_end == right.y_start && left.regions == right.regions {
                    left.y_end = right.y_end;
                    true
                } else {
                    false
                }
            });
        }
    }
}

impl<V> BandedRegion<V> {
    fn first_band_in_range(&self, y_start: i32, y_end: i32) -> Option<RbNodeRef<'_, Band<V>, ()>> {
        if y_start >= y_end {
            None
        } else {
            self.bands
                .lower_bound(|band, _| y_start < band.y_end)
                .filter(|node| node.value().y_start < y_end)
        }
    }
}

impl<V> Band<V> {
    fn first_region_in_range(
        &self,
        x_start: i32,
        x_end: i32,
    ) -> Option<RbNodeRef<'_, Region<V>, ()>> {
        if x_start >= x_end {
            None
        } else {
            self.regions
                .lower_bound(|region, _| x_start < region.x_end)
                .filter(|node| node.value().x_start < x_end)
        }
    }
    fn for_all_in_range(
        &self,
        mut x_start: i32,
        x_end: i32,
        mut fun: impl FnMut(i32, i32, Option<&V>) -> bool,
    ) {
        while let Some(node) = self.first_region_in_range(x_start, x_end) {
            let region = node.value();
            if region.x_start > x_start {
                if !fun(x_start, region.x_start, None) {
                    return;
                }
            }
            if !fun(region.x_start, region.x_end, Some(&region.value)) {
                return;
            }
            x_start = min(x_end, region.x_end);
        }
        if x_start < x_end {
            fun(x_start, x_end, None);
        }
    }
    fn all_in_range(
        &self,
        x_start: i32,
        x_end: i32,
        fun: impl Fn(i32, i32, Option<&V>) -> bool,
    ) -> bool {
        let mut return_value = true;
        self.for_all_in_range(x_start, x_end, |x0, x1, value| {
            if fun(x0, x1, value) {
                true
            } else {
                return_value = false;
                false
            }
        });
        return_value
    }
}

impl<V: Eq> BandedRegion<V> {
    fn find_next_rect(
        &self,
        boundary: Rect,
        cursor: &mut Point,
    ) -> Option<(
        RbNodeRef<'_, Band<V>, ()>,
        RbNodeRef<'_, Region<V>, ()>,
        Rect,
    )> {
        loop {
            let Some(band_node) = self.first_band_in_range(cursor.y, boundary.br.y) else {
                cursor.y = boundary.br.y;
                return None;
            };
            let band = band_node.value();
            let Some(region_node) = band.first_region_in_range(cursor.x, boundary.br.x) else {
                cursor.x = boundary.tl.x;
                cursor.y = band.y_end;
                continue;
            };
            let region = region_node.value();
            let rect = Rect::new(region.x_start, band.y_start, region.x_end, band.y_end)
                .intersect(Rect {
                    tl: *cursor,
                    br: boundary.br,
                })
                .unwrap();
            return Some((band_node, region_node, rect));
        }
    }
    fn rect_grow_x(
        &self,
        boundary: Rect,
        mut region_node: RbNodeRef<Region<V>, ()>,
        rect: &mut Rect,
    ) {
        while rect.br.x < boundary.br.x {
            let Some(succ) = region_node.successor() else {
                break;
            };
            if succ.value().x_start != region_node.value().x_end
                || succ.value().value != region_node.value().value
            {
                break;
            }
            rect.br.x = min(boundary.br.x, succ.value().x_end);
            region_node = succ;
        }
    }
    fn rect_grow_y(
        &self,
        boundary: Rect,
        mut band_node: RbNodeRef<Band<V>, ()>,
        rect: &mut Rect,
        rect_value: &V,
        is_draining: bool,
    ) -> i32 {
        while rect.br.y < boundary.br.y {
            let Some(succ) = band_node.successor() else {
                break;
            };
            if succ.value().y_start != band_node.value().y_end
                || !is_draining
                    && band_node
                        .value()
                        .first_region_in_range(rect.br.x, boundary.br.x)
                        .is_some()
                || !is_draining
                    && succ
                        .value()
                        .first_region_in_range(boundary.tl.x, rect.tl.x)
                        .is_some()
                || !succ
                    .value()
                    .all_in_range(rect.tl.x, rect.br.x, |_, _, value| {
                        value == Some(rect_value)
                    })
            {
                break;
            }
            rect.br.y = min(boundary.br.y, succ.value().y_end);
            band_node = succ;
        }
        band_node.value().y_start
    }
}

pub struct BandedRegionIterator<'a, V> {
    banded_region: &'a BandedRegion<V>,
    boundary: Rect,
    cursor: Point,
}

impl<'a, V: Eq> Iterator for BandedRegionIterator<'a, V> {
    type Item = (Rect, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let (band_node, region_node, mut rect) = self
            .banded_region
            .find_next_rect(self.boundary, &mut self.cursor)?;
        let value = &region_node.value().value;
        self.banded_region
            .rect_grow_x(self.boundary, region_node, &mut rect);
        self.cursor.y =
            self.banded_region
                .rect_grow_y(self.boundary, band_node, &mut rect, value, false);
        self.cursor.x = rect.br.x;
        return Some((rect, value));
    }
}

pub struct BandedRegionDrainingIterator<'a, V: Clone + Eq> {
    banded_region: &'a mut BandedRegion<V>,
    boundary: Rect,
    cursor: Point,
}

impl<'a, V: Clone + Eq> Iterator for BandedRegionDrainingIterator<'a, V> {
    type Item = (Rect, V);

    fn next(&mut self) -> Option<Self::Item> {
        let (band_node, region_node, mut rect) = self
            .banded_region
            .find_next_rect(self.boundary, &mut self.cursor)?;
        let value = region_node.value().value.clone();
        self.banded_region
            .rect_grow_x(self.boundary, region_node, &mut rect);
        self.banded_region
            .rect_grow_y(self.boundary, band_node, &mut rect, &value, true);
        self.banded_region.remove(rect);
        return Some((rect, value));
    }
}

impl<'a, V: Clone + Eq> Drop for BandedRegionDrainingIterator<'a, V> {
    fn drop(&mut self) {
        while self.next().is_some() {}
    }
}

impl<V> BandedRegion<V> {
    pub fn update_values(&mut self, mut update_fn: impl FnMut(Rect, &mut V)) {
        unsafe {
            let mut band_node = self.bands.lowest_node_raw().unwrap_or_default();
            while !band_node.is_null() {
                let band = (*band_node).value();
                let mut region_node = band.regions.lowest_node_raw().unwrap_or_default();
                while !region_node.is_null() {
                    let region = (*region_node).value_mut();
                    let rect = Rect::new(region.x_start, band.y_start, region.x_end, band.y_end);
                    update_fn(rect, &mut region.value);
                    region_node = (*region_node).successor().unwrap_or_default();
                }
                band_node = (*band_node).successor().unwrap_or_default();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{print, println};

    fn print_banded_region<V: core::fmt::Debug>(r: &BandedRegion<V>) {
        for node in r.bands.iter() {
            let band = node.value();
            print!("{:4}..{:4}: ", band.y_start, band.y_end);
            for node_region in band.regions.iter() {
                let region = node_region.value();
                print!(
                    "{:4}..{:4} ({:?}), ",
                    region.x_start, region.x_end, region.value
                );
            }
            print!("\n");
        }
    }

    #[test_case]
    fn test_banded_region() {
        let mut region = BandedRegion::new();
        // overlapping rectangles
        region.update(Rect::new(0, 0, 30, 30), |_| Some(()));
        region.update(Rect::new(20, 20, 50, 50), |_| Some(()));
        region.update(Rect::new(10, 25, 40, 35), |_| Some(()));

        // punch holes
        region.update(Rect::new(5, 5, 15, 15), |_| None);
        region.update(Rect::new(25, 25, 35, 35), |_| None);

        // adjacent (shared edge)
        region.update(Rect::new(50, 0, 60, 10), |_| Some(()));
        region.update(Rect::new(60, 0, 70, 10), |_| Some(()));

        // zero-area (should be a no-op or degenerate)
        region.update(Rect::new(5, 5, 5, 10), |_| Some(()));

        // fully contained hole (remove something entirely inside an existing rect)
        region.update(Rect::new(22, 0, 28, 5), |_| None);

        // re-add over a hole
        region.update(Rect::new(25, 25, 35, 35), |_| Some(()));
        print_banded_region(&region);
        region.coalesce();
        print_banded_region(&region);

        for (rect, _) in region.query(Rect::new(10, 10, 30, 30)) {
            println!("{rect:?}");
        }
    }
}
