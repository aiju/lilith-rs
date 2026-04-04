Lilith OS project
===================


Memory map
------------
```
0xFFFF_8000_0000_0000 - ...                      Physical memory is mapped 1:1 here (direct map)
0xFFFF_9000_0000_0000 - ...                      Frame info tables are mapped here
0xFFFF_A000_0000_0000 - ...                      Virtual allocations happen here
0xFFFF_FFFF_8000_0000 - 0xFFFF_FFFF_9000_0000    Kernel code/data/bss
0xFFFF_FFFF_9FF0_0000 - 0xFFFF_FFFF_A000_0000    Kernel stack
```

Heap allocations happen in three different ways:

1. Allocations between 1KB and 4MB are allocated using a buddy allocator, so they get rounded to the next power of two of the page size (4KB). The allocator returns an address in the direct range.

2. Allocations smaller than this are allocated by taking a page from the buddy allocator and cutting it up into pieces of various fixed sizes, e.g. 64 bytes (SLUB allocator).

3. Allocations larger than 4 MB are allocated by assigning virtual address space in the virtual allocation region and backing that with 4 KB pages.

Compilation requirements
----------------------

`objcopy` needs to be in `$PATH` to build the kernel. On MacOS the easiest option is to `brew install binutils` and symlink `objcopy` from somewhere within `/opt/homebrew` to `/usr/local/bin`.


Rust-analyzer bug
--------------------

As of 28-03-2026 you need to put
```
[unstable]
json-target-spec = true
```
in your `$HOME/.cargo/config.toml` to get rust-analyzer to work properly on this project.
