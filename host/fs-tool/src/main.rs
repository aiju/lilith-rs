use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Write;
use std::process;

const MAGIC: &[u8; 8] = b"LILITH!!";
const PAGE_SIZE: u64 = 4096;
const NAME_LEN: usize = 32;
const ENTRY_SIZE: usize = 64; // 32 name + 8 offset + 8 size + 16 reserved
const HEADER_SIZE: usize = 16; // 8 magic + 8 num_files

fn align_up(val: u64, align: u64) -> u64 {
    (val + align - 1) & !(align - 1)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: fs-tool <image> <file> [file ...]");
        process::exit(1);
    }

    let image_path = &args[1];
    let file_paths = &args[2..];
    let num_files = file_paths.len() as u64;

    // read all input files
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut seen = HashSet::new();
    for path in file_paths {
        let data = fs::read(path).unwrap_or_else(|e| {
            eprintln!("fs-tool: {}: {}", path, e);
            process::exit(1);
        });
        let name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        if name.len() > NAME_LEN {
            eprintln!("fs-tool: file name too long (max {} bytes): {}", NAME_LEN, name);
            process::exit(1);
        }
        if !seen.insert(name.to_string()) {
            eprintln!("fs-tool: duplicate file name: {}", name);
            process::exit(1);
        }
        files.push((name.to_string(), data));
    }

    let table_size = ENTRY_SIZE as u64 * num_files;
    let data_start = align_up(HEADER_SIZE as u64 + table_size, PAGE_SIZE);

    // compute offsets
    let mut offset = data_start;
    let mut entries: Vec<(String, u64, u64)> = Vec::new();
    for (name, data) in &files {
        entries.push((name.clone(), offset, data.len() as u64));
        offset = align_up(offset + data.len() as u64, PAGE_SIZE);
    }
    let total_size = offset;

    // build image
    let mut image = vec![0u8; total_size as usize];

    // header
    image[..8].copy_from_slice(MAGIC);
    image[8..16].copy_from_slice(&num_files.to_le_bytes());

    // table
    for (i, (name, off, size)) in entries.iter().enumerate() {
        let base = HEADER_SIZE + i * ENTRY_SIZE;
        let name_bytes = name.as_bytes();
        image[base..base + name_bytes.len()].copy_from_slice(name_bytes);
        image[base + 32..base + 40].copy_from_slice(&off.to_le_bytes());
        image[base + 40..base + 48].copy_from_slice(&size.to_le_bytes());
        // bytes 48..64 are reserved, already zero
    }

    // file data
    for (i, (_name, data)) in files.iter().enumerate() {
        let off = entries[i].1 as usize;
        image[off..off + data.len()].copy_from_slice(data);
    }

    // write image
    let mut out = fs::File::create(image_path).unwrap_or_else(|e| {
        eprintln!("fs-tool: {}: {}", image_path, e);
        process::exit(1);
    });
    out.write_all(&image).unwrap_or_else(|e| {
        eprintln!("fs-tool: write: {}", e);
        process::exit(1);
    });
}
