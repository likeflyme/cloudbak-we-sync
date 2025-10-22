use anyhow::Result;
use std::path::{Path, PathBuf};
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

#[derive(Debug, Clone)]
pub struct WeChatProcess {
    pub pid: i32,
    pub exe_path: String,
    pub version: i32,
    pub status: String,
    pub data_dir: Option<String>,
    pub account_name: Option<String>,
    pub full_version: Option<String>,
}

fn strip_exe(name: &str) -> &str { name.strip_suffix(".exe").unwrap_or(name) }

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;

#[cfg(target_os = "windows")]
fn get_file_version(path: &str) -> Option<String> {
    use winapi::shared::minwindef::{DWORD, LPVOID, LPCVOID, UINT};
    use winapi::um::winver::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};

    let wide: Vec<u16> = Path::new(path).as_os_str().encode_wide().chain([0]).collect();
    unsafe {
        let mut handle: DWORD = 0;
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle);
        if size == 0 { return None; }
        let mut buf = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, buf.as_mut_ptr() as LPVOID) == 0 { return None; }
        let mut block_ptr: LPVOID = std::ptr::null_mut();
        let mut len: UINT = 0;
        let subblock: Vec<u16> = "\\".encode_utf16().chain([0]).collect();
        if VerQueryValueW(buf.as_ptr() as LPCVOID, subblock.as_ptr(), &mut block_ptr, &mut len) == 0 { return None; }
        if block_ptr.is_null() { return None; }
        // VS_FIXEDFILEINFO layout: DWORD dwSignature; DWORD dwStrucVersion; DWORD dwFileVersionMS; DWORD dwFileVersionLS; ...
        let dwords = std::slice::from_raw_parts(block_ptr as *const u32, (len as usize) / 4);
        if dwords.len() < 4 { return None; }
        let hi = dwords[2];
        let lo = dwords[3];
        let major = (hi >> 16) & 0xFFFF;
        let minor = hi & 0xFFFF;
        let build = (lo >> 16) & 0xFFFF;
        let rev = lo & 0xFFFF;
        Some(format!("{}.{}.{}.{}", major, minor, build, rev))
    }
}

#[cfg(not(target_os = "windows"))]
fn get_file_version(_path: &str) -> Option<String> { None }

fn is_container_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "wechat files" || lower == "微信文件" || lower == "xwechat_files"
}

