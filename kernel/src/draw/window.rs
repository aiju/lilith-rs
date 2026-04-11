use alloc::vec::Vec;

use crate::{
    define_id,
    draw::{
        banded_region::BandedRegion,
        geometry::{Color, Rect},
        surface::Surface,
    },
    id_vec::IdSparseVec,
};

define_id!(WindowId);

pub struct Window {
    surface: Surface,
    z_index: usize,
}

impl Window {
    pub fn rect(&self) -> Rect {
        self.surface.rect()
    }
    pub fn surface_mut(&mut self) -> &mut Surface {
        &mut self.surface
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Region {
    z_index: usize,
    dirty: bool,
}

pub struct Screen {
    windows: IdSparseVec<WindowId, Window>,
    visible: Vec<WindowId>,
    damage_tracker: BandedRegion<Region>,
}

impl Screen {
    pub fn new(rect: Rect) -> Screen {
        let mut screen = Screen {
            windows: IdSparseVec::new(),
            visible: Vec::new(),
            damage_tracker: BandedRegion::new(),
        };
        screen.damage_tracker.update(rect, |_| {
            Some(Region {
                z_index: usize::MAX,
                dirty: true,
            })
        });
        screen
    }
    fn reset_damage_tracker(&mut self, rect: Rect) {
        self.damage_tracker = BandedRegion::new();
        self.damage_tracker.update(rect, |_| {
            Some(Region {
                z_index: usize::MAX,
                dirty: false,
            })
        });
        for &id in self.visible.iter().rev() {
            let window = self.windows.get(id).unwrap();
            self.damage_tracker.update(window.rect(), |_| {
                Some(Region {
                    z_index: window.z_index,
                    dirty: false,
                })
            });
        }
    }
    pub fn update(&mut self, frame_buffer: &mut Surface) {
        for (rect, region) in self.damage_tracker.drain(frame_buffer.rect()) {
            if !region.dirty {
                continue;
            }
            if region.z_index == usize::MAX {
                frame_buffer.fill(rect, Color::WHITE);
            } else {
                let window = self.windows.get(self.visible[region.z_index]).unwrap();
                frame_buffer.blit(rect, &window.surface, rect.tl);
            }
        }
        self.reset_damage_tracker(frame_buffer.rect());
    }
    pub fn mark_dirty_beyond(&mut self, z_index: usize, rect: Rect) {
        self.damage_tracker.update(rect, |region| {
            region.map(|r| Region {
                z_index: r.z_index,
                dirty: r.dirty || r.z_index >= z_index,
            })
        });
    }
    pub fn mark_dirty(&mut self, window_id: WindowId, rect: Rect) {
        let window = self.windows.get(window_id).unwrap();
        if let Some(rect) = rect.intersect(window.rect()) {
            self.damage_tracker.update(rect, |region| {
                if region.filter(|r| r.z_index < window.z_index).is_some() {
                    region.cloned()
                } else {
                    Some(Region {
                        z_index: window.z_index,
                        dirty: true,
                    })
                }
            });
        }
    }
    fn z_index_update(&mut self, update_fn: impl Fn(usize) -> usize) {
        for (_, window) in self.windows.iter_mut() {
            window.z_index = update_fn(window.z_index)
        }
        self.damage_tracker
            .update_values(|_, region| region.z_index = update_fn(region.z_index));
    }
    pub fn new_window(&mut self, rect: Rect, mut z_index: usize) -> WindowId {
        z_index = z_index.min(self.visible.len());
        let surface = Surface::new(rect, Color::WHITE);
        self.z_index_update(|z| if z >= z_index && z != usize::MAX { z + 1 } else { z });
        let window_id = self.windows.insert(Window { surface, z_index });
        self.visible.insert(z_index, window_id);
        self.mark_dirty(window_id, rect);
        window_id
    }
    fn rebuild_damage_tracker_move(&mut self, z_index: usize, rect: Rect) {
        for &id in self.visible.iter().rev() {
            let window = self.windows.get(id).unwrap();
            if window.z_index <= z_index {
                continue;
            }
            if let Some(r) = window.rect().intersect(rect) {
                self.damage_tracker.update(r, |region| {
                    if region.filter(|r| r.z_index < z_index).is_some() {
                        region.cloned()
                    } else {
                        Some(Region {
                            z_index: window.z_index,
                            dirty: true,
                        })
                    }
                })
            }
        }
    }
    fn rebuild_damage_tracker_move_to_back(
        &mut self,
        old_z_index: usize,
        new_z_index: usize,
        rect: Rect,
    ) {
        for &id in self.visible.iter().rev() {
            let window = self.windows.get(id).unwrap();
            if window.z_index < old_z_index || window.z_index >= new_z_index {
                continue;
            }
            if let Some(r) = window.rect().intersect(rect) {
                self.damage_tracker.update(r, |region| {
                    if region
                        .filter(|r| r.z_index < old_z_index || r.z_index >= new_z_index)
                        .is_some()
                    {
                        region.cloned()
                    } else {
                        Some(Region {
                            z_index: window.z_index,
                            dirty: true,
                        })
                    }
                })
            }
        }
    }
    pub fn move_window(&mut self, window_id: WindowId, new_rect: Rect) {
        let window = self.windows.get(window_id).unwrap();
        self.rebuild_damage_tracker_move(window.z_index, window.rect());
        let window = self.windows.get_mut(window_id).unwrap();
        window.surface = Surface::new(new_rect, Color::WHITE);
        self.mark_dirty(window_id, new_rect);
    }
    pub fn set_window_z_index(&mut self, window_id: WindowId, mut new_z_index: usize) {
        new_z_index = new_z_index.min(self.visible.len() - 1);
        let window = self.windows.get(window_id).unwrap();
        let old_z_index = window.z_index;
        let rect = window.rect();
        if new_z_index == old_z_index {
            return;
        }
        if new_z_index > old_z_index {
            self.z_index_update(|z| {
                if z > new_z_index {
                    z
                } else if z > old_z_index {
                    z - 1
                } else if z == old_z_index {
                    new_z_index
                } else {
                    z
                }
            });
            self.visible[old_z_index..=new_z_index].rotate_left(1);
            self.rebuild_damage_tracker_move_to_back(old_z_index, new_z_index, rect);
        } else {
            self.z_index_update(|z| {
                if z > old_z_index {
                    z
                } else if z == old_z_index {
                    new_z_index
                } else if z >= new_z_index {
                    z + 1
                } else {
                    z
                }
            });
            self.visible[new_z_index..=old_z_index].rotate_right(1);
            self.damage_tracker.update(rect, |region| {
                if region.filter(|r| r.z_index <= new_z_index).is_some() {
                    region.cloned()
                } else {
                    Some(Region {
                        z_index: new_z_index,
                        dirty: true,
                    })
                }
            });
        }
    }
    // TODO: allows illegal operations on windows like changing rect
    pub fn window_mut(&mut self, window_id: WindowId) -> &mut Window {
        self.windows.get_mut(window_id).unwrap()
    }
}
