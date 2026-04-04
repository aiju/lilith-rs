// this compiles as 32-bit, which is why it's not rust code

#include <stdint.h>

#define BOOT __attribute__((section(".boot.vesa")))

typedef struct {
    uint16_t ax;
    uint16_t cx;
    uint16_t dx;
    uint16_t bx;
    uint16_t si;
    uint16_t di;
} VesaRegs;

extern void vesa_call(VesaRegs *);

typedef struct {
    char     VbeSignature[4];       // VBE Signature ('VESA')
    uint16_t VbeVersion;            // VBE Version
    uint32_t OemStringPtr;          // VbeFarPtr to OEM String
    uint8_t  Capabilities[4];       // Capabilities of graphics controller
    uint32_t VideoModePtr;          // VbeFarPtr to VideoModeList
    uint16_t TotalMemory;           // Number of 64kb memory blocks
    // Added for VBE 2.0+
    uint16_t OemSoftwareRev;        // VBE implementation software revision
    uint32_t OemVendorNamePtr;      // VbeFarPtr to Vendor Name String
    uint32_t OemProductNamePtr;     // VbeFarPtr to Product Name String
    uint32_t OemProductRevPtr;      // VbeFarPtr to Product Revision String
    uint8_t  Reserved[222];         // Reserved for VBE implementation scratch area
    uint8_t  OemData[256];          // Data Area for OEM Strings
} __attribute__((packed)) VbeInfoBlock;

typedef struct {
    // Mandatory information for all VBE revisions
    uint16_t ModeAttributes;
    uint8_t  WinAAttributes;
    uint8_t  WinBAttributes;
    uint16_t WinGranularity;
    uint16_t WinSize;
    uint16_t WinASegment;
    uint16_t WinBSegment;
    uint32_t WinFuncPtr;
    uint16_t BytesPerScanLine;

    // Mandatory information for VBE 1.2 and above
    uint16_t XResolution;
    uint16_t YResolution;
    uint8_t  XCharSize;
    uint8_t  YCharSize;
    uint8_t  NumberOfPlanes;
    uint8_t  BitsPerPixel;
    uint8_t  NumberOfBanks;
    uint8_t  MemoryModel;
    uint8_t  BankSize;
    uint8_t  NumberOfImagePages;
    uint8_t  Reserved0;             // reserved for page function

    // Direct Color fields (required for direct/6 and YUV/7 memory models)
    uint8_t  RedMaskSize;
    uint8_t  RedFieldPosition;
    uint8_t  GreenMaskSize;
    uint8_t  GreenFieldPosition;
    uint8_t  BlueMaskSize;
    uint8_t  BlueFieldPosition;
    uint8_t  RsvdMaskSize;
    uint8_t  RsvdFieldPosition;
    uint8_t  DirectColorModeInfo;

    // Mandatory information for VBE 2.0 and above
    uint32_t PhysBasePtr;
    uint32_t Reserved1;             // always 0
    uint16_t Reserved2;             // always 0

    // Mandatory information for VBE 3.0 and above
    uint16_t LinBytesPerScanLine;
    uint8_t  BnkNumberOfImagePages;
    uint8_t  LinNumberOfImagePages;
    uint8_t  LinRedMaskSize;
    uint8_t  LinRedFieldPosition;
    uint8_t  LinGreenMaskSize;
    uint8_t  LinGreenFieldPosition;
    uint8_t  LinBlueMaskSize;
    uint8_t  LinBlueFieldPosition;
    uint8_t  LinRsvdMaskSize;
    uint8_t  LinRsvdFieldPosition;
    uint32_t MaxPixelClock;

    uint8_t  Reserved3[190];        // remainder of ModeInfoBlock
} __attribute__((packed)) ModeInfoBlock;

__attribute__((section(".real_mode_bss")))
VbeInfoBlock vesa_vbe_info_block;
__attribute__((section(".real_mode_bss")))
ModeInfoBlock vesa_mode_info_block;