fn find_account_under_container(container: &Path) -> Option<(String, String)> {
    if !container.exists() { return None; }
    if let Ok(rd) = std::fs::read_dir(container) {
        for ent in rd.flatten() {
            let acc_dir = ent.path();
            if !acc_dir.is_dir() { continue; }
            let session = acc_dir.join("db_storage").join("session").join("session.db");
            if session.exists() {
                let account_name = ent.file_name().to_string_lossy().to_string();
                return Some((acc_dir.to_string_lossy().to_string(), account_name));
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn get_container_from_registry() -> Option<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
    use windows::Win32::System::Registry::{HKEY, RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_SZ, REG_EXPAND_SZ, REG_VALUE_TYPE};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    unsafe {
        let subkey_w: Vec<u16> = OsStr::new("SOFTWARE\\Tencent\\WeChat").encode_wide().chain([0]).collect();
        let value_w: Vec<u16> = OsStr::new("FileSavePath").encode_wide().chain([0]).collect();
        let mut hkey: HKEY = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey_w.as_ptr()), 0, KEY_READ, &mut hkey).is_err() {
            return None;
        }
        // Query size first
        let mut typ: REG_VALUE_TYPE = REG_VALUE_TYPE(0);
        let mut cb: u32 = 0;
        let res1 = RegQueryValueExW(hkey, PCWSTR(value_w.as_ptr()), None, Some(&mut typ as *mut _), None, Some(&mut cb));
        if res1.is_err() || (typ != REG_SZ && typ != REG_EXPAND_SZ) { let _ = RegCloseKey(hkey); return None; }
        if cb == 0 { let _ = RegCloseKey(hkey); return None; }
        let mut buf: Vec<u16> = vec![0u16; (cb as usize + 1) / 2];
        let res2 = RegQueryValueExW(hkey, PCWSTR(value_w.as_ptr()), None, Some(&mut typ as *mut _), Some(buf.as_mut_ptr() as *mut u8), Some(&mut cb));
        let _ = RegCloseKey(hkey);
        if res2.is_err() { return None; }
        if let Some(nul_pos) = buf.iter().position(|&c| c == 0) { buf.truncate(nul_pos); }
        let mut path_str = String::from_utf16_lossy(&buf);
        if typ == REG_EXPAND_SZ {
            // Expand env vars like %USERPROFILE%
            let in_w: Vec<u16> = path_str.encode_utf16().chain([0]).collect();
            let mut out: Vec<u16> = vec![0u16; 1024];
            let n = ExpandEnvironmentStringsW(PCWSTR(in_w.as_ptr()), Some(&mut out));
            if n > 0 {
                if let Some(z) = out.iter().position(|&c| c == 0) { out.truncate(z); }
                path_str = String::from_utf16_lossy(&out);
            }
        }
        let path = PathBuf::from(path_str);
        if path.as_os_str().is_empty() { None } else { Some(path) }
    }
}

#[cfg(not(target_os = "windows"))]
fn get_container_from_registry() -> Option<PathBuf> { None }

fn candidate_doc_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    // Prefer registry FileSavePath if available; restrict search to this base to avoid false positives
    if let Some(reg_path) = get_container_from_registry() {
        roots.push(reg_path.clone());
        roots.push(reg_path.join("WeChat Files"));
        roots.push(reg_path.join("微信文件"));
        roots.push(reg_path.join("WeChat Files").join("xwechat_files"));
        roots.push(reg_path.join("微信文件").join("xwechat_files"));
        roots.sort();
        roots.dedup();
        return roots;
    }
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        // Direct document folders and OneDrive variants
        let docs = [home.join("Documents"), home.join("文档")];
        let onedrive_docs = [home.join("OneDrive").join("Documents"), home.join("OneDrive").join("文档")];
        for d in docs.iter().chain(onedrive_docs.iter()) {
            roots.push(d.clone());
            roots.push(d.join("WeChat Files"));
            roots.push(d.join("微信文件"));
            roots.push(d.join("WeChat Files").join("xwechat_files"));
            roots.push(d.join("微信文件").join("xwechat_files"));
        }
        if let Ok(rd) = std::fs::read_dir(&home) {
            for ent in rd.flatten() {
                let name = ent.file_name().to_string_lossy().to_string();
                if name.starts_with("OneDrive") {
                    let d1 = ent.path().join("Documents");
                    let d2 = ent.path().join("文档");
                    for d in [d1, d2] {
                        roots.push(d.clone());
                        roots.push(d.join("WeChat Files"));
                        roots.push(d.join("微信文件"));
                        roots.push(d.join("WeChat Files").join("xwechat_files"));
                        roots.push(d.join("微信文件").join("xwechat_files"));
                    }
                }
            }
        }
        // Also consider typical containers under home (if relocated from Documents)
        roots.push(home.join("WeChat Files"));
        roots.push(home.join("微信文件"));
        roots.push(home.join("微信文件").join("xwechat_files"));
    }
    for drive in ["C:\\", "D:\\", "E:\\", "F:\\"] {
        let base = Path::new(drive);
        roots.push(base.join("WeChat Files"));
        roots.push(base.join("微信文件"));
        roots.push(base.join("WeChat Files").join("xwechat_files"));
        roots.push(base.join("微信文件").join("xwechat_files"));
    }
    roots.sort();
    roots.dedup();
    roots
}

