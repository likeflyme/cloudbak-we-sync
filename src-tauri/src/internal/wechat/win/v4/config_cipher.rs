use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};

use crate::commands::wechat::is_extract_cancelled;
use crate::internal::windows::validator::V4DbTarget;

const CONFIG_CIPHER_NAME: &[u8] = b"com.Tencent.WCDB.Config.Cipher";
const CONFIG_XOR_MASK: [u8; 32] = [
    0xd2, 0xc7, 0x44, 0x24, 0x58, 0x02, 0x00, 0x00, 0x00, 0x48, 0x89, 0x44, 0x24, 0x50, 0x48, 0x8b,
    0x45, 0x00, 0x48, 0x84, 0x4c, 0x24, 0x48, 0x48, 0x89, 0x44, 0x25, 0x40, 0x48, 0x58, 0x4c, 0x24,
];
const MAX_USER_ADDRESS: usize = 0x0000_8000_0000_0000;
const MAX_REGION_SIZE: usize = 500 * 1024 * 1024;
const CONFIG_BLOB_MAX: usize = 1024;
const SCAN_CHUNK_SIZE: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy)]
struct ConfigCipherLayout {
    node_config_ptr_offset: usize,
    config_blob_object_offset: usize,
    object_data_ptr_offset: usize,
    object_data_len_offset: usize,
}

// Observed in Windows WeChat 4.1.11. Additional layouts can be added here
// without changing the scanner and verification flow.
const CONFIG_CIPHER_LAYOUTS: &[ConfigCipherLayout] = &[ConfigCipherLayout {
    node_config_ptr_offset: 0x28,
    config_blob_object_offset: 0x88,
    object_data_ptr_offset: 0x08,
    object_data_len_offset: 0x10,
}];

#[derive(Clone, Copy, Debug)]
struct MemoryRegion {
    base: usize,
    size: usize,
}

trait MemoryReader {
    fn regions(&self) -> &[MemoryRegion];
    fn read(&self, address: usize, size: usize) -> Option<Vec<u8>>;
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct KeyCandidate {
    key: [u8; 32],
    embedded_salt: Option<[u8; 16]>,
}

#[derive(Default)]
struct ConfigScanStats {
    needle_occurrences: usize,
    string_object_refs: usize,
    node_candidates: usize,
    config_ptr_candidates: usize,
    blob_count: usize,
    candidate_count: usize,
    verified_candidates: usize,
    matched_salts: usize,
}

#[derive(Default)]
struct LegacyScanStats {
    candidate_count: usize,
    matched_salts: usize,
}

#[cfg(target_os = "windows")]
struct ProcessMemory {
    handle: windows::Win32::Foundation::HANDLE,
    regions: Vec<MemoryRegion>,
}

#[cfg(target_os = "windows")]
impl ProcessMemory {
    fn open(pid: u32) -> Result<Self> {
        use windows::Win32::Foundation::BOOL;
        use windows::Win32::System::Memory::{
            VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_EXECUTE_READ,
            PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS,
            PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY,
        };
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
        };

        fn is_readable(protect: u32) -> bool {
            let base = protect & 0xff;
            if protect & PAGE_GUARD.0 != 0 || base == PAGE_NOACCESS.0 {
                return false;
            }
            base == PAGE_READONLY.0
                || base == PAGE_READWRITE.0
                || base == PAGE_WRITECOPY.0
                || base == PAGE_EXECUTE_READ.0
                || base == PAGE_EXECUTE_READWRITE.0
                || base == PAGE_EXECUTE_WRITECOPY.0
        }

        unsafe {
            let handle = OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, BOOL(0), pid)
                .map_err(|e| anyhow!("OpenProcess failed: {e}"))?;

