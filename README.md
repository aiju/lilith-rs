Lilith OS project
===================


Memory map
------------
```
0xFFFF_8000_0000_0000 - ...                      Physical memory is mapped 1:1 here
0xFFFF_9000_0000_0000 - ...                      Frame info tables are mapped here
0xFFFF_FFFF_8000_0000 - 0xFFFF_FFFF_9000_0000    Kernel code/data/bss
0xFFFF_FFFF_9000_0000 - 0xFFFF_FFFF_9000_1000    Boot info page (4 KB)
0xFFFF_FFFF_9FF0_0000 - 0xFFFF_FFFF_A000_0000    Kernel stack
0xFFFF_FFFF_A000_0000 - 0xFFFF_FFFF_B000_0000    Kernel heap
```


Rust-analyzer bug
--------------------

As of 28-03-2026 you need to put
```
[unstable]
json-target-spec = true
```
in your `$HOME/.cargo/config.toml` to get rust-analyzer to work properly on this project.
