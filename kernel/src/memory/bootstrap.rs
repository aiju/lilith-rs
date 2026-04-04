use crate::memory::rbtree::{Augment, RbNode, RbTree};
use crate::prelude::*;

use core::marker::PhantomData;
use core::mem::MaybeUninit;

use x86_64::PhysAddr;

use core::fmt::Debug;
use core::{alloc::Layout, ops::Range};

use x86_64::{
    VirtAddr,
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageSize, PageTableFlags, PhysFrame,
        Size1GiB, Size4KiB, Translate,
    },
};

use crate::memory::FRAME_LAYOUT;
use crate::memory::buddy::buddy_init;
use crate::memory::slub::slub_init;
use crate::memory::{
    KERNEL_STACK_SIZE, KERNEL_STACK_TOP, PHYSICAL_MEMORY_MAX_SIZE, PHYSICAL_MEMORY_OFFSET,
    frame_info::{FrameInfo, frame_info_addr},
    phys_to_mut, phys_to_virt,
};

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct MultibootAoutSyms {
    tabsize: u32,
    strsize: u32,
    addr: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct MultibootElfSections {
    num: u32,
    size: u32,
    addr: u32,
    shndx: u32,
}

#[repr(C)]
union MultibootSyms {
    raw: [u32; 4],
    aout: MultibootAoutSyms,
    elf: MultibootElfSections,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct MultibootColorIndexed {
    palette_addr: u32,
    palette_num_colors: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct MultibootColorRgb {
    red_field_position: u8,
    red_mask_size: u8,
    green_field_position: u8,
    green_mask_size: u8,
    blue_field_position: u8,
    blue_mask_size: u8,
}

#[repr(C)]
union MultibootColorInfo {
    raw: [u32; 6],
    indexed: MultibootColorIndexed,
    rgb: MultibootColorRgb,
}

#[repr(C)]
struct MultibootInfoRaw {
    flags: u32,                     // 0
    mem_lower: u32,                 // 4
    mem_upper: u32,                 // 8
    boot_device: u32,               // 12
    cmdline: u32,                   // 16
    mods_count: u32,                // 20
    mods_addr: u32,                 // 24
    syms: MultibootSyms,            // 28-40
    mmap_length: u32,               // 44
    mmap_addr: u32,                 // 48
    drives_length: u32,             // 52
    drives_addr: u32,               // 56
    config_table: u32,              // 60
    boot_loader_name: u32,          // 64
    apm_table: u32,                 // 68
    vbe_control_info: u32,          // 72
    vbe_mode_info: u32,             // 76
    vbe_mode: u16,                  // 80
    vbe_interface_seg: u16,         // 82
    vbe_interface_off: u16,         // 84
    vbe_interface_len: u16,         // 86
    framebuffer_addr: u64,          // 88
    framebuffer_pitch: u32,         // 96
    framebuffer_width: u32,         // 100
    framebuffer_height: u32,        // 104
    framebuffer_bpp: u8,            // 108
    framebuffer_type: u8,           // 109
    color_info: MultibootColorInfo, // 110-115
}

impl core::fmt::Debug for MultibootInfoRaw {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MultibootInfoRaw")
            .field("flags", &self.flags)
            .field("mem_lower", &self.mem_lower)
            .field("mem_upper", &self.mem_upper)
            .field("boot_device", &self.boot_device)
            .field("cmdline", &self.cmdline)
            .field("mods_count", &self.mods_count)
            .field("mods_addr", &self.mods_addr)
            .field("syms", unsafe { &self.syms.raw })
            .field("mmap_length", &self.mmap_length)
            .field("mmap_addr", &self.mmap_addr)
            .field("drives_length", &self.drives_length)
            .field("drives_addr", &self.drives_addr)
            .field("config_table", &self.config_table)
            .field("boot_loader_name", &self.boot_loader_name)
            .field("apm_table", &self.apm_table)
            .field("vbe_control_info", &self.vbe_control_info)
            .field("vbe_mode_info", &self.vbe_mode_info)
            .field("vbe_mode", &self.vbe_mode)
            .field("vbe_interface_seg", &self.vbe_interface_seg)
            .field("vbe_interface_off", &self.vbe_interface_off)
            .field("vbe_interface_len", &self.vbe_interface_len)
            .field("framebuffer_addr", &self.framebuffer_addr)
            .field("framebuffer_pitch", &self.framebuffer_pitch)
            .field("framebuffer_width", &self.framebuffer_width)
            .field("framebuffer_height", &self.framebuffer_height)
            .field("framebuffer_bpp", &self.framebuffer_bpp)
            .field("framebuffer_type", &self.framebuffer_type)
            .field("color_info", unsafe { &self.color_info.raw })
            .finish()
    }
}

enum MultibootMmapType {
    Reserved,
    Available,
    Acpi,
    Hibernation,
    BadRam,
}

#[repr(C, packed(1))]
#[derive(Debug)]
struct MultibootMmapEntry {
    entry_size: u32,
    addr: u64,
    len: u64,
    mem_type: u32,
}

impl MultibootMmapEntry {
    fn is_usable(&self) -> bool {
        match self.mem_type {
            1 => true,
            _ => false,
        }
    }
}

struct MultibootMmapIter<'a> {
    ptr: *const u8,
    end: *const u8,
    _phantom: PhantomData<&'a u8>,
}

impl MultibootInfoRaw {
    pub fn mmaps(&self) -> MultibootMmapIter<'_> {
        assert!(
            self.flags & 1 << 6 != 0,
            "no mmap data passed by bootloader"
        );
        MultibootMmapIter {
            ptr: phys_to_virt(unsafe { PhysAddr::new_unsafe(self.mmap_addr.into()) }).as_ptr(),
            end: phys_to_virt(unsafe {
                PhysAddr::new_unsafe(self.mmap_addr as u64 + self.mmap_length as u64)
            })
            .as_ptr(),
            _phantom: Default::default(),
        }
    }
}

