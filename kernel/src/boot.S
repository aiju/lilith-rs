.intel_syntax noprefix

.set MULTIBOOT_MAGIC, 0x1BADB002
.set MULTIBOOT_FLAGS, 0x10001

.set ENTRY_PRESENT, 1<<0
.set ENTRY_WRITABLE, 1<<1
.set ENTRY_HUGE_PAGE, 1<<7

.set SEL_NULL, 0x00
.set SEL_CODE16, 0x08
.set SEL_DATA16, 0x10
.set SEL_CODE32, 0x18
.set SEL_CODE64, 0x20
.set SEL_DATA32, 0x28

// have to switch to at&t syntax bc of a GAS bug with this instruction
#define FAR_JUMP(sel, addr) \
    .att_syntax; \
    ljmp $sel, $addr; \
    .intel_syntax noprefix

.section .multiboot_header, "a"
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

.section .boot, "ax"
.global _start
.code32
_start:

    // make sure the direction flag is cleared
    cld

    // for the first section use a stack location in the first 64 KB bc we're switching to real mode
    mov esp, 0xf000

    // save location of multiboot info struct
    mov multiboot_info_ptr, ebx

    // copy real mode segment into its correct location
    call copy_real_mode_stub

    // load our own gdt
    lgdt [boot_gdt_ptr]

    // we do VESA mode setting before trying to enter long mode
    call vesa_modeset

    // set up very rudimentary page tables for early init
    // insert entries for PDPTs for the first and last 512GB of virtual memory, as well as the direct map
    mov edi, offset boot_pml4
    mov dword ptr [edi], ENTRY_PRESENT + ENTRY_WRITABLE + offset boot_pdpt_0
    mov dword ptr [edi+4], 0

    mov dword ptr [edi+8*256], ENTRY_PRESENT + ENTRY_WRITABLE + offset boot_pdpt_0
    mov dword ptr [edi+8*256+4], 0

    mov dword ptr [edi+8*511], ENTRY_PRESENT + ENTRY_WRITABLE + offset boot_pdpt_511
    mov dword ptr [edi+8*511+4], 0

    // identity map the first 4 GB
    mov edi, offset boot_pdpt_0
    mov dword ptr [edi+8*0], ENTRY_PRESENT + ENTRY_WRITABLE + ENTRY_HUGE_PAGE
    mov dword ptr [edi+8*0+4], 0

    mov dword ptr [edi+8*1], ENTRY_PRESENT + ENTRY_WRITABLE + ENTRY_HUGE_PAGE + 0x40000000
    mov dword ptr [edi+8*1+4], 0

    mov dword ptr [edi+8*2], ENTRY_PRESENT + ENTRY_WRITABLE + ENTRY_HUGE_PAGE + 0x80000000
    mov dword ptr [edi+8*2+4], 0

    mov dword ptr [edi+8*3], ENTRY_PRESENT + ENTRY_WRITABLE + ENTRY_HUGE_PAGE + 0xC0000000
    mov dword ptr [edi+8*3+4], 0

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

    // enter long mode!!!
    // load 64-bit descriptor into CS by jumping to the instruction following this one using a far jump
    // need to jump to a 32-bit address as a first step
    FAR_JUMP(SEL_CODE64, 1f)

.code64
1:
    // jump to the high address (0xFFFFFFFF8xxxxxxx)
    mov rdi, offset _start64
    jmp rdi

.section .real_mode_text, "ax"
// put this in the first 64KB so we can use it from real mode
boot_gdt_ptr:
    .word 8*6
    .long boot_gdt

#define GDT_ENTRY(base, limit, flags) \
    (((((base) >> 24) & 0xff) << 56) | \
     ((((limit) >> 16) & 0xf) << 48) | \
     ((((base) >> 16) & 0xff) << 32) | \
     (((base) & 0xffff) << 16) | \
     ((limit) & 0xffff) | \
     (flags))
