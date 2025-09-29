mod winproc;
mod memory;
mod validator;
mod dat2img;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "cloudbak_rs", about = "WeChat v4 Key Extractor (Rust)")]
struct Args {
    /// Optional WeChat v4 data directory (contains db_storage/..)
    #[arg(long)]
    data_dir: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("Wechat v4 Key Extractor (Rust)");
    println!("==============================");
    println!("This tool extracts encryption keys from a running WeChat v4 process. Please ensure WeChat v4 is running.");
    println!("Limit Windows version: 4.0.3.36");
    println!("Limit macOS version: 4.0.3.80 (not supported by this binary)");
    println!("Ready to extract keys...");

    #[cfg(target_os = "windows")]
    {
        run_windows(args)?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        println!("This Rust tool currently supports Windows only.");
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn is_container_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "wechat files" || lower == "微信文件" || lower == "xwechat_files"
}

#[cfg(target_os = "windows")]
fn normalize_data_dir(input: &str) -> (Option<String>, Option<String>) {
    use std::path::PathBuf;

    let mut cand = PathBuf::from(input);

    // If user passed a file path, move to its directory
    if cand.is_file() {
        if let Some(parent) = cand.parent() { cand = parent.to_path_buf(); }
    }

    // If the path is a known container (WeChat Files/微信文件/xwechat_files), pick first account
    if let Some(name) = cand.file_name().and_then(|s| s.to_str()) {
        if is_container_name(name) {
            if let Ok(rd) = std::fs::read_dir(&cand) {
                for ent in rd.flatten() {
                    let acc_dir = ent.path();
                    if !acc_dir.is_dir() { continue; }
                    let session = acc_dir.join("db_storage").join("session").join("session.db");
                    if session.exists() {
                        let account_name = acc_dir.file_name().map(|s| s.to_string_lossy().to_string());
                        return (Some(acc_dir.to_string_lossy().to_string()), account_name);
                    }
                }
            }
        }
    }

    // Try to locate the nearest ancestor that looks like an account dir
    for _ in 0..4 {
        let session = cand.join("db_storage").join("session").join("session.db");
        if session.exists() {
            let account_name = cand.file_name().map(|s| s.to_string_lossy().to_string());
            return (Some(cand.to_string_lossy().to_string()), account_name);
        }
        if let Some(parent) = cand.parent() {
            cand = parent.to_path_buf();
        } else {
            break;
        }
    }

    // Fallback: return the path and best-effort name
    let account_name = cand.file_name().map(|s| s.to_string_lossy().to_string());
    (Some(cand.to_string_lossy().to_string()), account_name)
}

#[cfg(target_os = "windows")]
fn run_windows(args: Args) -> Result<()> {
    use winproc::find_wechat_v4_processes;
    use std::time::Instant;

    let mut procs = find_wechat_v4_processes()?;
    if procs.is_empty() {
        println!("No WeChat v4 process found.");
        return Ok(());
    }

    // Prefer first online process with data_dir (or override by CLI), else first
    if let Some(ref dir) = args.data_dir {
        let (norm_dir, norm_name) = normalize_data_dir(dir);
        for p in &mut procs {
            p.data_dir = norm_dir.clone();
            // Infer account name from provided path if missing or container given
            if p.account_name.is_none() || p.account_name.as_deref().map(|n| is_container_name(n)).unwrap_or(false) {
                p.account_name = norm_name.clone();
            }
        }
    }

    let proc = procs
        .iter()
        .find(|p| p.status == "online" && p.data_dir.is_some())
        .unwrap_or(&procs[0])
        .clone();

    println!("Found WeChat process: PID={}", proc.pid);
    println!(
        "Full Version: {}",
        proc.full_version.as_deref().unwrap_or("unknown")
    );
    println!("DataDir={}", proc.data_dir.as_deref().unwrap_or(""));
    println!("AccountName={}", proc.account_name.as_deref().unwrap_or(""));

    let start = Instant::now();
    let (data_key_hex, img_key_hex) = memory::extract_keys_windows(&proc)?;
    let elapsed = start.elapsed().as_secs_f32();
    println!("extract_keys 耗时: {:.2}s", elapsed);
    println!("Data Key: {}", data_key_hex.as_deref().unwrap_or(""));
    println!("Image Key: {}", img_key_hex.as_deref().unwrap_or(""));

    if let Some(dir) = proc.data_dir.as_deref() {
        if let Some(xor_key) = dat2img::scan_and_set_xor_key(dir)? {
            println!("XOR Key: {}", xor_key);
        }
    }

    Ok(())
}