#[cfg(target_os = "windows")]
fn infer_from_open_handles(pid: u32) -> Option<(String, String)> {
    use std::mem::size_of;
    use std::path::PathBuf;
    use winapi::ctypes::c_void;
    use winapi::shared::minwindef::{DWORD, FARPROC, HMODULE};
    use winapi::shared::ntdef::NTSTATUS;
    use winapi::um::fileapi::GetFinalPathNameByHandleW;
    use winapi::um::handleapi::{CloseHandle, DuplicateHandle};
    use winapi::um::libloaderapi::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
    use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcess};
    use winapi::um::winnt::{HANDLE, PROCESS_DUP_HANDLE};

    #[allow(non_camel_case_types, non_snake_case)]
    #[repr(C)]
    struct SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX {
        Object: *mut c_void,
        UniqueProcessId: usize,
        HandleValue: usize,
        GrantedAccess: u32,
        CreatorBackTraceIndex: u16,
        ObjectTypeIndex: u16,
        HandleAttributes: u32,
        Reserved: u32,
    }

    type NtQuerySystemInformationFn = unsafe extern "system" fn(
        SystemInformationClass: u32,
        SystemInformation: *mut c_void,
        SystemInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> NTSTATUS;

    unsafe {
        // Load ntdll and resolve NtQuerySystemInformation
        let ntdll_w: Vec<u16> = "ntdll.dll".encode_utf16().chain([0]).collect();
        let mut ntdll: HMODULE = GetModuleHandleW(ntdll_w.as_ptr());
        if ntdll.is_null() {
            ntdll = LoadLibraryW(ntdll_w.as_ptr());
            if ntdll.is_null() { return None; }
        }
        let proc: FARPROC = GetProcAddress(ntdll, b"NtQuerySystemInformation\0".as_ptr() as *const i8);
        if proc.is_null() { return None; }
        let ntqsi: NtQuerySystemInformationFn = std::mem::transmute(proc);

        // Query system handles into a buffer
        const SYSTEM_EXTENDED_HANDLE_INFORMATION: u32 = 0x40;
        let mut len: u32 = 1 << 20; // start with 1 MB
        let mut buffer: Vec<u8> = Vec::new();
        let mut ret: u32 = 0;
        loop {
            buffer.resize(len as usize, 0u8);
            let status = ntqsi(
                SYSTEM_EXTENDED_HANDLE_INFORMATION,
                buffer.as_mut_ptr() as *mut c_void,
                len,
                &mut ret,
            );
            // STATUS_INFO_LENGTH_MISMATCH = 0xC0000004
            if (status as u32) == 0xC0000004 {
                let new_len = if ret > len { ret } else { len.saturating_mul(2) };
                if new_len <= len { return None; }
                len = new_len;
                continue;
            }
            // NT_SUCCESS(status)
            if (status as i32) < 0 { return None; }
            break;
        }

        let base = buffer.as_ptr();
        let count = *(base as *const usize);
        let entries_ptr = base.add(2 * size_of::<usize>()) as *const SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX;

        // Open target process for handle duplication
        let h_src: HANDLE = OpenProcess(PROCESS_DUP_HANDLE, 0, pid);
        if h_src.is_null() { return None; }
        let h_dst: HANDLE = GetCurrentProcess();

        let mut result: Option<(String, String)> = None;

        for i in 0..count {
            let entry = &*entries_ptr.add(i);
            if entry.UniqueProcessId as u32 != pid { continue; }

            // Duplicate the handle into current process
            let mut dup: HANDLE = std::ptr::null_mut();
            let src_handle: HANDLE = entry.HandleValue as usize as isize as *mut c_void;
            let ok = DuplicateHandle(
                h_src,
                src_handle,
                h_dst,
                &mut dup,
                0,
                0,
                0x00000002, // DUPLICATE_SAME_ACCESS
            );
            if ok == 0 || dup.is_null() { continue; }

            // Try resolve to a file path
            let mut wbuf: Vec<u16> = vec![0u16; 32768];
            let n = GetFinalPathNameByHandleW(dup, wbuf.as_mut_ptr(), wbuf.len() as DWORD, 0);
            if n > 0 && (n as usize) < wbuf.len() {
                let len = n as usize;
                let path = String::from_utf16_lossy(&wbuf[..len]);
                let mut low = path.to_ascii_lowercase();
                if let Some(stripped) = low.strip_prefix("\\\\?\\") { low = stripped.to_string(); }
                if low.ends_with("db_storage\\session\\session.db") || low.ends_with("db_storage/session/session.db") {
                    // account dir = parent of db_storage
                    let acc_dir = PathBuf::from(&path)
                        .parent().and_then(|p| p.parent()).and_then(|p| p.parent())
                        .map(|p| p.to_string_lossy().to_string());
                    if let Some(acc) = acc_dir {
                        let name = PathBuf::from(&acc)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        result = Some((acc, name));
                        CloseHandle(dup);
                        break;
                    }
                }
            }
            CloseHandle(dup);
        }
        CloseHandle(h_src);
        result
    }
}

