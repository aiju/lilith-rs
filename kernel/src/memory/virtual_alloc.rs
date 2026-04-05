use core::{
    alloc::Layout,
    ops::{Add, Range, Sub},
};

use x86_64::VirtAddr;

use crate::{
    memory::{
        FRAME_SIZE, MemoryError,
        address_space::KERNEL_ADDRESS_SPACE,
        rbtree::{Augment, OwnedRbNode, RbTree},
    },
    sync::IrqLock,
};

pub const VIRTUAL_ALLOCATOR_START: VirtAddr = VirtAddr::new_truncate(0xFFFF_A000_0000_0000);
pub const VIRTUAL_ALLOCATOR_END: VirtAddr = VirtAddr::new_truncate(0xFFFF_B000_0000_0000);

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
pub trait Address: Ord + Add<Output = Self> + Sub<Output = Self> + Default + Copy {}
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

pub struct SpanAlloc<A, V> {
    spans: RbTree<Span<A, V>, MaxGap<A>>,
    range: Range<A>,
}

#[derive(Debug, Clone)]
pub enum SpanError {
    Memory(MemoryError),
    Exhausted,
}

impl<A, V> SpanAlloc<A, V> {
    const fn new(range: Range<A>) -> Self {
        SpanAlloc {
            spans: RbTree::new(),
            range,
        }
    }
}

fn node_cmp<A: Address, V>(x: &Span<A, V>, y: &Span<A, V>) -> core::cmp::Ordering {
    A::cmp(&x.start, &y.start)
}

fn by_containing<A: Address, V>(
    addr: A,
) -> impl FnMut(&Span<A, V>, &MaxGap<A>) -> core::cmp::Ordering {
    move |span, _| {
        if addr < span.start {
            core::cmp::Ordering::Greater
        } else if addr >= span.end {
            core::cmp::Ordering::Less
        } else {
            core::cmp::Ordering::Equal
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
    pub fn alloc(&mut self, size: A, value: V) -> Result<Range<A>, SpanError> {
        let gap = self.find_gap(size).ok_or(SpanError::Exhausted)?;
        let range = gap.start..gap.start + size;
        let span = Span {
            start: range.start,
            end: range.end,
            value,
        };
        let node = OwnedRbNode::new_direct(span).map_err(SpanError::Memory)?;
        self.spans.insert(node, node_cmp);
        Ok(range)
    }
    pub fn span_containing(&self, addr: A) -> Option<&V> {
        self.spans
            .find(by_containing(addr))
            .map(|x| &x.value().value)
    }
    pub unsafe fn free(&mut self, addr: A) -> (Range<A>, V) {
        let span = self
            .spans
            .find_mut(by_containing(addr))
            .expect("free with pointer not virtual_alloc'd")
            .remove()
            .into_value();
        (span.start..span.end, span.value)
    }
}

impl<A: Address + core::fmt::Debug, V> SpanAlloc<A, V> {
    fn check(&self) {
        self.spans.check(node_cmp);
    }
}

pub struct VirtualAllocation {
    guard_bottom: usize,
    guard_top: usize,
}

pub struct VirtualAllocator {
    spans: SpanAlloc<u64, VirtualAllocation>,
}

pub static VIRTUAL_ALLOCATOR: IrqLock<VirtualAllocator> = IrqLock::new(VirtualAllocator::new());

#[derive(Clone, Copy, Default)]
pub struct AllocSettings {
    pub guard_bottom: usize,
    pub guard_top: usize,
}

impl VirtualAllocator {
    fn err_map(err: SpanError) -> MemoryError {
        match err {
            SpanError::Memory(memory_error) => memory_error,
            SpanError::Exhausted => MemoryError::OutOfVirtual,
        }
    }
    pub const fn new() -> Self {
        VirtualAllocator {
            spans: SpanAlloc::new(VIRTUAL_ALLOCATOR_START.as_u64()..VIRTUAL_ALLOCATOR_END.as_u64()),
        }
    }
    fn try_map_pages(range: Range<u64>) -> Result<(), (Range<u64>, MemoryError)> {
        let mut kernel_as = KERNEL_ADDRESS_SPACE.lock();
        for addr in range.clone().step_by(FRAME_SIZE) {
            unsafe {
                kernel_as
                    .map_new_page(VirtAddr::new_unsafe(addr))
                    .map_err(|e| (range.start..addr, e))?
            };
        }
        Ok(())
    }
    fn unmap_pages(range: Range<u64>) {
        let mut kernel_as = KERNEL_ADDRESS_SPACE.lock();
        for addr in range.step_by(FRAME_SIZE) {
            unsafe { kernel_as.unmap_page(VirtAddr::new_unsafe(addr)) };
        }
    }
    pub fn alloc(
        &mut self,
        layout: Layout,
        settings: AllocSettings,
    ) -> Result<VirtAddr, MemoryError> {
        let guard_bottom = settings.guard_bottom.next_multiple_of(FRAME_SIZE);
        let guard_top = settings.guard_top.next_multiple_of(FRAME_SIZE);
        let layout = layout.align_to(FRAME_SIZE).unwrap().pad_to_align();
        assert!(layout.align() <= 4096);
        let total_size = layout.size() + guard_bottom + guard_top;
        let allocation = VirtualAllocation {
            guard_bottom,
            guard_top,
        };
        let total_range = self
            .spans
            .alloc(total_size as u64, allocation)
            .map_err(Self::err_map)?;
        let range = total_range.start + settings.guard_bottom as u64
            ..total_range.end - settings.guard_top as u64;
        match Self::try_map_pages(range.clone()) {
            Ok(()) => Ok(unsafe { VirtAddr::new_unsafe(range.start) }),
            Err((done_range, err)) => {
                Self::unmap_pages(done_range);
                unsafe { self.spans.free(range.start) };
                Err(err)
            }
        }
    }
    pub unsafe fn free(&mut self, addr: VirtAddr) {
        let (total_range, allocation) = unsafe { self.spans.free(addr.as_u64()) };
        let range = total_range.start + allocation.guard_bottom as u64
            ..total_range.end - allocation.guard_top as u64;
        assert_eq!(addr.as_u64(), range.start);
        let mut kernel_as = KERNEL_ADDRESS_SPACE.lock();
        for addr in range.clone().step_by(FRAME_SIZE) {
            unsafe { kernel_as.unmap_page(VirtAddr::new_unsafe(addr)) };
        }
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
                unsafe { valloc.free(v.start) };
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

#[test_case]
fn test_virtual_alloc() {
    use core::sync::atomic::AtomicU32;
    use core::sync::atomic::Ordering::Relaxed;

    let mut va = VIRTUAL_ALLOCATOR.lock();
    let n = 2 * 1024 * 1024;
    let sp: &mut [AtomicU32] = unsafe {
        core::slice::from_raw_parts_mut(
            va.alloc(
                Layout::from_size_align(n * 4, 4096).unwrap(),
                Default::default(),
            )
            .unwrap()
            .as_mut_ptr(),
            n,
        )
    };
    let mut rng = fastrand::Rng::with_seed(42);
    for i in 0..n {
        sp[i].store(rng.u32(..), Relaxed);
    }
    let mut rng = fastrand::Rng::with_seed(42);
    for i in 0..n {
        assert_eq!(sp[i].load(Relaxed), rng.u32(..));
    }
    unsafe { va.free(VirtAddr::from_ptr(sp.as_ptr())) }
}
