use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};

const IV_SIZE: usize = 16;
const HMAC_SHA512_SIZE: usize = 64;
const KEY_SIZE: usize = 32;
const AES_BLOCK_SIZE: usize = 16;
const ROUND_COUNT: u64 = 256000;
const PAGE_SIZE: usize = 4096;
const SALT_SIZE: usize = 16;
const SQLITE_HEADER: &[u8] = b"SQLite format 3\x00";

#[cfg(target_os = "windows")]
fn pbkdf2_hmac_sha512(password: &[u8], salt: &[u8], iterations: u64, out: &mut [u8]) -> Result<()> {
    use windows::Win32::Security::Cryptography::{BCryptCloseAlgorithmProvider, BCryptDeriveKeyPBKDF2, BCryptOpenAlgorithmProvider, BCRYPT_ALG_HANDLE, BCRYPT_ALG_HANDLE_HMAC_FLAG, BCRYPT_SHA512_ALGORITHM};
    unsafe {
        let mut h_alg = BCRYPT_ALG_HANDLE::default();
        let status = BCryptOpenAlgorithmProvider(&mut h_alg, BCRYPT_SHA512_ALGORITHM, None, BCRYPT_ALG_HANDLE_HMAC_FLAG);
        if status.is_err() { return Err(anyhow!("BCryptOpenAlgorithmProvider failed: {:?}", status)); }
        let status = BCryptDeriveKeyPBKDF2(h_alg, Some(password), Some(salt), iterations as u64, out, 0);
        let _ = BCryptCloseAlgorithmProvider(h_alg, 0);
        if status.is_err() { return Err(anyhow!("BCryptDeriveKeyPBKDF2 failed: {:?}", status)); }
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn pbkdf2_hmac_sha512(_password: &[u8], _salt: &[u8], _iterations: u64, _out: &mut [u8]) -> Result<()> {
    Err(anyhow!("PBKDF2 not implemented for this platform"))
}

fn compute_hmac(mac_key: &[u8], data: &[u8], page_number: u32) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;
    let mut mac = Hmac::<Sha512>::new_from_slice(mac_key).unwrap();
    mac.update(data);
    mac.update(&page_number.to_le_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn decrypt_db_v4(buf: &[u8], pkey_hex: &str) -> Result<Vec<u8>> {
    if buf.starts_with(SQLITE_HEADER) {
        return Ok(buf.to_vec());
    }
    if buf.len() < PAGE_SIZE { return Err(anyhow!("DB too small")); }

    let salt = &buf[..SALT_SIZE];
    let mut mac_salt = [0u8; SALT_SIZE];
    for i in 0..SALT_SIZE { mac_salt[i] = salt[i] ^ 0x3a; }

    let pass_key = hex::decode(pkey_hex)?;
    if pass_key.len() != KEY_SIZE { return Err(anyhow!("data key length != 32")); }

    let mut key = [0u8; KEY_SIZE];
    pbkdf2_hmac_sha512(&pass_key, salt, ROUND_COUNT, &mut key)?;

    let mut mac_key = [0u8; KEY_SIZE];
    pbkdf2_hmac_sha512(&key, &mac_salt, 2, &mut mac_key)?;

    let mut out = Vec::with_capacity(buf.len() + 1);
    out.extend_from_slice(&SQLITE_HEADER[..SQLITE_HEADER.len() - 1]);
    out.push(0x00);

    let reserve_unaligned = IV_SIZE + HMAC_SHA512_SIZE;
    let reserve = ((reserve_unaligned + AES_BLOCK_SIZE - 1) / AES_BLOCK_SIZE) * AES_BLOCK_SIZE;
    let total_page = buf.len() / PAGE_SIZE;

    use aes::cipher::{KeyIvInit, BlockDecryptMut};
    use cbc::cipher::block_padding::NoPadding;
    type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

    for cur_page in 0..total_page {
        let offset = if cur_page == 0 { SALT_SIZE } else { 0 };
        let start = cur_page * PAGE_SIZE;
        let end = start + PAGE_SIZE;
        if end > buf.len() { break; }

        let mac_data_start = start + offset;
        let mac_data_end = end - reserve + IV_SIZE;
        if mac_data_end > end { return Err(anyhow!("invalid page layout")); }

        let expected = compute_hmac(&mac_key, &buf[mac_data_start..mac_data_end], (cur_page as u32) + 1);
        let actual_start = end - reserve + IV_SIZE;
        let actual_end = actual_start + expected.len();
        if actual_end > end { return Err(anyhow!("invalid mac region")); }
        let actual = &buf[actual_start..actual_end];
        if actual.iter().any(|&b| b != 0) && actual != expected.as_slice() {
            return Err(anyhow!("HMAC verification failed at page {}", cur_page + 1));
        }

        let iv = &buf[end - reserve .. end - reserve + IV_SIZE];
        let cipher = Aes256CbcDec::new_from_slices(&key, iv).map_err(|e| anyhow!("cipher init: {:?}", e))?;
        let mut page_buf = buf[mac_data_start.. end - reserve].to_vec();
        let dec_slice = cipher.decrypt_padded_mut::<NoPadding>(&mut page_buf)
            .map_err(|_| anyhow!("AES decrypt failed"))?;
        out.extend_from_slice(dec_slice);
        out.extend_from_slice(&buf[end - reserve .. end]);
    }

    Ok(out)
}

fn trimmed_username(wx_id: &str) -> String {
    if let Some((left, _)) = wx_id.rsplit_once('_') { left.to_string() } else { wx_id.to_string() }
}

fn app_data_dir() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Some(home) = std::env::var_os("USERPROFILE") { return Ok(PathBuf::from(home).join(".we-sync")); }
        if let Some(home) = std::env::var_os("HOMEPATH") { return Ok(PathBuf::from(home).join(".we-sync")); }
        Err(anyhow!("cannot resolve USERPROFILE"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = dirs_next::home_dir() { return Ok(home.join(".we-sync")); }
        Err(anyhow!("cannot resolve home_dir"))
    }
}

pub fn extract_avatar_to_appdata(data_dir: &str, data_key_hex: &str, wx_id: &str) -> Option<String> {
    let db_path = Path::new(data_dir).join("db_storage").join("head_image").join("head_image.db");
    if !db_path.exists() { return None; }
    let buf = fs::read(&db_path).ok()?;
    let dec = decrypt_db_v4(&buf, data_key_hex).ok()?;

    // write to temp file and query via rusqlite
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("we_sync_head_image_{}.db", std::process::id()));
    fs::write(&tmp, &dec).ok()?;

    let uname = trimmed_username(wx_id);
    let conn = rusqlite::Connection::open(&tmp).ok()?;
    let mut stmt = conn.prepare("SELECT image_buffer FROM head_image WHERE username = ?1").ok()?;
    let mut rows = stmt.query([uname.as_str()]).ok()?;
    let row = if let Some(r) = rows.next().ok()? { Some(r) } else { None };
    let img: Option<Vec<u8>> = match row { Some(r) => r.get(0).ok(), None => None };
    drop(rows);
    drop(stmt);
    drop(conn);
    let _ = fs::remove_file(&tmp);

    if let Some(bytes) = img {
        let base = app_data_dir().ok()?;
        let avatars = base.join("avatars");
        fs::create_dir_all(&avatars).ok()?;
        // decide extension
        let ext = if bytes.len() >= 4 && &bytes[..2] == b"\xFF\xD8" { "jpg" }
                  else if bytes.len() >= 8 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" { "png" }
                  else { "img" };
        let file_name = format!("{}.{}", uname, ext);
        let out_path = avatars.join(file_name);
        fs::write(&out_path, &bytes).ok()?;
        return Some(out_path.to_string_lossy().to_string())
    }
    return None;
}
