use core::arch::naked_asm;
use x86_64::{VirtAddr, structures::idt::InterruptDescriptorTable};

use crate::interrupts::{DOUBLE_FAULT_IST_INDEX, int_common_entry};

macro_rules! int_entry {
    ($sym:ident, $num:expr, false) => {
        #[unsafe(naked)]
        extern "C" fn $sym() {
            naked_asm!(
                "push 0",            // fake error code
                "push {n}",          // interrupt number
                "jmp {common}",
                n = const $num,
                common = sym int_common_entry,
            )
        }
    };
    ($sym:ident, $num:expr, true) => {
        #[unsafe(naked)]
        extern "C" fn $sym() {
            naked_asm!(
                // CPU already pushed error code
                "push {n}",          // interrupt number
                "jmp {common}",
                n = const $num,
                common = sym int_common_entry,
            )
        }
    };
}

int_entry!(divide_error_entry, 0, false);
int_entry!(debug_entry, 1, false);
int_entry!(non_maskable_interrupt_entry, 2, false);
int_entry!(breakpoint_entry, 3, false);
int_entry!(overflow_entry, 4, false);
int_entry!(bound_range_exceeded_entry, 5, false);
int_entry!(invalid_opcode_entry, 6, false);
int_entry!(device_not_available_entry, 7, false);
int_entry!(double_fault_entry, 8, true);
// 9: coprocessor segment overrun (unused)
int_entry!(invalid_tss_entry, 10, true);
int_entry!(segment_not_present_entry, 11, true);
int_entry!(stack_segment_fault_entry, 12, true);
int_entry!(general_protection_fault_entry, 13, true);
int_entry!(page_fault_entry, 14, true);
// 15: reserved
int_entry!(x87_floating_point_entry, 16, false);
int_entry!(alignment_check_entry, 17, true);
int_entry!(machine_check_entry, 18, false);
int_entry!(simd_floating_point_entry, 19, false);
int_entry!(virtualization_entry, 20, false);
int_entry!(cp_protection_exception_entry, 21, true);
// 22-27: reserved
int_entry!(hv_injection_exception_entry, 28, false);
int_entry!(vmm_communication_exception_entry, 29, true);
int_entry!(security_exception_entry, 30, true);
// 31: reserved
int_entry!(irq0_entry, 32, false);
int_entry!(irq1_entry, 33, false);
int_entry!(irq2_entry, 34, false);
int_entry!(irq3_entry, 35, false);
int_entry!(irq4_entry, 36, false);
int_entry!(irq5_entry, 37, false);
int_entry!(irq6_entry, 38, false);
int_entry!(irq7_entry, 39, false);
int_entry!(irq8_entry, 40, false);
int_entry!(irq9_entry, 41, false);
int_entry!(irq10_entry, 42, false);
int_entry!(irq11_entry, 43, false);
int_entry!(irq12_entry, 44, false);
int_entry!(irq13_entry, 45, false);
int_entry!(irq14_entry, 46, false);
int_entry!(irq15_entry, 47, false);
int_entry!(irq16_entry, 48, false);
int_entry!(irq17_entry, 49, false);
int_entry!(irq18_entry, 50, false);
int_entry!(irq19_entry, 51, false);
int_entry!(irq20_entry, 52, false);
int_entry!(irq21_entry, 53, false);
int_entry!(irq22_entry, 54, false);
int_entry!(irq23_entry, 55, false);
int_entry!(irq24_entry, 56, false);
int_entry!(irq25_entry, 57, false);
int_entry!(irq26_entry, 58, false);
int_entry!(irq27_entry, 59, false);
int_entry!(irq28_entry, 60, false);
int_entry!(irq29_entry, 61, false);
int_entry!(irq30_entry, 62, false);
int_entry!(irq31_entry, 63, false);
int_entry!(irq32_entry, 64, false);
int_entry!(irq33_entry, 65, false);
int_entry!(irq34_entry, 66, false);
int_entry!(irq35_entry, 67, false);
int_entry!(irq36_entry, 68, false);
int_entry!(irq37_entry, 69, false);
int_entry!(irq38_entry, 70, false);
int_entry!(irq39_entry, 71, false);
int_entry!(irq40_entry, 72, false);
int_entry!(irq41_entry, 73, false);
int_entry!(irq42_entry, 74, false);
int_entry!(irq43_entry, 75, false);
int_entry!(irq44_entry, 76, false);
int_entry!(irq45_entry, 77, false);
int_entry!(irq46_entry, 78, false);
int_entry!(irq47_entry, 79, false);
int_entry!(irq48_entry, 80, false);
int_entry!(irq49_entry, 81, false);
int_entry!(irq50_entry, 82, false);
int_entry!(irq51_entry, 83, false);
int_entry!(irq52_entry, 84, false);
int_entry!(irq53_entry, 85, false);
int_entry!(irq54_entry, 86, false);
int_entry!(irq55_entry, 87, false);
int_entry!(irq56_entry, 88, false);
int_entry!(irq57_entry, 89, false);
int_entry!(irq58_entry, 90, false);
int_entry!(irq59_entry, 91, false);
int_entry!(irq60_entry, 92, false);
int_entry!(irq61_entry, 93, false);
int_entry!(irq62_entry, 94, false);
int_entry!(irq63_entry, 95, false);
int_entry!(irq64_entry, 96, false);
int_entry!(irq65_entry, 97, false);
int_entry!(irq66_entry, 98, false);
int_entry!(irq67_entry, 99, false);
int_entry!(irq68_entry, 100, false);
int_entry!(irq69_entry, 101, false);
int_entry!(irq70_entry, 102, false);
int_entry!(irq71_entry, 103, false);
int_entry!(irq72_entry, 104, false);
int_entry!(irq73_entry, 105, false);
int_entry!(irq74_entry, 106, false);
int_entry!(irq75_entry, 107, false);
int_entry!(irq76_entry, 108, false);
int_entry!(irq77_entry, 109, false);
int_entry!(irq78_entry, 110, false);
int_entry!(irq79_entry, 111, false);
int_entry!(irq80_entry, 112, false);
int_entry!(irq81_entry, 113, false);
int_entry!(irq82_entry, 114, false);
int_entry!(irq83_entry, 115, false);
int_entry!(irq84_entry, 116, false);
int_entry!(irq85_entry, 117, false);
int_entry!(irq86_entry, 118, false);
int_entry!(irq87_entry, 119, false);
int_entry!(irq88_entry, 120, false);
int_entry!(irq89_entry, 121, false);
int_entry!(irq90_entry, 122, false);
int_entry!(irq91_entry, 123, false);
int_entry!(irq92_entry, 124, false);
int_entry!(irq93_entry, 125, false);
int_entry!(irq94_entry, 126, false);
int_entry!(irq95_entry, 127, false);
int_entry!(irq96_entry, 128, false);
int_entry!(irq97_entry, 129, false);
int_entry!(irq98_entry, 130, false);
int_entry!(irq99_entry, 131, false);
int_entry!(irq100_entry, 132, false);
int_entry!(irq101_entry, 133, false);
int_entry!(irq102_entry, 134, false);
int_entry!(irq103_entry, 135, false);
int_entry!(irq104_entry, 136, false);
int_entry!(irq105_entry, 137, false);
int_entry!(irq106_entry, 138, false);
int_entry!(irq107_entry, 139, false);
int_entry!(irq108_entry, 140, false);
int_entry!(irq109_entry, 141, false);
int_entry!(irq110_entry, 142, false);
int_entry!(irq111_entry, 143, false);
int_entry!(irq112_entry, 144, false);
int_entry!(irq113_entry, 145, false);
int_entry!(irq114_entry, 146, false);
int_entry!(irq115_entry, 147, false);
int_entry!(irq116_entry, 148, false);
int_entry!(irq117_entry, 149, false);
int_entry!(irq118_entry, 150, false);
int_entry!(irq119_entry, 151, false);
int_entry!(irq120_entry, 152, false);
int_entry!(irq121_entry, 153, false);
int_entry!(irq122_entry, 154, false);
int_entry!(irq123_entry, 155, false);
int_entry!(irq124_entry, 156, false);
int_entry!(irq125_entry, 157, false);
int_entry!(irq126_entry, 158, false);
int_entry!(irq127_entry, 159, false);
int_entry!(irq128_entry, 160, false);
int_entry!(irq129_entry, 161, false);
int_entry!(irq130_entry, 162, false);
int_entry!(irq131_entry, 163, false);
int_entry!(irq132_entry, 164, false);
int_entry!(irq133_entry, 165, false);
int_entry!(irq134_entry, 166, false);
int_entry!(irq135_entry, 167, false);
int_entry!(irq136_entry, 168, false);
int_entry!(irq137_entry, 169, false);
int_entry!(irq138_entry, 170, false);
int_entry!(irq139_entry, 171, false);
int_entry!(irq140_entry, 172, false);
int_entry!(irq141_entry, 173, false);
int_entry!(irq142_entry, 174, false);
int_entry!(irq143_entry, 175, false);
int_entry!(irq144_entry, 176, false);
int_entry!(irq145_entry, 177, false);
int_entry!(irq146_entry, 178, false);
int_entry!(irq147_entry, 179, false);
int_entry!(irq148_entry, 180, false);
int_entry!(irq149_entry, 181, false);
int_entry!(irq150_entry, 182, false);
int_entry!(irq151_entry, 183, false);
int_entry!(irq152_entry, 184, false);
int_entry!(irq153_entry, 185, false);
int_entry!(irq154_entry, 186, false);
int_entry!(irq155_entry, 187, false);
int_entry!(irq156_entry, 188, false);
int_entry!(irq157_entry, 189, false);
int_entry!(irq158_entry, 190, false);
int_entry!(irq159_entry, 191, false);
int_entry!(irq160_entry, 192, false);
int_entry!(irq161_entry, 193, false);
int_entry!(irq162_entry, 194, false);
int_entry!(irq163_entry, 195, false);
int_entry!(irq164_entry, 196, false);
int_entry!(irq165_entry, 197, false);
int_entry!(irq166_entry, 198, false);
int_entry!(irq167_entry, 199, false);
int_entry!(irq168_entry, 200, false);
int_entry!(irq169_entry, 201, false);
int_entry!(irq170_entry, 202, false);
int_entry!(irq171_entry, 203, false);
int_entry!(irq172_entry, 204, false);
int_entry!(irq173_entry, 205, false);
int_entry!(irq174_entry, 206, false);
int_entry!(irq175_entry, 207, false);
int_entry!(irq176_entry, 208, false);
int_entry!(irq177_entry, 209, false);
int_entry!(irq178_entry, 210, false);
int_entry!(irq179_entry, 211, false);
int_entry!(irq180_entry, 212, false);
int_entry!(irq181_entry, 213, false);
int_entry!(irq182_entry, 214, false);
int_entry!(irq183_entry, 215, false);
int_entry!(irq184_entry, 216, false);
int_entry!(irq185_entry, 217, false);
int_entry!(irq186_entry, 218, false);
int_entry!(irq187_entry, 219, false);
int_entry!(irq188_entry, 220, false);
int_entry!(irq189_entry, 221, false);
int_entry!(irq190_entry, 222, false);
int_entry!(irq191_entry, 223, false);
int_entry!(irq192_entry, 224, false);
int_entry!(irq193_entry, 225, false);
int_entry!(irq194_entry, 226, false);
int_entry!(irq195_entry, 227, false);
int_entry!(irq196_entry, 228, false);
int_entry!(irq197_entry, 229, false);
int_entry!(irq198_entry, 230, false);
int_entry!(irq199_entry, 231, false);
int_entry!(irq200_entry, 232, false);
int_entry!(irq201_entry, 233, false);
int_entry!(irq202_entry, 234, false);
int_entry!(irq203_entry, 235, false);
int_entry!(irq204_entry, 236, false);
int_entry!(irq205_entry, 237, false);
int_entry!(irq206_entry, 238, false);
int_entry!(irq207_entry, 239, false);
int_entry!(irq208_entry, 240, false);
int_entry!(irq209_entry, 241, false);
int_entry!(irq210_entry, 242, false);
int_entry!(irq211_entry, 243, false);
int_entry!(irq212_entry, 244, false);
int_entry!(irq213_entry, 245, false);
int_entry!(irq214_entry, 246, false);
int_entry!(irq215_entry, 247, false);
int_entry!(irq216_entry, 248, false);
int_entry!(irq217_entry, 249, false);
int_entry!(irq218_entry, 250, false);
int_entry!(irq219_entry, 251, false);
int_entry!(irq220_entry, 252, false);
int_entry!(irq221_entry, 253, false);
int_entry!(irq222_entry, 254, false);
int_entry!(irq223_entry, 255, false);