#define GDT_PRESENT (1<<47 | 1<<40)
#define GDT_DPL(n) ((n&3)<<45)
#define GDT_CODE (3<<43)
#define GDT_DATA (2<<43)
#define GDT_RW (1<<41)
#define GDT_GRANULARITY (1<<55)
#define GDT_32 (1<<54)
#define GDT_CODE64 (1<<53)

boot_gdt:
    .quad GDT_ENTRY(0, 0, 0) /* null descriptor */
    .quad GDT_ENTRY(0, 0xFFFFF, GDT_PRESENT | GDT_DPL(0) | GDT_CODE | GDT_RW) /* 16-bit code descriptor */
    .quad GDT_ENTRY(0, 0xFFFFF, GDT_PRESENT | GDT_DPL(0) | GDT_DATA | GDT_RW) /* 16-bit data descriptor */
    .quad GDT_ENTRY(0, 0xFFFFF, GDT_PRESENT | GDT_DPL(0) | GDT_CODE | GDT_32 | GDT_RW | GDT_GRANULARITY) /* 32-bit code descriptor */
    .quad GDT_ENTRY(0, 0xFFFFF, GDT_PRESENT | GDT_DPL(0) | GDT_CODE | GDT_CODE64 | GDT_RW | GDT_GRANULARITY) /* 64-bit code descriptor */
    .quad GDT_ENTRY(0, 0xFFFFF, GDT_PRESENT | GDT_DPL(0) | GDT_DATA | GDT_32 | GDT_RW | GDT_GRANULARITY) /* 32/64-bit data descriptor */

.section .boot_reclaimable_phys, "a"
.align 4096
boot_pml4:
    .skip 4096
boot_pdpt_0:
    .skip 4096
boot_pdpt_511:
    .skip 4096

multiboot_info_ptr: .skip 4

.section .text
.global _start64
_start64:
    mov rsp, offset boot_stack_top
    mov edi, multiboot_info_ptr
    call early_init
    mov rsp, rax
    mov rdi, rax
    call main
    // call for stack alignment -- should never return

.section .boot_reclaimable, "a"
.align 16
.global boot_stack
boot_stack:
    .skip 256*1024
boot_stack_top:

.section .boot, "ax"
.global copy_real_mode_stub
.code32
copy_real_mode_stub:
    mov edi, offset __real_mode_text_start
    mov esi, offset __real_mode_text_start_phys
    cld
1:
    movsd
    cmp esi, offset __real_mode_text_end_phys
    jl 1b
    ret

.section .real_mode_text, "ax"
.global vesa_call
.code32
vesa_call:
    mov edi, [esp+4]
    mov ax, [edi]
    mov cx, [edi+2]
    mov dx, [edi+4]
    mov bx, [edi+6]
    mov si, [edi+8]
    mov di, [edi+10]
    push eax

    // change CS to 16-bit selector
    FAR_JUMP(SEL_CODE16, 1f)

.code16
1:
    // set data selectors to 16-bit
    mov ax, SEL_DATA16
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    // load real-mode IDT
    lidt [real_mode_idt_ptr]
    // disable protected-mode
    mov eax, cr0
    and eax, ~1
    mov cr0, eax
    // set CS to 0000
    FAR_JUMP(0x0000, 1f)
1:
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    pop eax
    int 0x10
    push eax

    // re-load GDT in case it was trashed by the BIOS
    lgdt [boot_gdt_ptr]

    // re-enable protected mode
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    // load 32-bit code selector
    FAR_JUMP(SEL_CODE32, 1f)

.code32
1:
    // load 32-bit data selector
    mov ax, SEL_DATA32
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    // not sure if BIOS preserves upper 16-bit
    mov eax, esp
    and eax, 0xffff
    mov esp, eax

    pop eax
    push edi

    mov edi, [esp+8]
    mov [edi], ax
    mov [edi+2], cx
    mov [edi+4], dx
    mov [edi+6], bx
    mov [edi+8], si
    pop eax
    mov [edi+10], ax

    ret

real_mode_idt_ptr:
    .word 0x3ff
    .word 0x0000