fn try_infer_data_dir() -> Option<(String, String)> {
    #[cfg(target_os = "windows")]
    {
        // Try handles first for exact match
        let mut sys = System::new();
        sys.refresh_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::everything()));
        for (_pid, p) in sys.processes() {
            if strip_exe(p.name()) == "Weixin" {
                if let Some((d, a)) = infer_from_open_handles(p.pid().as_u32()) { return Some((d, a)); }
            }
        }
    }

    // Fallback: registry/doc roots search
    for base in candidate_doc_roots() {
        if !base.exists() { continue; }
        if let Some(name) = base.file_name().and_then(|s| s.to_str()) {
            if is_container_name(name) || name.eq_ignore_ascii_case("xwechat_files") {
                if let Some(pair) = find_account_under_container(&base) { return Some(pair); }
            }
        }
        for cname in ["WeChat Files", "微信文件", "xwechat_files"] {
            let cont = base.join(cname);
            if cont.exists() {
                if let Some(pair) = find_account_under_container(&cont) { return Some(pair); }
            }
        }
        for entry in walkdir::WalkDir::new(&base).max_depth(5).into_iter().flatten() {
            if !entry.file_type().is_file() { continue; }
            if entry.file_name().to_string_lossy().eq_ignore_ascii_case("session.db") {
                let p = entry.path();
                if let Some(session_dir) = p.parent() {
                    if session_dir.file_name().and_then(|s| s.to_str()) == Some("session") {
                        if let Some(db_storage_dir) = session_dir.parent() {
                            if db_storage_dir.file_name().and_then(|s| s.to_str()) == Some("db_storage") {
                                if let Some(account_dir) = db_storage_dir.parent() {
                                    let account_name = account_dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                                    return Some((account_dir.to_string_lossy().to_string(), account_name));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn find_wechat_v4_processes() -> Result<Vec<WeChatProcess>> {
    let mut sys = System::new();
    sys.refresh_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::everything()));

    let mut results = Vec::new();
    for (_pid, p) in sys.processes() {
        let name = strip_exe(p.name());
        if name != "Weixin" { continue; }
        let cmdline = p.cmd().join(" ");
        if cmdline.contains("--") { continue; }
        let exe_path = p.exe().map(|pp| pp.to_string_lossy().to_string()).unwrap_or_default();
        let full_version = if !exe_path.is_empty() { get_file_version(&exe_path) } else { None };

        let (mut data_dir, mut account_name) = (None, None);
        if let Some((d, a)) = try_infer_data_dir() { data_dir = Some(d); account_name = Some(a); }

        let status = if data_dir.is_some() { "online" } else { "offline" }.to_string();
        results.push(WeChatProcess {
            pid: p.pid().as_u32() as i32,
            exe_path,
            version: 4,
            status,
            data_dir,
            account_name,
            full_version,
        });
    }

    Ok(results)
}

pub fn find_wechat_v3_processes() -> Result<Vec<WeChatProcess>> {
    let mut sys = System::new();
    sys.refresh_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::everything()));
    let mut results = Vec::new();
    for (_pid, p) in sys.processes() {
        let name = strip_exe(p.name());
        if name != "WeChat" { continue; }
        let exe_path = p.exe().map(|pp| pp.to_string_lossy().to_string()).unwrap_or_default();
        let full_version = if !exe_path.is_empty() { get_file_version(&exe_path) } else { None };
        let (mut data_dir, mut account_name) = (None, None);
        if let Some((d, a)) = try_infer_data_dir() { data_dir = Some(d); account_name = Some(a); }
        let status = if data_dir.is_some() { "online" } else { "offline" }.to_string();
        results.push(WeChatProcess {
            pid: p.pid().as_u32() as i32,
            exe_path,
            version: 3,
            status,
            data_dir,
            account_name,
            full_version,
        });
    }
    Ok(results)
}