fn to_addr(f: extern "C" fn()) -> VirtAddr {
    VirtAddr::from_ptr(f as *const ())
}

pub(super) fn fill_idt(idt: &mut InterruptDescriptorTable) {
    unsafe {
        idt.divide_error
            .set_handler_addr(to_addr(divide_error_entry));
        idt.debug.set_handler_addr(to_addr(debug_entry));
        idt.non_maskable_interrupt
            .set_handler_addr(to_addr(non_maskable_interrupt_entry));
        idt.breakpoint.set_handler_addr(to_addr(breakpoint_entry));
        idt.overflow.set_handler_addr(to_addr(overflow_entry));
        idt.bound_range_exceeded
            .set_handler_addr(to_addr(bound_range_exceeded_entry));
        idt.invalid_opcode
            .set_handler_addr(to_addr(invalid_opcode_entry));
        idt.device_not_available
            .set_handler_addr(to_addr(device_not_available_entry));
        idt.double_fault
            .set_handler_addr(to_addr(double_fault_entry))
            .set_stack_index(DOUBLE_FAULT_IST_INDEX);
        idt.invalid_tss.set_handler_addr(to_addr(invalid_tss_entry));
        idt.segment_not_present
            .set_handler_addr(to_addr(segment_not_present_entry));
        idt.stack_segment_fault
            .set_handler_addr(to_addr(stack_segment_fault_entry));
        idt.general_protection_fault
            .set_handler_addr(to_addr(general_protection_fault_entry));
        idt.page_fault.set_handler_addr(to_addr(page_fault_entry));
        // 15: reserved
        idt.x87_floating_point
            .set_handler_addr(to_addr(x87_floating_point_entry));
        idt.alignment_check
            .set_handler_addr(to_addr(alignment_check_entry));
        idt.machine_check
            .set_handler_addr(to_addr(machine_check_entry));
        idt.simd_floating_point
            .set_handler_addr(to_addr(simd_floating_point_entry));
        idt.virtualization
            .set_handler_addr(to_addr(virtualization_entry));
        idt.cp_protection_exception
            .set_handler_addr(to_addr(cp_protection_exception_entry));
        // 22-27: reserved
        idt.hv_injection_exception
            .set_handler_addr(to_addr(hv_injection_exception_entry));
        idt.vmm_communication_exception
            .set_handler_addr(to_addr(vmm_communication_exception_entry));
        idt.security_exception
            .set_handler_addr(to_addr(security_exception_entry));
        // 31: reserved
        idt[32].set_handler_addr(to_addr(irq0_entry));
        idt[33].set_handler_addr(to_addr(irq1_entry));
        idt[34].set_handler_addr(to_addr(irq2_entry));
        idt[35].set_handler_addr(to_addr(irq3_entry));
        idt[36].set_handler_addr(to_addr(irq4_entry));
        idt[37].set_handler_addr(to_addr(irq5_entry));
        idt[38].set_handler_addr(to_addr(irq6_entry));
        idt[39].set_handler_addr(to_addr(irq7_entry));
        idt[40].set_handler_addr(to_addr(irq8_entry));
        idt[41].set_handler_addr(to_addr(irq9_entry));
        idt[42].set_handler_addr(to_addr(irq10_entry));
        idt[43].set_handler_addr(to_addr(irq11_entry));
        idt[44].set_handler_addr(to_addr(irq12_entry));
        idt[45].set_handler_addr(to_addr(irq13_entry));
        idt[46].set_handler_addr(to_addr(irq14_entry));
        idt[47].set_handler_addr(to_addr(irq15_entry));
        idt[48].set_handler_addr(to_addr(irq16_entry));
        idt[49].set_handler_addr(to_addr(irq17_entry));
        idt[50].set_handler_addr(to_addr(irq18_entry));
        idt[51].set_handler_addr(to_addr(irq19_entry));
        idt[52].set_handler_addr(to_addr(irq20_entry));
        idt[53].set_handler_addr(to_addr(irq21_entry));
        idt[54].set_handler_addr(to_addr(irq22_entry));
        idt[55].set_handler_addr(to_addr(irq23_entry));
        idt[56].set_handler_addr(to_addr(irq24_entry));
        idt[57].set_handler_addr(to_addr(irq25_entry));
        idt[58].set_handler_addr(to_addr(irq26_entry));
        idt[59].set_handler_addr(to_addr(irq27_entry));
        idt[60].set_handler_addr(to_addr(irq28_entry));
        idt[61].set_handler_addr(to_addr(irq29_entry));
        idt[62].set_handler_addr(to_addr(irq30_entry));
        idt[63].set_handler_addr(to_addr(irq31_entry));
        idt[64].set_handler_addr(to_addr(irq32_entry));
        idt[65].set_handler_addr(to_addr(irq33_entry));
        idt[66].set_handler_addr(to_addr(irq34_entry));
        idt[67].set_handler_addr(to_addr(irq35_entry));
        idt[68].set_handler_addr(to_addr(irq36_entry));
        idt[69].set_handler_addr(to_addr(irq37_entry));
        idt[70].set_handler_addr(to_addr(irq38_entry));
        idt[71].set_handler_addr(to_addr(irq39_entry));
        idt[72].set_handler_addr(to_addr(irq40_entry));
        idt[73].set_handler_addr(to_addr(irq41_entry));
        idt[74].set_handler_addr(to_addr(irq42_entry));
        idt[75].set_handler_addr(to_addr(irq43_entry));
        idt[76].set_handler_addr(to_addr(irq44_entry));
        idt[77].set_handler_addr(to_addr(irq45_entry));
        idt[78].set_handler_addr(to_addr(irq46_entry));
        idt[79].set_handler_addr(to_addr(irq47_entry));
        idt[80].set_handler_addr(to_addr(irq48_entry));
        idt[81].set_handler_addr(to_addr(irq49_entry));
        idt[82].set_handler_addr(to_addr(irq50_entry));
        idt[83].set_handler_addr(to_addr(irq51_entry));
        idt[84].set_handler_addr(to_addr(irq52_entry));
        idt[85].set_handler_addr(to_addr(irq53_entry));
        idt[86].set_handler_addr(to_addr(irq54_entry));
        idt[87].set_handler_addr(to_addr(irq55_entry));
        idt[88].set_handler_addr(to_addr(irq56_entry));
        idt[89].set_handler_addr(to_addr(irq57_entry));
        idt[90].set_handler_addr(to_addr(irq58_entry));
        idt[91].set_handler_addr(to_addr(irq59_entry));
        idt[92].set_handler_addr(to_addr(irq60_entry));
        idt[93].set_handler_addr(to_addr(irq61_entry));
        idt[94].set_handler_addr(to_addr(irq62_entry));
        idt[95].set_handler_addr(to_addr(irq63_entry));
        idt[96].set_handler_addr(to_addr(irq64_entry));
        idt[97].set_handler_addr(to_addr(irq65_entry));
        idt[98].set_handler_addr(to_addr(irq66_entry));
        idt[99].set_handler_addr(to_addr(irq67_entry));
        idt[100].set_handler_addr(to_addr(irq68_entry));
        idt[101].set_handler_addr(to_addr(irq69_entry));
        idt[102].set_handler_addr(to_addr(irq70_entry));
        idt[103].set_handler_addr(to_addr(irq71_entry));
        idt[104].set_handler_addr(to_addr(irq72_entry));
        idt[105].set_handler_addr(to_addr(irq73_entry));
        idt[106].set_handler_addr(to_addr(irq74_entry));
        idt[107].set_handler_addr(to_addr(irq75_entry));
        idt[108].set_handler_addr(to_addr(irq76_entry));
        idt[109].set_handler_addr(to_addr(irq77_entry));
        idt[110].set_handler_addr(to_addr(irq78_entry));
        idt[111].set_handler_addr(to_addr(irq79_entry));
        idt[112].set_handler_addr(to_addr(irq80_entry));
        idt[113].set_handler_addr(to_addr(irq81_entry));
        idt[114].set_handler_addr(to_addr(irq82_entry));
        idt[115].set_handler_addr(to_addr(irq83_entry));
        idt[116].set_handler_addr(to_addr(irq84_entry));
        idt[117].set_handler_addr(to_addr(irq85_entry));
        idt[118].set_handler_addr(to_addr(irq86_entry));
        idt[119].set_handler_addr(to_addr(irq87_entry));
        idt[120].set_handler_addr(to_addr(irq88_entry));
        idt[121].set_handler_addr(to_addr(irq89_entry));
        idt[122].set_handler_addr(to_addr(irq90_entry));
        idt[123].set_handler_addr(to_addr(irq91_entry));
        idt[124].set_handler_addr(to_addr(irq92_entry));
        idt[125].set_handler_addr(to_addr(irq93_entry));
        idt[126].set_handler_addr(to_addr(irq94_entry));
        idt[127].set_handler_addr(to_addr(irq95_entry));
        idt[128].set_handler_addr(to_addr(irq96_entry));
        idt[129].set_handler_addr(to_addr(irq97_entry));
        idt[130].set_handler_addr(to_addr(irq98_entry));
        idt[131].set_handler_addr(to_addr(irq99_entry));
        idt[132].set_handler_addr(to_addr(irq100_entry));
        idt[133].set_handler_addr(to_addr(irq101_entry));
        idt[134].set_handler_addr(to_addr(irq102_entry));
        idt[135].set_handler_addr(to_addr(irq103_entry));
        idt[136].set_handler_addr(to_addr(irq104_entry));
        idt[137].set_handler_addr(to_addr(irq105_entry));
        idt[138].set_handler_addr(to_addr(irq106_entry));
        idt[139].set_handler_addr(to_addr(irq107_entry));
        idt[140].set_handler_addr(to_addr(irq108_entry));
        idt[141].set_handler_addr(to_addr(irq109_entry));
        idt[142].set_handler_addr(to_addr(irq110_entry));
        idt[143].set_handler_addr(to_addr(irq111_entry));
        idt[144].set_handler_addr(to_addr(irq112_entry));
        idt[145].set_handler_addr(to_addr(irq113_entry));
        idt[146].set_handler_addr(to_addr(irq114_entry));
        idt[147].set_handler_addr(to_addr(irq115_entry));
        idt[148].set_handler_addr(to_addr(irq116_entry));
        idt[149].set_handler_addr(to_addr(irq117_entry));
        idt[150].set_handler_addr(to_addr(irq118_entry));
        idt[151].set_handler_addr(to_addr(irq119_entry));
        idt[152].set_handler_addr(to_addr(irq120_entry));
        idt[153].set_handler_addr(to_addr(irq121_entry));
        idt[154].set_handler_addr(to_addr(irq122_entry));
        idt[155].set_handler_addr(to_addr(irq123_entry));
        idt[156].set_handler_addr(to_addr(irq124_entry));
        idt[157].set_handler_addr(to_addr(irq125_entry));
        idt[158].set_handler_addr(to_addr(irq126_entry));
        idt[159].set_handler_addr(to_addr(irq127_entry));
        idt[160].set_handler_addr(to_addr(irq128_entry));
        idt[161].set_handler_addr(to_addr(irq129_entry));
        idt[162].set_handler_addr(to_addr(irq130_entry));
        idt[163].set_handler_addr(to_addr(irq131_entry));
        idt[164].set_handler_addr(to_addr(irq132_entry));
        idt[165].set_handler_addr(to_addr(irq133_entry));
        idt[166].set_handler_addr(to_addr(irq134_entry));
        idt[167].set_handler_addr(to_addr(irq135_entry));
        idt[168].set_handler_addr(to_addr(irq136_entry));
        idt[169].set_handler_addr(to_addr(irq137_entry));
        idt[170].set_handler_addr(to_addr(irq138_entry));
        idt[171].set_handler_addr(to_addr(irq139_entry));
        idt[172].set_handler_addr(to_addr(irq140_entry));
        idt[173].set_handler_addr(to_addr(irq141_entry));
        idt[174].set_handler_addr(to_addr(irq142_entry));
        idt[175].set_handler_addr(to_addr(irq143_entry));
        idt[176].set_handler_addr(to_addr(irq144_entry));
        idt[177].set_handler_addr(to_addr(irq145_entry));
        idt[178].set_handler_addr(to_addr(irq146_entry));
        idt[179].set_handler_addr(to_addr(irq147_entry));
        idt[180].set_handler_addr(to_addr(irq148_entry));
        idt[181].set_handler_addr(to_addr(irq149_entry));
        idt[182].set_handler_addr(to_addr(irq150_entry));
        idt[183].set_handler_addr(to_addr(irq151_entry));
        idt[184].set_handler_addr(to_addr(irq152_entry));
        idt[185].set_handler_addr(to_addr(irq153_entry));
        idt[186].set_handler_addr(to_addr(irq154_entry));
        idt[187].set_handler_addr(to_addr(irq155_entry));
        idt[188].set_handler_addr(to_addr(irq156_entry));
        idt[189].set_handler_addr(to_addr(irq157_entry));
        idt[190].set_handler_addr(to_addr(irq158_entry));
        idt[191].set_handler_addr(to_addr(irq159_entry));
        idt[192].set_handler_addr(to_addr(irq160_entry));
        idt[193].set_handler_addr(to_addr(irq161_entry));
        idt[194].set_handler_addr(to_addr(irq162_entry));
        idt[195].set_handler_addr(to_addr(irq163_entry));
        idt[196].set_handler_addr(to_addr(irq164_entry));
        idt[197].set_handler_addr(to_addr(irq165_entry));
        idt[198].set_handler_addr(to_addr(irq166_entry));
        idt[199].set_handler_addr(to_addr(irq167_entry));
        idt[200].set_handler_addr(to_addr(irq168_entry));
        idt[201].set_handler_addr(to_addr(irq169_entry));
        idt[202].set_handler_addr(to_addr(irq170_entry));
        idt[203].set_handler_addr(to_addr(irq171_entry));
        idt[204].set_handler_addr(to_addr(irq172_entry));
        idt[205].set_handler_addr(to_addr(irq173_entry));
        idt[206].set_handler_addr(to_addr(irq174_entry));
        idt[207].set_handler_addr(to_addr(irq175_entry));
        idt[208].set_handler_addr(to_addr(irq176_entry));
        idt[209].set_handler_addr(to_addr(irq177_entry));
        idt[210].set_handler_addr(to_addr(irq178_entry));
        idt[211].set_handler_addr(to_addr(irq179_entry));
        idt[212].set_handler_addr(to_addr(irq180_entry));
        idt[213].set_handler_addr(to_addr(irq181_entry));
        idt[214].set_handler_addr(to_addr(irq182_entry));
        idt[215].set_handler_addr(to_addr(irq183_entry));
        idt[216].set_handler_addr(to_addr(irq184_entry));
        idt[217].set_handler_addr(to_addr(irq185_entry));
        idt[218].set_handler_addr(to_addr(irq186_entry));
        idt[219].set_handler_addr(to_addr(irq187_entry));
        idt[220].set_handler_addr(to_addr(irq188_entry));
        idt[221].set_handler_addr(to_addr(irq189_entry));
        idt[222].set_handler_addr(to_addr(irq190_entry));
        idt[223].set_handler_addr(to_addr(irq191_entry));
        idt[224].set_handler_addr(to_addr(irq192_entry));
        idt[225].set_handler_addr(to_addr(irq193_entry));
        idt[226].set_handler_addr(to_addr(irq194_entry));
        idt[227].set_handler_addr(to_addr(irq195_entry));
        idt[228].set_handler_addr(to_addr(irq196_entry));
        idt[229].set_handler_addr(to_addr(irq197_entry));
        idt[230].set_handler_addr(to_addr(irq198_entry));
        idt[231].set_handler_addr(to_addr(irq199_entry));
        idt[232].set_handler_addr(to_addr(irq200_entry));
        idt[233].set_handler_addr(to_addr(irq201_entry));
        idt[234].set_handler_addr(to_addr(irq202_entry));
        idt[235].set_handler_addr(to_addr(irq203_entry));
        idt[236].set_handler_addr(to_addr(irq204_entry));
        idt[237].set_handler_addr(to_addr(irq205_entry));
        idt[238].set_handler_addr(to_addr(irq206_entry));
        idt[239].set_handler_addr(to_addr(irq207_entry));
        idt[240].set_handler_addr(to_addr(irq208_entry));
        idt[241].set_handler_addr(to_addr(irq209_entry));
        idt[242].set_handler_addr(to_addr(irq210_entry));
        idt[243].set_handler_addr(to_addr(irq211_entry));
        idt[244].set_handler_addr(to_addr(irq212_entry));
        idt[245].set_handler_addr(to_addr(irq213_entry));
        idt[246].set_handler_addr(to_addr(irq214_entry));
        idt[247].set_handler_addr(to_addr(irq215_entry));
        idt[248].set_handler_addr(to_addr(irq216_entry));
        idt[249].set_handler_addr(to_addr(irq217_entry));
        idt[250].set_handler_addr(to_addr(irq218_entry));
        idt[251].set_handler_addr(to_addr(irq219_entry));
        idt[252].set_handler_addr(to_addr(irq220_entry));
        idt[253].set_handler_addr(to_addr(irq221_entry));
        idt[254].set_handler_addr(to_addr(irq222_entry));
        idt[255].set_handler_addr(to_addr(irq223_entry));
    }
}

