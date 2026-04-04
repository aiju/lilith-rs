.intel_syntax noprefix

.set MULTIBOOT_MAGIC, 0x1BADB002
.set MULTIBOOT_FLAGS, 0x10001

.set ENTRY_PRESENT, 1<<0
.set ENTRY_WRITABLE, 1<<1
.set ENTRY_HUGE_PAGE, 1<<7


.section .boot, "ax"
.align 4
.global multiboot_header
multiboot_header:
    .long MULTIBOOT_MAGIC      /* magic */
    .long MULTIBOOT_FLAGS      /* flags */
    .long -(MULTIBOOT_MAGIC + MULTIBOOT_FLAGS)  /* checksum */
    .long __text_start_phys
    .long __text_start_phys
    .long __data_end_phys
    .long __bss_end_phys
    .long _start

.global _start
.code32
_start:

    // make sure the direction flag is cleared
    cld

    // insert entries for PDPTs for the first and last 512GB of virtual memory
    mov edi, offset boot_pml4
    mov dword ptr [edi], ENTRY_PRESENT + ENTRY_WRITABLE + offset boot_pdpt_0
    mov dword ptr [edi+4], 0

    mov dword ptr [edi+8*256], ENTRY_PRESENT + ENTRY_WRITABLE + offset boot_pdpt_0
    mov dword ptr [edi+8*256+4], 0

    mov dword ptr [edi+8*511], ENTRY_PRESENT + ENTRY_WRITABLE + offset boot_pdpt_511
    mov dword ptr [edi+8*511+4], 0

    // identity map the first GB
    mov edi, offset boot_pdpt_0
    mov dword ptr [edi], ENTRY_PRESENT + ENTRY_WRITABLE + ENTRY_HUGE_PAGE
    mov dword ptr [edi+4], 0

    // also map the first GB at 0xFFFFFFFF_80000000
    mov edi, offset boot_pdpt_511
    mov dword ptr [edi + 510*8], ENTRY_PRESENT + ENTRY_WRITABLE + ENTRY_HUGE_PAGE
    mov dword ptr [edi + 510*8 + 4], 0

    // tell the CPU about our PML4 location
    mov edi, offset boot_pml4
    mov cr3, edi

    // enable PAE
    mov eax, cr4
    or eax, 1<<5
    mov cr4, eax

    // activate cpu features: long mode, NX bit, SYSCALL instruction
    mov ecx, 0xc0000080
    rdmsr
    or eax, 1<<0 | 1<<8 | 1<<11
    wrmsr

    // activate paging 
    mov eax, cr0
    or eax, 1<<31
    mov cr0, eax

    // load boot
    lgdt [boot_gdt_ptr]

    // enter long mode!!!
    // load 64-bit descriptor into CS by jumping to the instruction following this one using a far jump
    // (have to switch to at&t syntax bc of a GAS bug with this instruction)
.att_syntax
    ljmp $0x08, $1f
.intel_syntax noprefix

.code64
1:
    mov rdi, offset _start64
    jmp rdi

boot_gdt_ptr:
    .word 8*3
    .long boot_gdt

boot_gdt:
    .long 0x00000000, 0x00000000 /* null descriptor */
    .long 0x0000FFFF, 0x00AF9B00 /* 64-bit code descriptor */
    .long 0x0000FFFF, 0x00CF9300 /* 64-bit data descriptor */

.section .boot_reclaimable_phys, "a"
.align 4096
boot_pml4:
    .skip 4096
boot_pdpt_0:
    .skip 4096
boot_pdpt_511:
    .skip 4096

.section .text
.global _start64
_start64:
    mov rsp, offset boot_stack_top
    mov edi, ebx
    call early_init
    mov rsp, 0xFFFFFFFFA0000000
    call main
    // call for stack alignment -- should never return

.section .boot_reclaimable, "a"
.align 16
.global boot_stack
boot_stack:
    .skip 256*1024
boot_stack_top:
