use core::{alloc::Layout, ops::Range};

use x86_64::VirtAddr;

use crate::memory::{
    FRAME_SIZE, kernel_alloc,
    rbtree::{Augment, RbNode, RbTree},
};

const VIRTUAL_ALLOC_START: u64 = 0xFFFF_A000_0000_0000;
const VIRTUAL_ALLOC_END: u64 = 0xFFFF_B000_0000_0000;

#[derive(PartialEq, Eq, Debug)]
struct VMap {
    start: u64,
    end: u64,
}

impl PartialOrd for VMap {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some((*self).cmp(other))
    }
}

impl Ord for VMap {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.start).cmp(&other.start)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct MaxGap {
    min_addr: u64,
    max_addr: u64,
    max_gap: u64,
}

impl Default for MaxGap {
    fn default() -> Self {
        Self {
            min_addr: VIRTUAL_ALLOC_END,
            max_addr: VIRTUAL_ALLOC_START,
            max_gap: 0,
        }
    }
}

impl Augment<VMap> for MaxGap {
    fn augment(node: &VMap, left: &Self, right: &Self) -> Self {
        let gap1 = if left.min_addr <= left.max_addr {
            node.start - left.max_addr
        } else {
            0
        };
        let gap2 = if right.min_addr <= right.max_addr {
            right.min_addr - node.end
        } else {
            0
        };
        let max_gap = gap1.max(gap2).max(left.max_gap).max(right.max_gap);
        MaxGap {
            min_addr: left.min_addr.min(node.start).min(right.min_addr),
            max_addr: left.max_addr.max(node.end).max(right.max_addr),
            max_gap,
        }
    }
}

struct VirtualAlloc {
    vmaps: RbTree<VMap, MaxGap>,
}

impl VirtualAlloc {
    const fn new() -> Self {
        VirtualAlloc {
            vmaps: RbTree::new(),
        }
    }
    fn find_gap(&self, size: u64) -> Option<Range<u64>> {
        let Some(mut node) = self.vmaps.head() else {
            return Some(VIRTUAL_ALLOC_START..VIRTUAL_ALLOC_END);
        };
        let mut tree_min = VIRTUAL_ALLOC_START;
        let mut tree_max = VIRTUAL_ALLOC_END;
        loop {
            let left = node.left().map(|n| n.augment());
            let right = node.right().map(|n| n.augment());
            let left_end = if let Some(left) = left {
                if left.min_addr - tree_min >= size {
                    return Some(tree_min..left.min_addr);
                }
                if left.max_gap >= size {
                    tree_max = node.value().start;
                    node = node.left().unwrap();
                    continue;
                }
                left.max_addr
            } else {
                tree_min
            };
            let right_start = right.map(|x| x.min_addr).unwrap_or(tree_max);
            if node.value().start - left_end >= size {
                return Some(left_end..node.value().start);
            }
            if right_start - node.value().end >= size {
                return Some(node.value().end..right_start);
            }
            if let Some(right) = right {
                if right.max_gap >= size {
                    tree_min = node.value().end;
                    node = node.right().unwrap();
                    continue;
                }
                if tree_max - right.max_addr >= size {
                    return Some(right.max_addr..tree_max);
                }
            }
            return None;
        }
    }
    pub fn alloc(&mut self, layout: Layout) -> Option<VirtAddr> {
        let layout = layout.align_to(FRAME_SIZE).unwrap().pad_to_align();
        let gap = self.find_gap(layout.size() as u64)?;
        let vmap = VMap {
            start: gap.start,
            end: gap.start + layout.size() as u64,
        };
        let node: *mut RbNode<VMap, MaxGap> =
            kernel_alloc(Layout::new::<RbNode<VMap, MaxGap>>())?.as_mut_ptr();
        unsafe { core::ptr::write(node, RbNode::new(vmap)) };
        self.vmaps.insert(node);
        Some(VirtAddr::new(gap.start))
    }
    pub unsafe fn free(&mut self, addr: VirtAddr) {
        let node = self
            .vmaps
            .find(|vmap, _| vmap.start.cmp(&addr.as_u64()))
            .expect("free with pointer not virtual_alloc'd");
        self.vmaps.remove(node);
    }
}

#[test_case]
fn test_virtual_alloc() {
    use alloc::vec::Vec;
    
    let mut rng = fastrand::Rng::with_seed(42);
    for _ in 0..10 {
        let mut valloc = VirtualAlloc::new();
        let mut allocations = Vec::new();
        for _ in 0..1000 {
            if allocations.is_empty() || rng.u32(0..2) == 0 {
                let size = rng.usize(1..=100) * FRAME_SIZE;
                //serial_print!("alloc {:x} = ", size);
                let layout = Layout::from_size_align(size, FRAME_SIZE).unwrap();
                let addr = valloc.alloc(layout).expect("alloc failed");
                allocations.push(addr);
                //serial_print!("{:?}\n", addr);
            } else {
                let i = rng.usize(0..allocations.len());
                let v = allocations.swap_remove(i);
                //serial_println!("free({:?}\n", v);
                unsafe { valloc.free(v) };
            }

            valloc.vmaps.check();
            let mut right_bound = VIRTUAL_ALLOC_START;
            for span in valloc.vmaps.iter() {
                assert!(span.value().start >= right_bound);
                //serial_println!("{:?} {:?}  : {:x}..{:x} [{:8x}]   {:x}..{:x} [{:x}]", span as *const RbNode<_, _>, span.parent().map(|x| x as *const _), span.value().start, span.value().end, span.value().start - right_bound, span.augment().min_addr, span.augment().max_addr, span.augment().max_gap);
                right_bound = span.value().end;
            }
            assert!(right_bound <= VIRTUAL_ALLOC_END);
        }
    }
}
