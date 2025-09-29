use anyhow::Result;
use std::fs;

pub fn scan_and_set_xor_key(data_dir: &str) -> Result<Option<u8>> {
    use walkdir::WalkDir;
    let mut v4_xor_key: Option<u8> = None;
    for e in WalkDir::new(data_dir).into_iter().flatten() {
        if !e.file_type().is_file() { continue; }
        let name = e.file_name().to_string_lossy();
        if !name.ends_with("_t.dat") { continue; }
        let data = match fs::read(e.path()) { Ok(d) => d, Err(_) => continue };
        if data.len() < 17 { continue; }
        let header = &data[..4];
        if header != b"\x07\x08\x56\x31" && header != b"\x07\x08\x56\x32" { continue; }
        let xor_len = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
        let tail = &data[15..];
        if xor_len == 0 || xor_len > tail.len() { continue; }
        let xor_data = &tail[tail.len()-xor_len..];
        if xor_data.len() >= 2 {
            let jpg_tail = [0xFFu8, 0xD9u8];
            let k0 = xor_data[xor_data.len()-2] ^ jpg_tail[0];
            let k1 = xor_data[xor_data.len()-1] ^ jpg_tail[1];
            if k0 == k1 { v4_xor_key = Some(k0); break; }
        }
    }
    Ok(v4_xor_key)
}
