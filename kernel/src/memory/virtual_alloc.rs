use core::{
    alloc::Layout,
    ops::{Add, Range, Sub},
};

use x86_64::VirtAddr;

use crate::memory::{
    FRAME_SIZE, kernel_alloc, kernel_alloc_ptr, kernel_free,
    rbtree::{Augment, RbNode, RbTree},
};

const VIRTUAL_ALLOC_START: u64 = 0xFFFF_A000_0000_0000;
const VIRTUAL_ALLOC_END: u64 = 0xFFFF_B000_0000_0000;

struct Span<A, V> {
    start: A,
    end: A,
    value: V,
}

#[derive(Debug, PartialEq, Eq)]
struct MaxGap<A> {
    min_addr: A,
    max_addr: A,
    max_gap: A,
}

// should maybe use a Zero trait instead of Default in augment?
trait Address: Ord + Add<Output = Self> + Sub<Output = Self> + Default + Copy {}
impl<A: Ord + Add<Output = Self> + Sub<Output = Self> + Default + Copy> Address for A {}

impl<A: Address, V> Augment<Span<A, V>> for MaxGap<A> {
    fn augment(node: &Span<A, V>, left: &Option<Self>, right: &Option<Self>) -> Self {
        let mut max_gap = A::default();
        let mut min_addr = node.start;
        let mut max_addr = node.end;
        if let Some(left) = left {
            max_gap = max_gap.max(left.max_gap);
            max_gap = max_gap.max(node.start - left.max_addr);
            min_addr = min_addr.min(left.min_addr);
            debug_assert!(left.max_addr <= node.start);
        }
        if let Some(right) = right {
            max_gap = max_gap.max(right.max_gap);
            max_gap = max_gap.max(right.min_addr - node.end);
            max_addr = max_addr.max(right.max_addr);
            debug_assert!(node.end <= right.min_addr);
        }
        MaxGap {
            min_addr,
            max_addr,
            max_gap,
        }
    }
}

struct SpanAlloc<A, V> {
    spans: RbTree<Span<A, V>, MaxGap<A>>,
    range: Range<A>,
}

impl<A, V> SpanAlloc<A, V> {
    const fn new(range: Range<A>) -> Self {
        SpanAlloc {
            spans: RbTree::new(),
            range,
        }
    }
}

impl<A: Address, V> SpanAlloc<A, V> {
    fn find_gap(&self, size: A) -> Option<Range<A>> {
        let Some(mut node) = self.spans.head() else {
            return Some(self.range.clone());
        };
        let mut tree_min = self.range.start;
        let mut tree_max = self.range.end;
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
    pub fn alloc(&mut self, size: A, value: V) -> Option<A> {
        let gap = self.find_gap(size)?;
        let span = Span {
            start: gap.start,
            end: gap.start + size,
            value,
        };
        let node = kernel_alloc_ptr().unwrap();
        unsafe { core::ptr::write(node, RbNode::new(span)) };
        self.spans.insert(node, |x, y| A::cmp(&x.start, &y.start));
        Some(gap.start)
    }
    pub unsafe fn free(&mut self, addr: A) {
        let node = self
            .spans
            .find(|vmap, _| vmap.start.cmp(&addr))
            .expect("free with pointer not virtual_alloc'd");
        self.spans.remove(node);
        unsafe { kernel_free(VirtAddr::from_ptr(node)) };
    }
}

impl<A: Address + core::fmt::Debug, V> SpanAlloc<A, V> {
    fn check(&self) {
        self.spans.check(|x, y| A::cmp(&x.start, &y.start));
    }
}

#[test_case]
fn test_span_alloc() {
    use alloc::vec::Vec;

    let mut rng = fastrand::Rng::with_seed(42);
    let range = 1000000..2000000;
    for _ in 0..10 {
        let mut valloc = SpanAlloc::<u64, ()>::new(range.clone());
        let mut allocations = Vec::new();
        for _ in 0..1000 {
            if allocations.is_empty() || rng.u32(0..2) == 0 {
                let size = rng.u64(1..=100);
                //serial_print!("alloc {:x} = ", size);
                let addr = valloc.alloc(size, ()).expect("alloc failed");
                allocations.push(addr);
                //serial_print!("{:?}\n", addr);
            } else {
                let i = rng.usize(0..allocations.len());
                let v = allocations.swap_remove(i);
                //serial_println!("free({:?}\n", v);
                unsafe { valloc.free(v) };
            }

            valloc.check();
            let mut right_bound = range.start;
            for span in valloc.spans.iter() {
                assert!(span.value().start >= right_bound);
                //serial_println!("{:?} {:?}  : {:x}..{:x} [{:8x}]   {:x}..{:x} [{:x}]", span as *const RbNode<_, _>, span.parent().map(|x| x as *const _), span.value().start, span.value().end, span.value().start - right_bound, span.augment().min_addr, span.augment().max_addr, span.augment().max_gap);
                right_bound = span.value().end;
            }
            assert!(right_bound <= range.end);
        }
    }
}