impl<'a> Iterator for MultibootMmapIter<'a> {
    type Item = &'a MultibootMmapEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr == self.end {
            None
        } else {
            unsafe {
                assert!(self.ptr.add(core::mem::size_of::<MultibootMmapEntry>()) <= self.end);
                let r = &*(self.ptr as *const MultibootMmapEntry);
                self.ptr = self.ptr.add(4 + r.entry_size as usize);
                Some(r)
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SpanType {
    Reserved,
    InUse,
    Reclaimable,
    Free,
}

impl SpanType {
    fn needs_frameinfo(self) -> bool {
        match self {
            SpanType::Reserved => false,
            SpanType::InUse | SpanType::Reclaimable | SpanType::Free => true,
        }
    }
    fn is_usable(self) -> bool {
        match self {
            SpanType::Reserved | SpanType::InUse => false,
            SpanType::Reclaimable | SpanType::Free => true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Span {
    start: u64,
    end: u64,
    span_type: SpanType,
}

type NodeSlot = MaybeUninit<RbNode<Span, MaxFree>>;

struct MaxFree(u64);

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

struct BootAlloc {
    tree: RbTree<Span, MaxFree>,
    free_node: *mut NodeSlot,
}

static mut SPANS: [NodeSlot; 128] = [const { MaybeUninit::uninit() }; 128];

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
    fn mark_reserved(&mut self, start: u64, len: u64) {
        self.update(start, len, |_| SpanType::Reserved);
    }
    fn mark_used(&mut self, start: u64, len: u64) {
        self.update(start, len, |s| match s {
            Some(SpanType::Reserved) => SpanType::Reserved,
            _ => SpanType::InUse,
        });
    }
    fn mark_reclaimable(&mut self, start: u64, len: u64) {
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
}

struct ReclaimableRangeIter<'a> {
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

fn boot_oom() -> ! {
    panic!("out of memory on boot -- shouldn't happen");
}

unsafe impl FrameAllocator<Size4KiB> for BootAlloc {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        unsafe {
            Some(PhysFrame::from_start_address_unchecked(
                self.alloc(FRAME_LAYOUT).unwrap_or_else(|| boot_oom()),
            ))
        }
    }
}

unsafe extern "C" {
    static __text_start: u8;
    static __data_start: u8;
    static __bss_end: u8;
    static __text_start_phys: u8;
    static __data_start_phys: u8;
    static __bss_end_phys: u8;
    static __reclaimable_start_phys: u8;
    static __reclaimable_end_phys: u8;
}

#[inline(always)]
fn text_start() -> VirtAddr {
    VirtAddr::from_ptr(&raw const __text_start)
}
#[inline(always)]
fn data_start() -> VirtAddr {
    VirtAddr::from_ptr(&raw const __data_start)
}
#[inline(always)]
fn bss_end() -> VirtAddr {
    VirtAddr::from_ptr(&raw const __bss_end)
}

struct Bootstrap<'a> {
    boot_alloc: BootAlloc,
    offset_page_table: OffsetPageTable<'a>,
    previous_table: OffsetPageTable<'a>,
}

impl Bootstrap<'_> {
    unsafe fn map<S>(&mut self, virt: VirtAddr, phys: PhysAddr, flags: PageTableFlags)
    where
        S: PageSize + Debug,
        for<'a> OffsetPageTable<'a>: Mapper<S>,
    {
        unsafe {
            self.offset_page_table
                .map_to(
                    Page::<S>::from_start_address_unchecked(virt),
                    PhysFrame::from_start_address_unchecked(phys),
                    flags,
                    &mut self.boot_alloc,
                )
                .expect("map error -- shouldn't happen")
                .ignore();
        }
    }
    unsafe fn ensure_allocated(&mut self, virt: VirtAddr) -> PhysAddr {
        unsafe {
            let flags_rwdata = PageTableFlags::PRESENT
                | PageTableFlags::GLOBAL
                | PageTableFlags::WRITABLE
                | PageTableFlags::NO_EXECUTE;
            match self.offset_page_table.translate_addr(virt) {
                None => {
                    let phys = self
                        .boot_alloc
                        .alloc(Layout::from_size_align_unchecked(4096, 4096))
                        .unwrap_or_else(|| boot_oom());
                    self.map::<Size4KiB>(virt.align_down(4096u64), phys, flags_rwdata);
                    phys + (virt.as_u64() & 4095)
                }
                Some(phys) => phys,
            }
        }
    }
    unsafe fn copy_mapping_4kib(&mut self, start: VirtAddr, end: VirtAddr, flags: PageTableFlags) {
        assert!(start.is_aligned(4096u64));
        for virt in (start..end).step_by(4096) {
            let phys = self
                .previous_table
                .translate_addr(virt)
                .expect("page not mapped");
            unsafe { self.map::<Size4KiB>(virt, phys, flags) };
        }
    }
    unsafe fn setup_page_table(&mut self) {
        unsafe {
            let flags_rwdata = PageTableFlags::PRESENT
                | PageTableFlags::GLOBAL
                | PageTableFlags::WRITABLE
                | PageTableFlags::NO_EXECUTE;
            let flags_code = PageTableFlags::PRESENT | PageTableFlags::GLOBAL;

            self.offset_page_table.level_4_table().zero();

            // set up direct map at PHYSICAL_MEMORY_OFFSET
            for i in (0..PHYSICAL_MEMORY_MAX_SIZE).step_by(1024 * 1024 * 1024) {
                self.map::<Size1GiB>(
                    PHYSICAL_MEMORY_OFFSET + i,
                    PhysAddr::new_unsafe(i as u64),
                    flags_rwdata,
                );
            }

            // map kernel text / data / bss
            self.copy_mapping_4kib(text_start(), data_start(), flags_code);
            self.copy_mapping_4kib(data_start(), bss_end(), flags_rwdata);

            // allocate a new kernel stack
            for addr in (KERNEL_STACK_TOP - KERNEL_STACK_SIZE..KERNEL_STACK_TOP).step_by(4096) {
                self.ensure_allocated(addr);
            }

            // allocate and map pages for FrameInfo structs
            let mut addr = 0;
            while let Some(node) = self.boot_alloc.tree.lower_bound(|s, _| s.start >= addr) {
                let span = *(*node).value();
                if span.span_type.needs_frameinfo() {
                    let fi_start =
                        frame_info_addr(PhysAddr::new_unsafe(span.start).align_down(4096u64))
                            .align_down(4096u64);
                    let fi_end = frame_info_addr(PhysAddr::new_unsafe(span.end).align_up(4096u64))
                        .align_up(4096u64);
                    // might want to be more efficient here somehow
                    for fi_addr in (fi_start..fi_end).step_by(4096) {
                        let phys = self.ensure_allocated(fi_addr);
                        // sometimes does redundant work if things aren't page-aligned but should be OK
                        FrameInfo::new_page(phys_to_virt(phys).as_mut_ptr());
                    }
                }
                addr = span.end;
            }
        }
    }
}

pub unsafe fn init(multiboot_info_addr: PhysAddr) {
    unsafe {
        let multiboot_info: &MultibootInfoRaw = &*phys_to_virt(multiboot_info_addr).as_ptr();
        #[allow(static_mut_refs)]
        let mut boot_alloc = BootAlloc::new(&mut SPANS);

        // have to be careful not to allocate from boot_alloc until it's populated properly
        for span in multiboot_info.mmaps() {
            boot_alloc.update(span.addr, span.len, |s| {
                if span.is_usable() {
                    s.unwrap_or(SpanType::Free)
                } else {
                    SpanType::Reserved
                }
            });
        }

        boot_alloc.mark_reserved(0, 4096);
        boot_alloc.mark_used(
            &raw const __text_start_phys as u64,
            &raw const __bss_end_phys as u64 - &raw const __text_start_phys as u64,
        );
        boot_alloc.mark_reclaimable(
            multiboot_info_addr.as_u64(),
            core::mem::size_of::<MultibootInfoRaw>() as u64,
        );
        boot_alloc.mark_reclaimable(
            multiboot_info.mmap_addr as u64,
            multiboot_info.mmap_length as u64,
        );
        boot_alloc.mark_reclaimable(
            &raw const __reclaimable_start_phys as u64,
            &raw const __reclaimable_end_phys as u64 - &raw const __reclaimable_start_phys as u64,
        );

        // now we can allocate
        let page_table = boot_alloc
            .alloc(Layout::from_size_align_unchecked(4096, 4096))
            .unwrap_or_else(|| boot_oom());
        super::address_space::init(page_table);
        let offset_page_table =
            OffsetPageTable::new(phys_to_mut(page_table), PHYSICAL_MEMORY_OFFSET);
        let previous_table_ref = phys_to_mut(Cr3::read().0.start_address());
        let previous_table = OffsetPageTable::new(previous_table_ref, PHYSICAL_MEMORY_OFFSET);

        let mut bootstrap = Bootstrap {
            boot_alloc,
            offset_page_table,
            previous_table,
        };

        bootstrap.setup_page_table();

        let flags = Cr3::read().1;
        Cr3::write(PhysFrame::from_start_address_unchecked(page_table), flags);

        buddy_init(bootstrap.boot_alloc.reclaimable_range_iter());
        slub_init();
    }
}