/// Represents an x86-64 interrupt vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Interrupt {
    DivideError,
    Debug,
    NonMaskableInterrupt,
    Breakpoint,
    Overflow,
    BoundRangeExceeded,
    InvalidOpcode,
    DeviceNotAvailable,
    DoubleFault,
    CoprocessorSegmentOverrun,
    InvalidTss,
    SegmentNotPresent,
    StackSegmentFault,
    GeneralProtectionFault,
    PageFault,
    X87FloatingPoint,
    AlignmentCheck,
    MachineCheck,
    SimdFloatingPoint,
    Virtualization,
    CpProtectionException,
    HvInjectionException,
    VmmCommunicationException,
    SecurityException,
    /// User-defined interrupt, where 0 corresponds to vector 32.
    Irq(u8),
}

impl Interrupt {
    /// Returns the raw vector number (0–255).
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::DivideError => 0,
            Self::Debug => 1,
            Self::NonMaskableInterrupt => 2,
            Self::Breakpoint => 3,
            Self::Overflow => 4,
            Self::BoundRangeExceeded => 5,
            Self::InvalidOpcode => 6,
            Self::DeviceNotAvailable => 7,
            Self::DoubleFault => 8,
            Self::CoprocessorSegmentOverrun => 9,
            Self::InvalidTss => 10,
            Self::SegmentNotPresent => 11,
            Self::StackSegmentFault => 12,
            Self::GeneralProtectionFault => 13,
            Self::PageFault => 14,
            Self::X87FloatingPoint => 16,
            Self::AlignmentCheck => 17,
            Self::MachineCheck => 18,
            Self::SimdFloatingPoint => 19,
            Self::Virtualization => 20,
            Self::CpProtectionException => 21,
            Self::HvInjectionException => 28,
            Self::VmmCommunicationException => 29,
            Self::SecurityException => 30,
            Self::Irq(n) => n + 32,
        }
    }

    /// Returns `true` if the CPU pushes an error code for this vector.
    pub const fn has_error_code(self) -> bool {
        matches!(
            self,
            Self::DoubleFault
                | Self::InvalidTss
                | Self::SegmentNotPresent
                | Self::StackSegmentFault
                | Self::GeneralProtectionFault
                | Self::PageFault
                | Self::AlignmentCheck
                | Self::CpProtectionException
                | Self::VmmCommunicationException
                | Self::SecurityException
        )
    }
}

