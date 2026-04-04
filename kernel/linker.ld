ENTRY(_start)

LOAD_ADDRESS = 0x100000;
KERNEL_OFFSET = 0xFFFFFFFF80100000;
REAL_MODE_LOAD_ADDRESS = 0x1000;

SECTIONS {
    . = LOAD_ADDRESS;
    .boot : {
        __text_start = . - LOAD_ADDRESS + KERNEL_OFFSET;
        __text_start_phys = .;
        KEEP(*(.multiboot_header))
        *(.boot)
        *(.boot.*)
    }

    . = KERNEL_OFFSET + (. - LOAD_ADDRESS);

    .text : AT(ADDR(.text) - KERNEL_OFFSET + LOAD_ADDRESS) {
        *(.text .text.*)
    }

    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_OFFSET + LOAD_ADDRESS) {
        *(.rodata .rodata.*)
        *(.eh_frame_hdr)
        *(.eh_frame)
    }

    __real_mode_text_start_phys = ALIGN(4K) - KERNEL_OFFSET + LOAD_ADDRESS;
    .real_mode_text REAL_MODE_LOAD_ADDRESS : AT(__real_mode_text_start_phys) {
        __real_mode_text_start = .;
        *(.real_mode_text)
    }

    __real_mode_text_end_phys = __real_mode_text_start_phys + SIZEOF(.real_mode_text);

    .real_mode_bss ALIGN(4) (NOLOAD) : {
        *(.real_mode_bss)
    }

    . = KERNEL_OFFSET + (__real_mode_text_start_phys - LOAD_ADDRESS) + SIZEOF(.real_mode_text);

    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_OFFSET + LOAD_ADDRESS) {
        __data_start = .;
        __data_start_phys = . - KERNEL_OFFSET + LOAD_ADDRESS;
        *(.data.rel.ro .data.rel.ro.*)
        *(.got .got.*)
        *(.data .data.*)
        __data_end = .;
        __data_end_phys = . - KERNEL_OFFSET + LOAD_ADDRESS;
    }

    .bss (NOLOAD) : AT(ADDR(.bss) - KERNEL_OFFSET + LOAD_ADDRESS) {
        __bss_start = .;
        *(.bss .bss.*)
        . = ALIGN(4K);
        __reclaimable_start_phys = LOAD_ADDRESS + (. - KERNEL_OFFSET);
        *(.boot_reclaimable);
    }

    . = LOAD_ADDRESS + (. - KERNEL_OFFSET);

    .boot_reclaimable_phys (NOLOAD) : AT(ADDR(.boot_reclaimable_phys)) {
        *(.boot_reclaimable_phys)
        __reclaimable_end_phys = .;
    }

    . = KERNEL_OFFSET + (. - LOAD_ADDRESS);
    __bss_end = .;
    __bss_end_phys = . - KERNEL_OFFSET + LOAD_ADDRESS;
}