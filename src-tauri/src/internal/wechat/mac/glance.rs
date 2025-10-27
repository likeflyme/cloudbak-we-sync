//! macOS process memory reading utilities (simplified port from chatlog darwin/glance)
//! Works only on macOS. Other platforms return errors.

use anyhow::{Result, anyhow};

#[cfg(target_os = "macos")]
use std::{fs, io::Read, process::Command, time::Duration};

#[cfg(target_os = "macos")]
fn parse_vmmap_regions(pid: u32) -> Result<Vec<(u64,u64)>> {
    // Execute vmmap to list regions
    let out = Command::new("vmmap").arg(format!("{}", pid)).output()?;
    if !out.status.success() { return Err(anyhow!("vmmap failed")); }
    let txt = String::from_utf8_lossy(&out.stdout);
    let mut regions = Vec::new();
    for line in txt.lines() {
        // Typical line starts with start-end hex addresses like: 0000000100000000-0000000101000000 ... rwx/...
        if let Some((addr_part, rest)) = line.split_once(' ') {
            if addr_part.len() >= 33 && addr_part.contains('-') {
                let mut parts = addr_part.split('-');
                if let (Some(a), Some(b)) = (parts.next(), parts.next()) {
                    if let (Ok(start), Ok(end)) = (u64::from_str_radix(a.trim(),16), u64::from_str_radix(b.trim(),16)) {
                        if end>start && rest.contains("rw") { // choose readable+write regions
                            regions.push((start,end));
                        }
                    }
                }
            }
        }
    }
    if regions.is_empty() { return Err(anyhow!("no rw regions parsed")); }
    Ok(regions)
}

#[cfg(target_os = "macos")]
fn read_region_lldb(pid: u32, start: u64, end: u64) -> Result<Vec<u8>> {
    let size = end - start;
    // Use a temporary file instead of FIFO to simplify
    let path = std::env::temp_dir().join(format!("we_sync_memdump_{}_{}.bin", pid, start));
    let lldb_cmd = format!("lldb -p {pid} -o \"memory read --binary --force --outfile {outfile} --count {size} 0x{start:x}\" -o \"quit\"",
        pid=pid, outfile=path.display(), size=size, start=start);
    let status = Command::new("bash").arg("-c").arg(lldb_cmd).status()?;
    if !status.success() { return Err(anyhow!("lldb memory read failed")); }
    let mut f = fs::File::open(&path)?; let mut buf = Vec::new(); f.read_to_end(&mut buf)?; let _=fs::remove_file(path);
    Ok(buf)
}

#[cfg(target_os = "macos")]
pub fn read_process_memory(pid: u32) -> Result<Vec<u8>> {
    let regions = parse_vmmap_regions(pid)?;
    // pick the largest rw region as heuristic (chatlog selects first filtered region)
    let (start,end) = regions.iter().max_by_key(|(s,e)| e - s).copied().unwrap();
    read_region_lldb(pid, start, end)
}

#[cfg(not(target_os = "macos"))]
pub fn read_process_memory(_pid: u32) -> Result<Vec<u8>> { Err(anyhow!("macOS only")) }
