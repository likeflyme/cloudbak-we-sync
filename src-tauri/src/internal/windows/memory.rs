use anyhow::{anyhow, Result};
use crate::internal::windows::winproc::WeChatProcess;
use crate::internal::windows::validator::{DBValidator, ImgKeyValidator};
use rayon::prelude::*;
use crate::commands::wechat::{is_extract_cancelled};

#[cfg(target_os = "windows")]
fn enable_debug_privilege() -> Result<()> {
    use windows::Win32::Security::{AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY, LUID_AND_ATTRIBUTES};
    use windows::Win32::System::Threading::{OpenProcessToken, GetCurrentProcess};
    use windows::Win32::Foundation::{HANDLE, LUID};
    use windows::core::PCWSTR;
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token).is_err() {
            return Ok(()); // not fatal
        }
        let mut luid = LUID::default();
        let name: Vec<u16> = "SeDebugPrivilege".encode_utf16().chain([0]).collect();
        if LookupPrivilegeValueW(None, PCWSTR(name.as_ptr()), &mut luid).is_err() {
            return Ok(());
        }
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
        };
        let _ = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None);
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub fn extract_keys_windows(proc: &WeChatProcess) -> Result<(Option<String>, Option<String>)> {
    use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows::Win32::System::Memory::{MEM_COMMIT, MEM_PRIVATE, MEM_MAPPED, MEM_IMAGE, PAGE_GUARD, PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, VirtualQueryEx, MEMORY_BASIC_INFORMATION};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
    use windows::Win32::Foundation::BOOL;

    // Diagnostics for validators
    if let Some(dir) = proc.data_dir.as_deref() {
        use std::path::Path;
        let db_path = Path::new(dir).join("db_storage").join("message").join("message_0.db");
        println!("[dbg] DB path exists: {}", db_path.exists());
        let mut dat_count = 0usize;
        // Limit depth & early break if cancellation requested
        for ent in walkdir::WalkDir::new(dir).max_depth(6).into_iter().flatten() {
            if is_extract_cancelled() { println!("[dbg] cancelled during initial dat count"); return Ok((None, None)); }
            if ent.file_type().is_file() {
                let n = ent.file_name().to_string_lossy();
                if n.ends_with(".dat") { dat_count += 1; }
            }
            if dat_count % 3000 == 0 && dat_count > 0 { println!("[dbg] .dat counting progress: {}", dat_count); }
            if dat_count > 20000 { println!("[dbg] dat count cap reached: {}", dat_count); break; }
        }
        println!("[dbg] .dat files under data_dir: {}", dat_count);
    } else {
        println!("[dbg] No data_dir set; validators will be disabled");
    }

    // Prepare validators
    let db_validator: Option<DBValidator> = if let Some(dir) = proc.data_dir.as_deref() {
        if is_extract_cancelled() { return Ok((None, None)); }
        match DBValidator::new(dir) { Ok(v) => { println!("[dbg] DBValidator initialized"); Some(v) }, Err(e) => { println!("[dbg] DBValidator init failed: {}", e); None } }
    } else { None };
    let img_validator: Option<ImgKeyValidator> = if let Some(dir) = proc.data_dir.as_deref() {
        if is_extract_cancelled() { return Ok((None, None)); }
        match ImgKeyValidator::new(dir) { Ok(v) => { println!("[dbg] ImgKeyValidator initialized"); Some(v) }, Err(e) => { println!("[dbg] ImgKeyValidator init failed: {}", e); None } }
    } else { None };

    // Try enable SeDebugPrivilege
    let _ = enable_debug_privilege();

    // Helper: is region readable enough to scan keys
    #[inline]
    fn is_readable(protect: u32) -> bool {
        let base = protect & 0xFF; // low byte contains the primary protection
        if (protect & PAGE_GUARD.0) != 0 || base == PAGE_NOACCESS.0 { return false; }
        // Favor read/write and execute-read; include read-only too
        base == PAGE_READONLY.0
            || base == PAGE_READWRITE.0
            || base == PAGE_WRITECOPY.0
            || base == PAGE_EXECUTE_READ.0
            || base == PAGE_EXECUTE_READWRITE.0
            || base == PAGE_EXECUTE_WRITECOPY.0
    }

    // Helper: acceptable region type - prefer MEM_PRIVATE like Python, but keep others
    #[inline]
    fn is_acceptable_type(t: u32) -> bool {
        t == MEM_PRIVATE.0 || t == MEM_MAPPED.0 || t == MEM_IMAGE.0
    }

    // Fallback patterns (macOS-style) for extra robustness
    const DATA_PAT: &[u8] = b"\x20fts5(%\x00"; // 0x20 'f' 't' 's' '5' '(' '%' 0x00
    const DATA_OFFSETS: [isize; 3] = [16, -80, 64];
    static ZERO16: [u8; 16] = [0u8; 16];
    // Broaden offsets around 16 zero bytes signature; many builds place the AES key nearby
    const IMG_FALLBACK_OFFSETS: [isize; 9] = [-128, -96, -80, -64, -48, -32, -24, -20, -16];

    unsafe {
        let h = OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, BOOL(0), proc.pid as u32)?;
        if h.is_invalid() { return Err(anyhow!("OpenProcess failed")); }
        let mut address: usize = 0x10000;
        let max_addr: usize = 0x7FFF_FFFF_FFFF;

        // Phase 1: scan and collect candidates (do not run PBKDF2 here)
        let mut cand_db: Vec<[u8; 32]> = Vec::new();
        let mut cand_img: Vec<[u8; 16]> = Vec::new();
        use std::collections::HashSet;
        let mut seen_db: std::collections::HashSet<[u8; 32]> = HashSet::new();
        let mut seen_img: std::collections::HashSet<[u8; 16]> = HashSet::new();

        let mut regions_processed = 0usize;
        const MAX_CHUNK: usize = 32 * 1024 * 1024; // reduce chunk size for speed
        const MAX_REGION_CHUNKS: usize = 2; // fewer chunks per region first pass
        const MAX_REGIONS: usize = 400; // cap regions for initial scan
        const MAX_CAND_DB: usize = 2000; // lower candidate caps
        const MAX_CAND_IMG: usize = 20000;  // allow far more image candidates

        // Immediate image key result (pointer-verified), like Python
        let mut img_hex_inline: Option<String> = None;

        while address < max_addr {
            if is_extract_cancelled() { println!("[dbg] cancelled before region query"); return Ok((None, None)); }
            let mut mbi = MEMORY_BASIC_INFORMATION::default();
            let res = VirtualQueryEx(h, Some(address as _), &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>());
            if res == 0 { break; }
            let base = mbi.BaseAddress as usize;
            let size = mbi.RegionSize;
            let state = mbi.State;
            let protect = mbi.Protect;
            let memtype = mbi.Type;
            if size >= 64*1024 && size <= 512*1024*1024 && state == MEM_COMMIT && is_readable(protect.0) && is_acceptable_type(memtype.0) {
                let total_to_scan = std::cmp::min(size, MAX_CHUNK * MAX_REGION_CHUNKS);
                println!("[dbg] region base=0x{:x} size={} prot=0x{:x} type=0x{:x} scanning={}", base, size, protect.0, memtype.0, total_to_scan);
                let mut offset = 0usize;
                while offset < total_to_scan {
                    if is_extract_cancelled() { println!("[dbg] cancelled inside region scan"); return Ok((None, None)); }
                    let to_read = std::cmp::min(MAX_CHUNK, total_to_scan - offset);
                    let mut buf = vec![0u8; to_read];
                    let mut read = 0usize;
                    let ok = ReadProcessMemory(h, (base + offset) as _, buf.as_mut_ptr() as _, to_read, Some(&mut read));
                    if ok.is_ok() && read > 0 {
                        buf.truncate(read);
                        // Windows pointer-chasing pattern
                        let key_pattern: [u8; 24] = [
                            0,0,0,0,0,0,0,0,
                            0x20,0,0,0,0,0,0,0,
                            0x2F,0,0,0,0,0,0,0,
                        ];
                        let mut idx = buf.len();
                        let mut hits = 0usize;
                        while idx > 0 {
                            if is_extract_cancelled() { println!("[dbg] cancelled during pattern search"); return Ok((None, None)); }
                            if let Some(pos) = memchr::memmem::rfind(&buf[..idx], &key_pattern) {
                                hits += 1;
                                if pos >= 8 {
                                    let mut ptr_bytes = [0u8; 8];
                                    ptr_bytes.copy_from_slice(&buf[pos-8..pos]);
                                    let ptr = usize::from_le_bytes(ptr_bytes);
                                    if ptr > 0x10000 && ptr < 0x7FFF_FFFF_FFFF {
                                        // read 32 bytes at ptr
                                        let mut keybuf = [0u8; 32];
                                        let mut got = 0usize;
                                        let ok2 = ReadProcessMemory(h, ptr as _, keybuf.as_mut_ptr() as _, 32, Some(&mut got));
                                        if ok2.is_ok() && got >= 16 {
                                            // inline validate image key (first 16, then last 16) if possible
                                            if img_hex_inline.is_none() {
                                                if let Some(iv) = &img_validator {
                                                    let first16 = &keybuf[..16];
                                                    let last16 = &keybuf[16..32];
                                                    if iv.validate_img_key(first16) {
                                                        let hx = hex::encode(first16);
                                                        println!("[dbg] Image key (first16) via pointer at 0x{:x}", ptr);
                                                        img_hex_inline = Some(hx);
                                                    } else if got >= 32 && iv.validate_img_key(last16) {
                                                        let hx = hex::encode(last16);
                                                        println!("[dbg] Image key (last16) via pointer at 0x{:x}", ptr);
                                                        img_hex_inline = Some(hx);
                                                    }
                                                }
                                            }
                                            // collect image candidate (first 16 and last 16)
                                            if cand_img.len() < MAX_CAND_IMG {
                                                let mut k16 = [0u8; 16];
                                                k16.copy_from_slice(&keybuf[..16]);
                                                if seen_img.insert(k16) { cand_img.push(k16); }
                                            }
                                            if got >= 32 && cand_img.len() < MAX_CAND_IMG {
                                                let mut k16b = [0u8; 16];
                                                k16b.copy_from_slice(&keybuf[16..32]);
                                                if seen_img.insert(k16b) { cand_img.push(k16b); }
                                            }
                                            // collect db candidate (32)
                                            if keybuf.iter().filter(|&&x| x == 0).count() < 10 && cand_db.len() < MAX_CAND_DB {
                                                if seen_db.insert(keybuf) {
                                                    cand_db.push(keybuf);
                                                }
                                            }
                                        }
                                    }
                                }
                                idx = pos;
                            } else { break; }
                        }
                        println!("[dbg] pattern hits in chunk: {} (db_cands={}, img_cands={})", hits, cand_db.len(), cand_img.len());

                        // Fallback 1: macOS-style data patterns within the chunk (collect only)
                        if cand_db.len() < MAX_CAND_DB {
                            let mut i = buf.len();
                            while i > 0 {
                                if is_extract_cancelled() { println!("[dbg] cancelled during DATA_PAT fallback"); return Ok((None, None)); }
                                if let Some(pos) = memchr::memmem::rfind(&buf[..i], DATA_PAT) {
                                    for off in DATA_OFFSETS {
                                        let key_off = pos as isize + off;
                                        if key_off >= 0 {
                                            let ko = key_off as usize;
                                            if ko + 32 <= buf.len() {
                                                let mut k32 = [0u8; 32];
                                                k32.copy_from_slice(&buf[ko..ko+32]);
                                                if k32.iter().filter(|&&x| x == 0).count() < 10 && seen_db.insert(k32) {
                                                    cand_db.push(k32);
                                                }
                                            }
                                        }
                                        if cand_db.len() >= MAX_CAND_DB { break; }
                                    }
                                    if cand_db.len() >= MAX_CAND_DB { break; }
                                    i = pos;
                                } else { break; }
                            }
                        }

                        // Fallback 2: image key zero16 pattern within the chunk (try multiple offsets)
                        if cand_img.len() < MAX_CAND_IMG {
                            let mut i = buf.len();
                            while i > 0 {
                                if is_extract_cancelled() { println!("[dbg] cancelled during ZERO16 fallback"); return Ok((None, None)); }
                                if let Some(pos) = memchr::memmem::rfind(&buf[..i], &ZERO16) {
                                    for off in IMG_FALLBACK_OFFSETS {
                                        if pos as isize + off >= 0 {
                                            let ko = (pos as isize + off) as usize;
                                            if ko + 16 <= buf.len() {
                                                let mut k16 = [0u8; 16];
                                                k16.copy_from_slice(&buf[ko..ko+16]);
                                                if seen_img.insert(k16) { cand_img.push(k16); }
                                            }
                                        }
                                        if cand_img.len() >= MAX_CAND_IMG { break; }
                                    }
                                    if cand_img.len() >= MAX_CAND_IMG { break; }
                                    i = pos;
                                } else { break; }
                            }
                        }
                    }
                    if cand_db.len() >= MAX_CAND_DB && cand_img.len() >= MAX_CAND_IMG { break; }
                    offset += to_read;
                }
                regions_processed += 1;
                if regions_processed % 50 == 0 { println!("[dbg] region progress: {} regions, db_cands={}, img_cands={}", regions_processed, cand_db.len(), cand_img.len()); }
                if regions_processed > MAX_REGIONS { println!("[dbg] stop after {} regions", MAX_REGIONS); break; }
                if cand_db.len() >= MAX_CAND_DB && cand_img.len() >= MAX_CAND_IMG { break; }
            }
            let next = (mbi.BaseAddress as usize).saturating_add(mbi.RegionSize);
            address = if next <= address { address + mbi.RegionSize } else { next };
            if cand_db.len() >= MAX_CAND_DB && cand_img.len() >= MAX_CAND_IMG { break; }
        }

        println!("[dbg] candidates collected: db_keys={}, img_keys={}", cand_db.len(), cand_img.len());

        if is_extract_cancelled() { println!("[dbg] cancelled before validation phase"); return Ok((None, None)); }
        // Phase 2: validate candidates
        let data_hex = if let Some(v) = &db_validator {
            cand_db
                .par_iter()
                .find_any(|k| {
                    if is_extract_cancelled() { return false; }
                    v.validate_db_key(&k[..])
                })
                .map(|k| hex::encode(k))
        } else { None };
        if is_extract_cancelled() { println!("[dbg] cancelled mid validation"); return Ok((None, None)); }
        let mut img_hex = if let Some(iv) = &img_validator {
            if img_hex_inline.is_some() {
                img_hex_inline
            } else {
                let found = cand_img.iter().find_map(|k16| {
                    if is_extract_cancelled() { return None; }
                    if iv.validate_img_key(k16) { Some(hex::encode(k16)) } else { None }
                });
                if found.is_none() { println!("[dbg] ImgKey validation failed over {} candidates", cand_img.len()); }
                found
            }
        } else { None };

        // ---------------- Second-stage deep scan (B) if image key still missing ----------------
        if img_hex.is_none() && img_validator.is_some() && !is_extract_cancelled() {
            println!("[dbg] starting second-stage deep scan for image key (expanded limits)");
            const DEEP_MAX_CHUNK: usize = 96 * 1024 * 1024;
            const DEEP_MAX_REGION_CHUNKS: usize = 4;
            const DEEP_MAX_REGIONS: usize = 1200;
            const DEEP_MAX_CAND_IMG: usize = 60000;
            let iv = img_validator.as_ref().unwrap();
            let mut deep_address: usize = 0x10000;
            let mut deep_regions = 0usize;
            let mut deep_seen: std::collections::HashSet<[u8;16]> = std::collections::HashSet::new();
            let mut deep_found_inline: Option<String> = None;
            unsafe {
                while deep_address < max_addr {
                    if is_extract_cancelled() { println!("[dbg] cancelled during deep scan"); break; }
                    let mut mbi = MEMORY_BASIC_INFORMATION::default();
                    let res = VirtualQueryEx(h, Some(deep_address as _), &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>());
                    if res == 0 { break; }
                    let base = mbi.BaseAddress as usize;
                    let size = mbi.RegionSize;
                    let state = mbi.State;
                    let protect = mbi.Protect;
                    let memtype = mbi.Type;
                    if size >= 64*1024 && size <= 512*1024*1024 && state == MEM_COMMIT && is_readable(protect.0) && is_acceptable_type(memtype.0) {
                        let total_to_scan = std::cmp::min(size, DEEP_MAX_CHUNK * DEEP_MAX_REGION_CHUNKS);
                        let mut offset = 0usize;
                        while offset < total_to_scan {
                            if is_extract_cancelled() { println!("[dbg] cancelled inside deep region"); break; }
                            let to_read = std::cmp::min(DEEP_MAX_CHUNK, total_to_scan - offset);
                            let mut buf = vec![0u8; to_read];
                            let mut read = 0usize;
                            let ok = ReadProcessMemory(h, (base + offset) as _, buf.as_mut_ptr() as _, to_read, Some(&mut read));
                            if ok.is_ok() && read > 0 {
                                buf.truncate(read);
                                // pointer-based pattern reuse (image key extraction focus)
                                let key_pattern: [u8; 24] = [
                                    0,0,0,0,0,0,0,0,
                                    0x20,0,0,0,0,0,0,0,
                                    0x2F,0,0,0,0,0,0,0,
                                ];
                                let mut idx = buf.len();
                                while idx > 0 {
                                    if is_extract_cancelled() { println!("[dbg] cancelled during deep pattern search"); break; }
                                    if let Some(pos) = memchr::memmem::rfind(&buf[..idx], &key_pattern) {
                                        if pos >= 8 {
                                            let mut ptr_bytes = [0u8; 8];
                                            ptr_bytes.copy_from_slice(&buf[pos-8..pos]);
                                            let ptr = usize::from_le_bytes(ptr_bytes);
                                            if ptr > 0x10000 && ptr < 0x7FFF_FFFF_FFFF {
                                                let mut keybuf = [0u8;32];
                                                let mut got=0usize;
                                                let ok2 = ReadProcessMemory(h, ptr as _, keybuf.as_mut_ptr() as _, 32, Some(&mut got));
                                                if ok2.is_ok() && got >= 16 {
                                                    if deep_found_inline.is_none() && iv.validate_img_key(&keybuf[..16]) { deep_found_inline = Some(hex::encode(&keybuf[..16])); }
                                                    if deep_found_inline.is_none() && got >= 32 && iv.validate_img_key(&keybuf[16..32]) { deep_found_inline = Some(hex::encode(&keybuf[16..32])); }
                                                    if deep_found_inline.is_some() { break; }
                                                    // collect for fallback zero16 offsets expansion
                                                    if deep_seen.len() < DEEP_MAX_CAND_IMG {
                                                        let mut k16a = [0u8;16]; k16a.copy_from_slice(&keybuf[..16]); if deep_seen.insert(k16a) && iv.validate_img_key(&k16a) { deep_found_inline = Some(hex::encode(k16a)); break; }
                                                        if got >= 32 { let mut k16b=[0u8;16]; k16b.copy_from_slice(&keybuf[16..32]); if deep_seen.insert(k16b) && iv.validate_img_key(&k16b) { deep_found_inline = Some(hex::encode(k16b)); break; } }
                                                    }
                                                }
                                            }
                                        }
                                        idx = pos;
                                    } else { break; }
                                }
                                if deep_found_inline.is_none() {
                                    // broader ZERO16 fallback (extend offsets)
                                    const EXTRA_OFFSETS: [isize; 6] = [-200,-160,-144,-136,-128,-112];
                                    let mut i = buf.len();
                                    while i > 0 && deep_found_inline.is_none() {
                                        if let Some(pos) = memchr::memmem::rfind(&buf[..i], &ZERO16) {
                                            for off in IMG_FALLBACK_OFFSETS.iter().chain(EXTRA_OFFSETS.iter()) {
                                                if pos as isize + off >= 0 {
                                                    let ko = (pos as isize + off) as usize;
                                                    if ko + 16 <= buf.len() {
                                                        let mut k16=[0u8;16]; k16.copy_from_slice(&buf[ko..ko+16]);
                                                        if deep_seen.insert(k16) && iv.validate_img_key(&k16) { deep_found_inline = Some(hex::encode(k16)); break; }
                                                    }
                                                }
                                                if deep_found_inline.is_some() || deep_seen.len() >= DEEP_MAX_CAND_IMG { break; }
                                            }
                                            i = pos;
                                        } else { break; }
                                    }
                                }
                            }
                            if deep_found_inline.is_some() { break; }
                            offset += to_read;
                        }
                        deep_regions += 1;
                        if deep_regions % 60 == 0 { println!("[dbg] deep scan progress: {} regions", deep_regions); }
                        if deep_found_inline.is_some() { println!("[dbg] deep scan image key found; regions={}", deep_regions); break; }
                        if deep_regions > DEEP_MAX_REGIONS { println!("[dbg] deep scan region cap reached: {}", DEEP_MAX_REGIONS); break; }
                    }
                    let next = (mbi.BaseAddress as usize).saturating_add(mbi.RegionSize);
                    deep_address = if next <= deep_address { deep_address + mbi.RegionSize } else { next };
                    if deep_found_inline.is_some() || is_extract_cancelled() { break; }
                }
            }
            if deep_found_inline.is_some() { img_hex = deep_found_inline; } else { println!("[dbg] second-stage deep scan finished without image key"); }
        }
        // ---------------------------------------------------------------------------------------

        Ok((data_hex, img_hex))
    }
}
