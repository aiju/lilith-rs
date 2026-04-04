use std::process::Command;

fn main() {
    println!("cargo::rerun-if-changed=src/boot.S");
    cc::Build::new().file("src/boot.S").compile("boot");
    println!("cargo::rerun-if-changed=src/vesa.c");
    let files = cc::Build::new()
        .file("src/vesa.c")
        .flag("-m32")
        .flag("-ffreestanding")
        .flag("-nostdlib")
        .compile_intermediates();

    // this is incredibly cursed -- convert ELF32 object file to ELF64
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let status = Command::new("objcopy")
        .arg("-O")
        .arg("elf64-x86-64")
        .arg(&files[0])
        .arg(format!("{out_dir}/vesa64.o"))
        .status()
        .expect("failed to run objcopy");

    cc::Build::new()
        .object(format!("{out_dir}/vesa64.o"))
        .compile("vesa");
    assert!(status.success());
}