/// Error returned when converting a reserved vector number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservedVector(pub u8);

impl core::fmt::Display for ReservedVector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "reserved interrupt vector: {}", self.0)
    }
}

impl TryFrom<u8> for Interrupt {
    type Error = ReservedVector;

    fn try_from(vector: u8) -> Result<Self, Self::Error> {
        match vector {
            0 => Ok(Self::DivideError),
            1 => Ok(Self::Debug),
            2 => Ok(Self::NonMaskableInterrupt),
            3 => Ok(Self::Breakpoint),
            4 => Ok(Self::Overflow),
            5 => Ok(Self::BoundRangeExceeded),
            6 => Ok(Self::InvalidOpcode),
            7 => Ok(Self::DeviceNotAvailable),
            8 => Ok(Self::DoubleFault),
            9 => Ok(Self::CoprocessorSegmentOverrun),
            10 => Ok(Self::InvalidTss),
            11 => Ok(Self::SegmentNotPresent),
            12 => Ok(Self::StackSegmentFault),
            13 => Ok(Self::GeneralProtectionFault),
            14 => Ok(Self::PageFault),
            15 | 22..=27 | 31 => Err(ReservedVector(vector)),
            16 => Ok(Self::X87FloatingPoint),
            17 => Ok(Self::AlignmentCheck),
            18 => Ok(Self::MachineCheck),
            19 => Ok(Self::SimdFloatingPoint),
            20 => Ok(Self::Virtualization),
            21 => Ok(Self::CpProtectionException),
            28 => Ok(Self::HvInjectionException),
            29 => Ok(Self::VmmCommunicationException),
            30 => Ok(Self::SecurityException),
            32..=255 => Ok(Self::Irq(vector - 32)),
        }
    }
}
