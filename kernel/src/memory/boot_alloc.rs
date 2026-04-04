use crate::memory::buddy::buddy_add_range;
use crate::memory::rbtree::{Augment, RbNode, RbTree};
#[allow(unused_imports)]
use crate::prelude::*;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::{alloc::Layout, ops::Range};
use x86_64::PhysAddr;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpanType {
    Reserved,
    InUse,
    Reclaimable,
    Free,
}

impl SpanType {
    pub fn needs_frameinfo(self) -> bool {
        match self {
            SpanType::Reserved => false,
            SpanType::InUse | SpanType::Reclaimable | SpanType::Free => true,
        }
    }
    pub fn is_usable(self) -> bool {
        match self {
            SpanType::Reserved | SpanType::InUse => false,
            SpanType::Reclaimable | SpanType::Free => true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Span {
    pub start: u64,
    pub end: u64,
    pub span_type: SpanType,
}

pub type NodeSlot = MaybeUninit<RbNode<Span, MaxFree>>;

pub struct MaxFree(u64);

impl Augment<Span> for MaxFree {
    fn augment(node: &Span, left: &Option<Self>, right: &Option<Self>) -> Self {
        let a = if node.span_type == SpanType::Free {
            node.end - node.start
        } else {
            0
        };
        MaxFree(
            a.max(left.as_ref().map(|x| x.0).unwrap_or(0))
                .max(right.as_ref().map(|x| x.0).unwrap_or(0)),
        )
    }
}

pub struct BootAlloc {
    tree: RbTree<Span, MaxFree>,
    free_node: *mut NodeSlot,
}

impl BootAlloc {
    pub fn new(spans: &mut [NodeSlot]) -> BootAlloc {
        unsafe {
            let mut result = BootAlloc {
                tree: RbTree::new(),
                free_node: core::ptr::null_mut(),
            };
            let mut p = &raw mut result.free_node;
            for span in spans.iter_mut() {
                (*p) = span;
                p = span as *mut NodeSlot as *mut *mut NodeSlot;
            }
            *p = core::ptr::null_mut();
            result
        }
    }
    pub fn tree(&self) -> &RbTree<Span, MaxFree> {
        &self.tree
    }
    fn insert(&mut self, span: Span) {
        unsafe {
            if self.free_node.is_null() {
                self.compact();
                assert!(!self.free_node.is_null(), "BootAlloc ran out of spans!");
            }
            let node = self.free_node;
            self.free_node = *(node as *mut *mut NodeSlot);
            (*node).write(RbNode::new(span));
            self.tree
                .insert((*node).assume_init_mut(), |a, b| a.start.cmp(&b.start));
        }
    }
    fn remove(&mut self, node: *mut RbNode<Span, MaxFree>) -> Span {
        unsafe {
            let result = *(*node).value();
            self.tree.remove(node);
            *(node as *mut *mut NodeSlot) = self.free_node;
            self.free_node = node as *mut NodeSlot;
            result
        }
    }
    pub fn update(
        &mut self,
        mut addr: u64,
        len: u64,
        update_fn: impl Fn(Option<SpanType>) -> SpanType,
    ) {
        unsafe {
            let end = addr + len;
            while addr != end {
                // can do slightly better by walking from node to successor instead of looping lower_bound
                let Some(node) = self.tree.lower_bound(|n, _| n.end > addr) else {
                    break;
                };
                if (*node).value().start >= end {
                    break;
                }
                // we know n.end > addr and n.start < end
                let old_span = self.remove(node);
                if old_span.start > addr {
                    self.insert(Span {
                        start: addr,
                        end: old_span.start,
                        span_type: update_fn(None),
                    });
                    addr = old_span.start;
                } else if old_span.start < addr {
                    self.insert(Span {
                        start: old_span.start,
                        end: addr,
                        span_type: old_span.span_type,
                    });
                }
                let n_end = old_span.end.min(end);
                self.insert(Span {
                    start: addr,
                    end: n_end,
                    span_type: update_fn(Some(old_span.span_type)),
                });
                if old_span.end > end {
                    self.insert(Span {
                        start: end,
                        end: old_span.end,
                        span_type: old_span.span_type,
                    });
                }
                addr = n_end;
            }
            if addr != end {
                self.insert(Span {
                    start: addr,
                    end,
                    span_type: update_fn(None),
                });
            }
        }
    }
    pub fn mark_reserved(&mut self, start: u64, len: u64) {
        self.update(start, len, |_| SpanType::Reserved);
    }
    pub fn mark_used(&mut self, start: u64, len: u64) {
        self.update(start, len, |s| match s {
            Some(SpanType::Reserved) => SpanType::Reserved,
            _ => SpanType::InUse,
        });
    }
    pub fn mark_reclaimable(&mut self, start: u64, len: u64) {
        self.update(start, len, |s| match s {
            Some(SpanType::Reserved) => SpanType::Reserved,
            _ => SpanType::Reclaimable,
        });
    }
    fn alloc_worker(&mut self, layout: Layout, node: *mut RbNode<Span, MaxFree>) -> Option<u64> {
        unsafe {
            if let Some(left) = (*node).left() {
                if left.augment().0 >= layout.size() as u64 {
                    if let Some(result) = self.alloc_worker(layout, left as *const _ as *mut _) {
                        return Some(result);
                    }
                }
            }
            let span = (*node).value();
            if span.span_type == SpanType::Free {
                let start = span.start.next_multiple_of(layout.align() as u64);
                let available = span.end.saturating_sub(start);
                if available >= layout.size() as u64 {
                    return Some(start);
                }
            }
            if let Some(right) = (*node).right() {
                if right.augment().0 >= layout.size() as u64 {
                    if let Some(result) = self.alloc_worker(layout, right as *const _ as *mut _) {
                        return Some(result);
                    }
                }
            }
            None
        }
    }
    pub fn alloc(&mut self, layout: Layout) -> Option<PhysAddr> {
        let layout = layout.align_to(4096).unwrap().pad_to_align();
        let start = self.alloc_worker(layout, self.tree.head()? as *const _ as *mut _)?;
        self.update(start, layout.size() as u64, |t| {
            assert!(t == Some(SpanType::Free));
            SpanType::InUse
        });
        Some(unsafe { PhysAddr::new_unsafe(start) })
    }
    pub fn reclaimable_range_iter(&self) -> ReclaimableRangeIter<'_> {
        ReclaimableRangeIter {
            node: self
                .tree
                .lowest_node()
                .map(|r| r as *const _ as *mut _)
                .unwrap_or_default(),
            _phantom: PhantomData,
        }
    }
    pub fn compact(&mut self) {
        unsafe {
            let Some(mut node) = self.tree.lowest_node() else {
                return;
            };
            while let Some(succ) = (*node).successor() {
                if (*node).value().span_type == (*succ).value().span_type
                    && (*node).value().end == (*succ).value().start
                {
                    let start = (*node).value().start;
                    let end = (*succ).value().end;
                    let span_type = (*node).value().span_type;
                    self.tree.remove(node); // don't reinsert node back into freelist, reuse it
                    self.remove(succ); // return succ to freelist
                    core::ptr::write(
                        node,
                        RbNode::new(Span {
                            start,
                            end,
                            span_type,
                        }),
                    );
                    self.tree.insert(node, |a, b| a.start.cmp(&b.start));
                    continue;
                }
                node = succ;
            }
        }
    }
    pub fn claim_free_ranges(&mut self) {
        // claim any full pages from the free ranges
        unsafe {
            self.compact();
            let mut addr = 0;
            while let Some(node) = self.tree.lower_bound(|s, _| s.start >= addr) {
                let span = *(*node).value();
                if span.span_type == SpanType::Free {
                    let start = PhysAddr::new_unsafe(span.start).align_up(4096u64);
                    let end = PhysAddr::new_unsafe(span.end).align_down(4096u64);
                    if end > start {
                        buddy_add_range(start, end);
                        self.remove(node);
                        self.insert(Span {
                            start: start.as_u64(),
                            end: end.as_u64(),
                            span_type: SpanType::InUse,
                        });
                        if span.start < start.as_u64() {
                            self.insert(Span {
                                start: span.start,
                                end: start.as_u64(),
                                span_type: SpanType::Free,
                            });
                        }
                        if end.as_u64() < span.end {
                            self.insert(Span {
                                start: end.as_u64(),
                                end: span.end,
                                span_type: SpanType::Free,
                            });
                        }
                    }
                }
                addr = span.end;
            }
        }
    }
}

pub struct ReclaimableRangeIter<'a> {
    node: *mut RbNode<Span, MaxFree>,
    _phantom: PhantomData<&'a Span>,
}

impl Iterator for ReclaimableRangeIter<'_> {
    type Item = Range<PhysAddr>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            loop {
                while !self.node.is_null() && !(*self.node).value().span_type.is_usable() {
                    self.node = (*self.node).successor().unwrap_or_default();
                }
                if self.node.is_null() {
                    return None;
                }
                let start_addr = (*self.node).value().start;
                let mut end_addr = (*self.node).value().end;
                let mut succ = (*self.node).successor().unwrap_or_default();
                while !succ.is_null()
                    && (*succ).value().start == end_addr
                    && (*succ).value().span_type.is_usable()
                {
                    end_addr = (*succ).value().end;
                    succ = (*succ).successor().unwrap_or_default();
                }
                self.node = succ;
                let s = PhysAddr::new_unsafe(start_addr).align_up(4096u64);
                let e = PhysAddr::new_unsafe(end_addr).align_down(4096u64);
                if s < e {
                    return Some(s..e);
                }
            }
        }
    }
}