            let mut regions = Vec::new();
            let mut address = 0usize;
            while address < MAX_USER_ADDRESS {
                let mut mbi = MEMORY_BASIC_INFORMATION::default();
                let queried = VirtualQueryEx(
                    handle,
                    Some(address as _),
                    &mut mbi,
                    std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                );
                if queried == 0 {
                    break;
                }

                let base = mbi.BaseAddress as usize;
                let size = mbi.RegionSize;
                if mbi.State == MEM_COMMIT
                    && size > 0
                    && size < MAX_REGION_SIZE
                    && is_readable(mbi.Protect.0)
                {
                    regions.push(MemoryRegion { base, size });
                }

                let next = base.saturating_add(size);
                address = if next <= address {
                    address.saturating_add(0x1000)
                } else {
                    next
                };
            }

            Ok(Self { handle, regions })
        }
    }
}

#[cfg(target_os = "windows")]
impl MemoryReader for ProcessMemory {
    fn regions(&self) -> &[MemoryRegion] {
        &self.regions
    }

    fn read(&self, address: usize, size: usize) -> Option<Vec<u8>> {
        use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;

        if size == 0 {
            return Some(Vec::new());
        }
        unsafe {
            let mut buffer = vec![0u8; size];
            let mut bytes_read = 0usize;
            if ReadProcessMemory(
                self.handle,
                address as _,
                buffer.as_mut_ptr() as _,
                size,
                Some(&mut bytes_read),
            )
            .is_err()
                || bytes_read == 0
            {
                return None;
            }
            buffer.truncate(bytes_read);
            Some(buffer)
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for ProcessMemory {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    let bytes: [u8; 8] = data.get(offset..offset + 8)?.try_into().ok()?;
    Some(u64::from_le_bytes(bytes))
}

fn is_user_pointer(value: u64) -> bool {
    value >= 0x10000 && value < MAX_USER_ADDRESS as u64
}

fn walk_region_chunks<R, F>(reader: &R, chunk_size: usize, overlap: usize, mut visit: F)
where
    R: MemoryReader,
    F: FnMut(usize, &[u8]) -> bool,
{
    for region in reader.regions() {
        let mut offset = 0usize;
        let mut tail = Vec::new();
        let mut tail_base = region.base;

        while offset < region.size {
            if is_extract_cancelled() {
                return;
            }
            let current_size = chunk_size.min(region.size - offset);
            let Some(chunk) = reader.read(region.base + offset, current_size) else {
                tail.clear();
                offset += current_size;
                continue;
            };
            let data_base = if tail.is_empty() {
                region.base + offset
            } else {
                tail_base
            };
            let mut data = Vec::with_capacity(tail.len() + chunk.len());
            data.extend_from_slice(&tail);
            data.extend_from_slice(&chunk);
            if !visit(data_base, &data) {
                return;
            }

            let keep = overlap.min(data.len());
            if keep > 0 {
                tail = data[data.len() - keep..].to_vec();
                tail_base = data_base + data.len() - keep;
            } else {
                tail.clear();
                tail_base = region.base + offset + current_size;
            }
            offset += current_size;
        }
    }
}

fn find_bytes_in_regions<R: MemoryReader>(
    reader: &R,
    needle: &[u8],
    chunk_size: usize,
) -> HashSet<usize> {
    let mut addresses = HashSet::new();
    walk_region_chunks(
        reader,
        chunk_size,
        needle.len().saturating_sub(1),
        |base, data| {
            let mut search_pos = 0usize;
            while search_pos + needle.len() <= data.len() {
                let Some(relative) = memchr::memmem::find(&data[search_pos..], needle) else {
                    break;
                };
                let position = search_pos + relative;
                addresses.insert(base + position);
                search_pos = position + 1;
            }
            true
        },
    );
    addresses
}

fn probable_key(key: &[u8; 32]) -> bool {
    if key.iter().all(|byte| *byte == 0) || key.iter().all(|byte| *byte == 0xff) {
        return false;
    }
    key.iter().copied().collect::<HashSet<_>>().len() >= 15
}

fn parse_key_literals(data: &[u8]) -> Vec<KeyCandidate> {
    let mut candidates = HashSet::new();
    let mut position = 0usize;

    while position + 3 <= data.len() {
        if !matches!(data[position], b'x' | b'X') || data[position + 1] != b'\'' {
            position += 1;
            continue;
        }

        let hex_start = position + 2;
        let mut hex_end = hex_start;
        while hex_end < data.len() && data[hex_end].is_ascii_hexdigit() {
            hex_end += 1;
        }
        let run_len = hex_end - hex_start;
        if hex_end >= data.len() || data[hex_end] != b'\'' || !(64..=192).contains(&run_len) {
            position += 1;
            continue;
        }

        let mut starts = vec![0usize];
        if run_len > 96 {
            let mut start = 0usize;
            while start + 64 <= run_len {
                starts.push(start);
                start += 32;
            }
            starts.push(run_len - 64);
        }
        starts.sort_unstable();
        starts.dedup();

        for start in starts {
            if start + 64 > run_len {
                continue;
            }
            let Ok(key_bytes) = hex::decode(&data[hex_start + start..hex_start + start + 64])
            else {
                continue;
            };
            let Ok(key) = <[u8; 32]>::try_from(key_bytes.as_slice()) else {
                continue;
            };
            if !probable_key(&key) {
                continue;
            }

            let embedded_salt = if start + 96 <= run_len {
                hex::decode(&data[hex_start + start + 64..hex_start + start + 96])
                    .ok()
                    .and_then(|bytes| <[u8; 16]>::try_from(bytes.as_slice()).ok())
            } else {
                None
            };
            candidates.insert(KeyCandidate { key, embedded_salt });
        }

        position = hex_end + 1;
    }

    candidates.into_iter().collect()
}

fn decode_config_blob(blob: &[u8]) -> Vec<KeyCandidate> {
    if blob.is_empty() || blob.len() > CONFIG_BLOB_MAX {
        return Vec::new();
    }
    let decoded: Vec<u8> = blob
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ CONFIG_XOR_MASK[index % CONFIG_XOR_MASK.len()])
        .collect();
    parse_key_literals(&decoded)
}

fn verify_candidate(
    candidate: KeyCandidate,
    targets: &[V4DbTarget],
    found: &mut HashMap<[u8; 16], [u8; 32]>,
) -> usize {
    let mut matched = 0usize;
    let mut attempted = HashSet::new();

    if let Some(salt) = candidate.embedded_salt {
        for target in targets.iter().filter(|target| target.salt() == salt) {
            attempted.insert(salt);
            if !found.contains_key(&salt) && target.validate_derived_key(&candidate.key) {
                found.insert(salt, candidate.key);
                matched += 1;
                break;
            }
        }
    }

    for target in targets {
        let salt = target.salt();
        if found.contains_key(&salt) || attempted.contains(&salt) {
            continue;
        }
        attempted.insert(salt);
        if target.validate_derived_key(&candidate.key) {
            found.insert(salt, candidate.key);
            matched += 1;
        }
    }
    matched
}

fn target_salt_count(targets: &[V4DbTarget]) -> usize {
    targets
        .iter()
        .map(V4DbTarget::salt)
        .collect::<HashSet<_>>()
        .len()
}

fn scan_config_cipher_reader<R: MemoryReader>(
    reader: &R,
    targets: &[V4DbTarget],
    found: &mut HashMap<[u8; 16], [u8; 32]>,
    chunk_size: usize,
) -> ConfigScanStats {
    let mut stats = ConfigScanStats::default();
    let needle_addresses = find_bytes_in_regions(reader, CONFIG_CIPHER_NAME, chunk_size);
    stats.needle_occurrences = needle_addresses.len();
    if needle_addresses.is_empty() {
        return stats;
    }

    let patterns: Vec<(usize, [u8; 16])> = needle_addresses
        .iter()
        .map(|address| {
            let mut pattern = [0u8; 16];
            pattern[..8].copy_from_slice(&(*address as u64).to_le_bytes());
            pattern[8..].copy_from_slice(&(CONFIG_CIPHER_NAME.len() as u64).to_le_bytes());
            (*address, pattern)
        })
        .collect();
    let mut seen_config_ptrs = HashSet::new();
    let mut seen_candidates = HashSet::new();
    let wanted_salts = target_salt_count(targets);

    walk_region_chunks(reader, chunk_size, 0x80, |base, data| {
        if found.len() >= wanted_salts || is_extract_cancelled() {
            return false;
        }
        for (needle_address, pattern) in &patterns {
            let mut search_pos = 0usize;
            while search_pos + pattern.len() <= data.len() {
                let Some(relative) = memchr::memmem::find(&data[search_pos..], pattern) else {
                    break;
                };
                let position = search_pos + relative;
                stats.string_object_refs += 1;
                let reference_address = base + position;
                let Some(node_base) = reference_address.checked_sub(0x10) else {
                    search_pos = position + 1;
                    continue;
                };
                let Some(node) = reader.read(node_base, 0x50) else {
                    search_pos = position + 1;
                    continue;
                };
                if node.len() < 0x40
                    || read_u64(&node, 0x10) != Some(*needle_address as u64)
                    || read_u64(&node, 0x18) != Some(CONFIG_CIPHER_NAME.len() as u64)
                {
                    search_pos = position + 1;
                    continue;
                }

                stats.node_candidates += 1;
                for (layout_index, layout) in CONFIG_CIPHER_LAYOUTS.iter().enumerate() {
                    let Some(config_ptr) = read_u64(&node, layout.node_config_ptr_offset) else {
                        continue;
                    };
                    if !is_user_pointer(config_ptr)
                        || !seen_config_ptrs.insert((layout_index, config_ptr))
                    {
                        continue;
                    }
                    stats.config_ptr_candidates += 1;

                    let Some(object_address) =
                        (config_ptr as usize).checked_add(layout.config_blob_object_offset)
                    else {
                        continue;
                    };
                    let object_size = layout.object_data_len_offset + 8;
                    let Some(object) = reader.read(object_address, object_size) else {
                        continue;
                    };
                    let Some(data_ptr) = read_u64(&object, layout.object_data_ptr_offset) else {
                        continue;
                    };
                    let Some(data_len) = read_u64(&object, layout.object_data_len_offset) else {
                        continue;
                    };
                    if !is_user_pointer(data_ptr)
                        || data_len == 0
                        || data_len > CONFIG_BLOB_MAX as u64
                    {
                        continue;
                    }
                    let Some(blob) = reader.read(data_ptr as usize, data_len as usize) else {
                        continue;
                    };
                    if blob.len() != data_len as usize {
                        continue;
                    }
                    stats.blob_count += 1;

                    for candidate in decode_config_blob(&blob) {
                        if !seen_candidates.insert(candidate) {
                            continue;
                        }
                        stats.candidate_count += 1;
                        let matched = verify_candidate(candidate, targets, found);
                        if matched > 0 {
                            stats.verified_candidates += 1;
                            stats.matched_salts += matched;
                        }
                    }
                }

                search_pos = position + 1;
            }
        }
        found.len() < wanted_salts
    });

    stats
}

fn scan_legacy_reader<R: MemoryReader>(
    reader: &R,
    targets: &[V4DbTarget],
    found: &mut HashMap<[u8; 16], [u8; 32]>,
    chunk_size: usize,
) -> LegacyScanStats {
    let mut stats = LegacyScanStats::default();
    let mut seen_candidates = HashSet::new();
    let wanted_salts = target_salt_count(targets);

    walk_region_chunks(reader, chunk_size, 193, |_base, data| {
        if found.len() >= wanted_salts || is_extract_cancelled() {
            return false;
        }
        for candidate in parse_key_literals(data) {
            if !seen_candidates.insert(candidate) {
                continue;
            }
            stats.candidate_count += 1;
            stats.matched_salts += verify_candidate(candidate, targets, found);
        }
        found.len() < wanted_salts
    });
    stats
}

fn format_found_keys(found: HashMap<[u8; 16], [u8; 32]>) -> Vec<String> {
    let mut entries: Vec<([u8; 16], [u8; 32])> = found.into_iter().collect();
    entries.sort_by_key(|(salt, _)| *salt);
    entries
        .into_iter()
        .map(|(salt, key)| format!("{}{}", hex::encode(key), hex::encode(salt)))
        .collect()
}

#[cfg(target_os = "windows")]
pub(crate) fn scan_verified_db_keys(pids: &[u32], targets: &[V4DbTarget]) -> Result<Vec<String>> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    if pids.is_empty() {
        return Err(anyhow!("未找到可供数据库密钥扫描的微信进程"));
    }

    let wanted_salts = target_salt_count(targets);
    let mut found = HashMap::new();
    let mut opened_pids = HashSet::new();
    let mut config_totals = ConfigScanStats::default();

    for pid in pids {
        if is_extract_cancelled() || found.len() >= wanted_salts {
            break;
        }
        let Ok(reader) = ProcessMemory::open(*pid) else {
            continue;
        };
        opened_pids.insert(*pid);
        let stats = scan_config_cipher_reader(&reader, targets, &mut found, SCAN_CHUNK_SIZE);
        config_totals.needle_occurrences += stats.needle_occurrences;
        config_totals.string_object_refs += stats.string_object_refs;
        config_totals.node_candidates += stats.node_candidates;
        config_totals.config_ptr_candidates += stats.config_ptr_candidates;
        config_totals.blob_count += stats.blob_count;
        config_totals.candidate_count += stats.candidate_count;
        config_totals.verified_candidates += stats.verified_candidates;
        config_totals.matched_salts += stats.matched_salts;
    }

    tracing::info!(
        process_count = pids.len(),
        opened_process_count = opened_pids.len(),
        needle_occurrences = config_totals.needle_occurrences,
        string_object_refs = config_totals.string_object_refs,
        node_candidates = config_totals.node_candidates,
        config_ptr_candidates = config_totals.config_ptr_candidates,
        blob_count = config_totals.blob_count,
        candidate_count = config_totals.candidate_count,
        verified_candidates = config_totals.verified_candidates,
        matched_salts = config_totals.matched_salts,
        "finished Windows Config.Cipher database key scan"
    );

    let mut legacy_totals = LegacyScanStats::default();
    if found.len() < wanted_salts && !is_extract_cancelled() {
        for pid in pids {
            if is_extract_cancelled() || found.len() >= wanted_salts {
                break;
            }
            let Ok(reader) = ProcessMemory::open(*pid) else {
                continue;
            };
            opened_pids.insert(*pid);
            let stats = scan_legacy_reader(&reader, targets, &mut found, SCAN_CHUNK_SIZE);
            legacy_totals.candidate_count += stats.candidate_count;
            legacy_totals.matched_salts += stats.matched_salts;
        }
    }

    tracing::info!(
        candidate_count = legacy_totals.candidate_count,
        matched_salts = legacy_totals.matched_salts,
        total_matched_salts = found.len(),
        target_salts = wanted_salts,
        "finished legacy database key scan fallback"
    );

    if opened_pids.is_empty() {
        return Err(anyhow!("无法读取微信进程内存，请尝试以管理员身份运行"));
    }
    Ok(format_found_keys(found))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use pbkdf2::pbkdf2_hmac_array;
    use sha2::Sha512;

    const PAGE_SIZE: usize = 4096;
    const SALT_SIZE: usize = 16;
    const HMAC_SIZE: usize = 64;
    const IV_SIZE: usize = 16;

    struct FakeMemory {
        regions: Vec<MemoryRegion>,
        segments: Vec<(usize, Vec<u8>)>,
    }

    impl FakeMemory {
        fn new(segments: Vec<(usize, Vec<u8>)>) -> Self {
            let regions = segments
                .iter()
                .map(|(base, data)| MemoryRegion {
                    base: *base,
                    size: data.len(),
                })
                .collect();
            Self { regions, segments }
        }
    }

    impl MemoryReader for FakeMemory {
        fn regions(&self) -> &[MemoryRegion] {
            &self.regions
        }

        fn read(&self, address: usize, size: usize) -> Option<Vec<u8>> {
            self.segments.iter().find_map(|(base, data)| {
                let offset = address.checked_sub(*base)?;
                let end = offset.checked_add(size)?;
                data.get(offset..end).map(|slice| slice.to_vec())
            })
        }
    }

    fn valid_target(key: [u8; 32], salt: [u8; 16]) -> V4DbTarget {
        let mut page = vec![0x5au8; PAGE_SIZE];
        page[..SALT_SIZE].copy_from_slice(&salt);
        let mac_salt: [u8; 16] = salt.map(|byte| byte ^ 0x3a);
        let mac_key = pbkdf2_hmac_array::<Sha512, 32>(&key, &mac_salt, 2);
        let data_end = PAGE_SIZE - (IV_SIZE + HMAC_SIZE) + IV_SIZE;
        let mut mac = Hmac::<Sha512>::new_from_slice(&mac_key).unwrap();
        mac.update(&page[SALT_SIZE..data_end]);
        mac.update(&1u32.to_le_bytes());
        page[data_end..data_end + HMAC_SIZE].copy_from_slice(&mac.finalize().into_bytes());
        V4DbTarget::from_first_page("message_0.db", page).unwrap()
    }

    fn encoded_blob(key: [u8; 32], salt: [u8; 16]) -> Vec<u8> {
        let literal = format!("x'{}{}'", hex::encode(key), hex::encode(salt));
        literal
            .bytes()
            .enumerate()
            .map(|(index, byte)| byte ^ CONFIG_XOR_MASK[index % CONFIG_XOR_MASK.len()])
            .collect()
    }

    fn config_graph(blob: Vec<u8>, valid_config_ptr: bool) -> FakeMemory {
        let main_base = 0x10_0000usize;
        let config_ptr = 0x20_0000usize;
        let blob_ptr = 0x30_0000usize;
        let mut main = vec![0u8; 320];
        let needle_offset = 60usize;
        main[needle_offset..needle_offset + CONFIG_CIPHER_NAME.len()]
            .copy_from_slice(CONFIG_CIPHER_NAME);
        let reference_offset = 176usize;
        let node_base = reference_offset - 0x10;
        main[reference_offset..reference_offset + 8]
            .copy_from_slice(&((main_base + needle_offset) as u64).to_le_bytes());
        main[reference_offset + 8..reference_offset + 16]
            .copy_from_slice(&(CONFIG_CIPHER_NAME.len() as u64).to_le_bytes());
        let stored_config_ptr = if valid_config_ptr {
            config_ptr as u64
        } else {
            0
        };
        main[node_base + 0x28..node_base + 0x30].copy_from_slice(&stored_config_ptr.to_le_bytes());

        let mut config = vec![0u8; 0xb0];
        config[0x90..0x98].copy_from_slice(&(blob_ptr as u64).to_le_bytes());
        config[0x98..0xa0].copy_from_slice(&(blob.len() as u64).to_le_bytes());
        FakeMemory::new(vec![
            (main_base, main),
            (config_ptr, config),
            (blob_ptr, blob),
        ])
    }

    #[test]
    fn decodes_config_cipher_literal() {
        let key = std::array::from_fn(|index| (index + 1) as u8);
        let salt = std::array::from_fn(|index| (index + 41) as u8);
        let candidates = decode_config_blob(&encoded_blob(key, salt));
        assert!(candidates.contains(&KeyCandidate {
            key,
            embedded_salt: Some(salt)
        }));
    }

    #[test]
    fn parses_key_only_and_extended_literals() {
        let key = std::array::from_fn(|index| (index + 1) as u8);
        let salt = std::array::from_fn(|index| (index + 41) as u8);
        let key_only = format!("x'{}'", hex::encode(key));
        assert!(
            parse_key_literals(key_only.as_bytes()).contains(&KeyCandidate {
                key,
                embedded_salt: None,
            })
        );

        let extended = format!(
            "X'{}{}{}'",
            hex::encode(key),
            hex::encode(salt),
            "ab".repeat(16)
        );
        assert!(
            parse_key_literals(extended.as_bytes()).contains(&KeyCandidate {
                key,
                embedded_salt: Some(salt),
            })
        );
    }

    #[test]
    fn rejects_low_entropy_and_invalid_literals() {
        assert!(parse_key_literals(b"x'0011'").is_empty());
        let all_zero = format!("x'{}'", "00".repeat(32));
        assert!(parse_key_literals(all_zero.as_bytes()).is_empty());
        let missing_quote = format!("x'{}", "12".repeat(32));
        assert!(parse_key_literals(missing_quote.as_bytes()).is_empty());
    }

    #[test]
    fn follows_config_graph_and_validates_hmac_across_chunk_boundary() {
        let key = std::array::from_fn(|index| (index + 1) as u8);
        let salt = std::array::from_fn(|index| (index + 81) as u8);
        let target = valid_target(key, salt);
        let memory = config_graph(encoded_blob(key, salt), true);
        let mut found = HashMap::new();
        let stats = scan_config_cipher_reader(&memory, &[target], &mut found, 64);

        assert_eq!(stats.needle_occurrences, 1);
        assert_eq!(stats.verified_candidates, 1);
        assert_eq!(found.get(&salt), Some(&key));
    }

    #[test]
    fn verifies_candidates_against_non_message_targets_too() {
        let message_key = std::array::from_fn(|index| (index + 1) as u8);
        let message_salt = std::array::from_fn(|index| (index + 41) as u8);
        let optional_key = std::array::from_fn(|index| (index + 81) as u8);
        let optional_salt = std::array::from_fn(|index| (index + 121) as u8);
        let targets = vec![
            valid_target(message_key, message_salt),
            valid_target(optional_key, optional_salt),
        ];
        let memory = config_graph(encoded_blob(optional_key, optional_salt), true);
        let mut found = HashMap::new();

        let stats = scan_config_cipher_reader(&memory, &targets, &mut found, 64);

        assert_eq!(stats.verified_candidates, 1);
        assert_eq!(found.get(&optional_salt), Some(&optional_key));
        assert!(!found.contains_key(&message_salt));
    }

    #[test]
    fn ignores_invalid_config_pointer() {
        let key = std::array::from_fn(|index| (index + 1) as u8);
        let salt = std::array::from_fn(|index| (index + 101) as u8);
        let target = valid_target(key, salt);
        let memory = config_graph(encoded_blob(key, salt), false);
        let mut found = HashMap::new();
        let stats = scan_config_cipher_reader(&memory, &[target], &mut found, 64);

        assert_eq!(stats.needle_occurrences, 1);
        assert!(found.is_empty());
    }

    #[test]
    fn legacy_literal_fallback_handles_chunk_boundary() {
        let key = std::array::from_fn(|index| (index + 1) as u8);
        let salt = std::array::from_fn(|index| (index + 121) as u8);
        let target = valid_target(key, salt);
        let literal = format!("x'{}{}'", hex::encode(key), hex::encode(salt));
        let mut data = vec![0u8; 60];
        data.extend_from_slice(literal.as_bytes());
        data.extend_from_slice(&[0u8; 32]);
        let memory = FakeMemory::new(vec![(0x40_0000, data)]);
        let mut found = HashMap::new();
        let stats = scan_legacy_reader(&memory, &[target], &mut found, 64);

        assert_eq!(stats.matched_salts, 1);
        assert_eq!(found.get(&salt), Some(&key));
    }

    #[test]
    fn formats_keys_deterministically_by_salt() {
        let key_a = [1u8; 32];
        let key_b = [2u8; 32];
        let salt_a = [3u8; 16];
        let salt_b = [4u8; 16];
        let found = HashMap::from([(salt_b, key_b), (salt_a, key_a)]);
        let formatted = format_found_keys(found);

        assert_eq!(
            formatted[0],
            format!("{}{}", hex::encode(key_a), hex::encode(salt_a))
        );
        assert_eq!(
            formatted[1],
            format!("{}{}", hex::encode(key_b), hex::encode(salt_b))
        );
    }
}