#define VBE_MODE_SUPPORTED          (1 << 0)
#define VBE_MODE_TTY_OUTPUT         (1 << 2)
#define VBE_MODE_COLOR              (1 << 3)
#define VBE_MODE_GRAPHICS           (1 << 4)
#define VBE_MODE_NOT_VGA_COMPATIBLE (1 << 5)
#define VBE_MODE_NO_WINDOWED        (1 << 6)
#define VBE_MODE_LINEAR_FB          (1 << 7)
#define VBE_MODE_DOUBLE_SCAN        (1 << 8)
#define VBE_MODE_INTERLACED         (1 << 9)
#define VBE_MODE_TRIPLE_BUFFER      (1 << 10)
#define VBE_MODE_STEREO             (1 << 11)
#define VBE_MODE_DUAL_DISPLAY       (1 << 12)

__attribute__((section(".boot_reclaimable_phys")))
static int print_idx;

#define STR(s) ({ \
    static const char __str[] __attribute__((section(".boot.rodata"))) = (s); \
    (const char *)__str; \
})

BOOT static void *from_far_ptr(uint32_t far_ptr) {
    return (void*)(((far_ptr >> 16) << 4) + (far_ptr & 0xffff));
}

BOOT static void print(const char *str) {
    for(const char *p = str; *p != 0; p++) {
        if(print_idx < 80*25) {
            if(*p == '\n')
                print_idx += 80 - (print_idx % 80);
            else
                ((uint16_t*)0xb8000)[print_idx++] = 0x0700 | *p;
        }
    }
}

BOOT static void print_hex(uint32_t word) {
    char buf[9];

    for(int i = 0; i < 8; i++) {
        buf[i] = (word >> (28 - i * 4)) & 0xf;
        if(buf[i] > 9) buf[i] += 'a' - 10;
        else buf[i] += '0';
    }
    buf[8] = 0;
    print(buf);
}

BOOT static void print_regs(VesaRegs *regs) {
    print(STR("AX "));
    print_hex(regs->ax);
    print(STR(" CX "));
    print_hex(regs->cx);
    print(STR(" DX "));
    print_hex(regs->dx);
    print(STR(" BX "));
    print_hex(regs->bx);
    print(STR(" SI "));
    print_hex(regs->si);
    print(STR(" DI "));
    print_hex(regs->di);
    print(STR("\n"));
}

BOOT static void vesa_fail(const char *message, VesaRegs *regs) {
    print(STR("VESA MODESETTING FAILED: "));
    print(message);
    print(STR("\n"));
    if(regs != 0)
        print_regs(regs);
    print(STR("\n"));
    for(;;) asm("hlt");
}

BOOT void vesa_modeset() {
    print_idx = 0;
    for(int i = 0; i < 80*25; i++)
        ((uint16_t*)0xb8000)[i] = 0;

    VesaRegs regs;
    VbeInfoBlock *info = &vesa_vbe_info_block;
    ModeInfoBlock *mode = &vesa_mode_info_block;

    regs.ax = 0x4f00;
    regs.di = (uint32_t) info;
    vesa_call(&regs);
    if(regs.ax != 0x004f)
        vesa_fail(STR("Return VBE controller information failed"), &regs);


    uint16_t *video_mode_ptr = from_far_ptr(info->VideoModePtr);
    for(; *video_mode_ptr != 0xffff; video_mode_ptr++){
        regs.ax = 0x4f01;
        regs.cx = *video_mode_ptr;
        regs.di = (uint32_t) mode;
        vesa_call(&regs);
        if(regs.ax != 0x004f)
            vesa_fail(STR("Return VBE mode information failed"), &regs);
        

        if(~(mode->ModeAttributes | ~(VBE_MODE_SUPPORTED | VBE_MODE_COLOR | VBE_MODE_GRAPHICS | VBE_MODE_LINEAR_FB)) != 0)
            continue;

        if(mode->MemoryModel != 0x06)
            continue;

        if(mode->XResolution == 1024 && mode->YResolution == 768 && mode->BitsPerPixel == 32)
            break;
    }

    regs.ax = 0x4f02;
    regs.bx = *video_mode_ptr | (1<<14);
    vesa_call(&regs);
    if(regs.ax != 0x004f)
        vesa_fail(STR("Set VBE Mode failed"), &regs);
}
