use anyhow::Result;
use hmac::{Hmac, Mac};
use sha2::Sha512;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use windows::Win32::Security::Cryptography::{
    BCryptCloseAlgorithmProvider, BCryptDeriveKeyPBKDF2, BCryptOpenAlgorithmProvider,
    BCRYPT_ALG_HANDLE, BCRYPT_ALG_HANDLE_HMAC_FLAG, BCRYPT_SHA512_ALGORITHM,
};

const KEY_SIZE: usize = 32;
const SALT_SIZE: usize = 16;
const AES_BLOCK: usize = 16;
const IV_SIZE: usize = 16;
const V4_ITER: u32 = 256000;
const V3_ITER: u32 = 64000; // placeholder iteration count for v3 (TODO: adjust to real value)
const HMAC_SHA512_SIZE: usize = 64;
const PAGE_SIZE: usize = 4096;

pub struct DBValidator {
    first_page: Vec<u8>,
    salt: [u8; SALT_SIZE],
}

impl DBValidator {
    pub fn new(data_dir: &str) -> Result<Self> {
        let message_dir = Path::new(data_dir).join("db_storage").join("message");
        let mut candidates: Vec<PathBuf> = Vec::new();
        candidates.push(message_dir.join("message_0.db"));
        // Fallback: enumerate message_*.db
        if let Ok(rd) = fs::read_dir(&message_dir) {
            for ent in rd.flatten() {
                let p = ent.path();
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with("message_") && name.ends_with(".db") && p != candidates[0] {
                        candidates.push(p);
                    }
                }
            }
        }
        let mut first_page: Option<Vec<u8>> = None;
        for cand in candidates {
            if !cand.exists() { continue; }
            // Read only the first page (4KB) instead of the whole file
            match std::fs::File::open(&cand).and_then(|mut f| {
                use std::io::Read;
                let mut page = vec![0u8; PAGE_SIZE];
                // ensure we read PAGE_SIZE; if file is shorter, this will error
                f.read_exact(&mut page)?;
                Ok(page)
            }) {
                Ok(buf) => {
                    // if starts with SQLite header => already decrypted, skip
                    if &buf[..15] == b"SQLite format 3" { continue; }
                    println!("[dbg] Using DB file: {}", cand.to_string_lossy());
                    first_page = Some(buf);
                    break;
                }
                Err(_) => continue,
            }
        }
        let first_page = first_page.ok_or_else(|| anyhow::anyhow!("No suitable encrypted message_*.db found"))?;
        let mut salt = [0u8; SALT_SIZE];
        salt.copy_from_slice(&first_page[..SALT_SIZE]);
        Ok(Self{ first_page, salt })
    }

    #[cfg(target_os = "windows")]
    fn pbkdf2_hmac_sha512(password: &[u8], salt: &[u8], iterations: u64, out: &mut [u8]) -> Result<()> {
        unsafe {
            let mut h_alg = BCRYPT_ALG_HANDLE::default();
            // Open SHA512 provider in HMAC mode for PBKDF2-HMAC-SHA512
            let status = BCryptOpenAlgorithmProvider(
                &mut h_alg,
                BCRYPT_SHA512_ALGORITHM,
                None,
                BCRYPT_ALG_HANDLE_HMAC_FLAG,
            );
            if status.is_err() { return Err(anyhow::anyhow!("BCryptOpenAlgorithmProvider failed: {:?}", status)); }

            let status = BCryptDeriveKeyPBKDF2(
                h_alg,
                Some(password),
                Some(salt),
                iterations as u64,
                out,
                0,
            );

            // Close provider regardless of derive result
            let _ = BCryptCloseAlgorithmProvider(h_alg, 0);

            if status.is_err() {
                return Err(anyhow::anyhow!("BCryptDeriveKeyPBKDF2 failed: {:?}", status));
            }
            Ok(())
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn pbkdf2_hmac_sha512(_password: &[u8], _salt: &[u8], _iterations: u64, _out: &mut [u8]) -> Result<()> {
        // Non-Windows builds of this project are not supported currently.
        Err(anyhow::anyhow!("PBKDF2 not implemented for this platform"))
    }

    fn derive_keys(db_key: &[u8; KEY_SIZE], salt: &[u8; SALT_SIZE]) -> ([u8; KEY_SIZE], [u8; KEY_SIZE]) {
        let mut enc_key = [0u8; KEY_SIZE];
        // V4 PBKDF2-HMAC-SHA512
        Self::pbkdf2_hmac_sha512(db_key, salt, V4_ITER as u64, &mut enc_key)
            .expect("PBKDF2 (enc_key) failed");

        let mut mac_salt = [0u8; SALT_SIZE];
        for i in 0..SALT_SIZE { mac_salt[i] = salt[i] ^ 0x3a; }

        let mut mac_key = [0u8; KEY_SIZE];
        Self::pbkdf2_hmac_sha512(&enc_key, &mac_salt, 2u64, &mut mac_key)
            .expect("PBKDF2 (mac_key) failed");
        (enc_key, mac_key)
    }

    pub fn validate_db_key(&self, key: &[u8]) -> bool {
        if key.len() != KEY_SIZE { return false; }
        let mut k = [0u8; KEY_SIZE]; k.copy_from_slice(key);
        let (_enc_key, mac_key) = Self::derive_keys(&k, &self.salt);
        let data_end = PAGE_SIZE - (IV_SIZE + HMAC_SHA512_SIZE) + IV_SIZE;
        let mut mac = Hmac::<Sha512>::new_from_slice(&mac_key).unwrap();
        mac.update(&self.first_page[SALT_SIZE..data_end]);
        mac.update(&(1u32.to_le_bytes()));
        let calc = mac.finalize().into_bytes();
        let stored = &self.first_page[data_end..data_end+HMAC_SHA512_SIZE];
        subtle::ConstantTimeEq::ct_eq(&calc[..], stored).unwrap_u8() == 1
    }
}

pub struct DBValidatorV3 {
    first_page: Vec<u8>,
    salt: [u8; SALT_SIZE],
}

impl DBValidatorV3 {
    pub fn new(data_dir: &str) -> Result<Self> {
        // Reuse logic from DBValidator.new
        let message_dir = Path::new(data_dir).join("db_storage").join("message");
        let mut candidates: Vec<PathBuf> = Vec::new();
        candidates.push(message_dir.join("message_0.db"));
        if let Ok(rd) = std::fs::read_dir(&message_dir) {
            for ent in rd.flatten() {
                let p = ent.path();
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with("message_") && name.ends_with(".db") && p != candidates[0] {
                        candidates.push(p);
                    }
                }
            }
        }
        let mut first_page: Option<Vec<u8>> = None;
        for cand in candidates {
            if !cand.exists() { continue; }
            match std::fs::File::open(&cand).and_then(|mut f| { use std::io::Read; let mut page = vec![0u8; PAGE_SIZE]; f.read_exact(&mut page)?; Ok(page) }) {
                Ok(buf) => {
                    if &buf[..15] == b"SQLite format 3" { continue; } // skip already decrypted
                    println!("[dbg] (v3) Using DB file: {}", cand.to_string_lossy());
                    first_page = Some(buf);
                    break;
                }
                Err(_) => continue,
            }
        }
        let first_page = first_page.ok_or_else(|| anyhow::anyhow!("(v3) No suitable encrypted message_*.db found"))?;
        let mut salt = [0u8; SALT_SIZE];
        salt.copy_from_slice(&first_page[..SALT_SIZE]);
        Ok(Self { first_page, salt })
    }

    fn derive_keys(&self, db_key: &[u8; KEY_SIZE]) -> ([u8; KEY_SIZE], [u8; KEY_SIZE]) {
        let mut enc_key = [0u8; KEY_SIZE];
        // PBKDF2 for v3 (placeholder iterations)
        DBValidator::pbkdf2_hmac_sha512(db_key, &self.salt, V3_ITER as u64, &mut enc_key)
            .expect("PBKDF2 v3 enc_key failed");
        let mut mac_salt = [0u8; SALT_SIZE];
        for i in 0..SALT_SIZE { mac_salt[i] = self.salt[i] ^ 0x3a; }
        let mut mac_key = [0u8; KEY_SIZE];
        DBValidator::pbkdf2_hmac_sha512(&enc_key, &mac_salt, 2u64, &mut mac_key)
            .expect("PBKDF2 v3 mac_key failed");
        (enc_key, mac_key)
    }

    pub fn validate_db_key_v3(&self, key: &[u8]) -> bool {
        if key.len() != KEY_SIZE { return false; }
        let mut k = [0u8; KEY_SIZE]; k.copy_from_slice(key);
        let (_enc_key, mac_key) = self.derive_keys(&k);
        let data_end = PAGE_SIZE - (IV_SIZE + HMAC_SHA512_SIZE) + IV_SIZE;
        let mut mac = Hmac::<Sha512>::new_from_slice(&mac_key).unwrap();
        mac.update(&self.first_page[SALT_SIZE..data_end]);
        mac.update(&(1u32.to_le_bytes()));
        let calc = mac.finalize().into_bytes();
        let stored = &self.first_page[data_end..data_end+HMAC_SHA512_SIZE];
        subtle::ConstantTimeEq::ct_eq(&calc[..], stored).unwrap_u8() == 1
    }
}

pub struct ImgKeyValidator {
    encrypted_block: Option<[u8; 16]>,
}

impl ImgKeyValidator {
    pub fn new(data_dir: &str) -> Result<Self> {
        use walkdir::WalkDir;
        use std::path::PathBuf;

        fn read_block(path: &Path) -> Option<[u8; 16]> {
            let mut buf = [0u8; 31];
            if let Ok(mut f) = std::fs::File::open(path) {
                use std::io::Read;
                if f.read_exact(&mut buf).is_ok() && buf.len() >= 31 {
                    // Both 5632 and 5631 keep the first encrypted block at 15..31
                    let mut blk = [0u8; 16];
                    blk.copy_from_slice(&buf[15..31]);
                    return Some(blk);
                }
            }
            None
        }

        let mut enc: Option<[u8; 16]> = None;
        let mut picked: Option<PathBuf> = None;
        let mut total_dat = 0usize;
        let mut v631_dat = 0usize;
        let mut v632_dat = 0usize;

        // Collect candidates by priority
        let mut cand_v632_main: Vec<PathBuf> = Vec::new();
        let mut cand_v632_thumb: Vec<PathBuf> = Vec::new();
        let mut cand_v631_main: Vec<PathBuf> = Vec::new();
        let mut cand_v631_thumb: Vec<PathBuf> = Vec::new();

        for e in WalkDir::new(data_dir).into_iter().flatten() {
            if !e.file_type().is_file() { continue; }
            let name_os = e.file_name();
            let name = name_os.to_string_lossy().to_lowercase();
            if !name.ends_with(".dat") { continue; }
            let is_thumb = name.ends_with("_t.dat");
            total_dat += 1;
            if total_dat % 1000 == 0 { println!("[dbg] ImgKey scan progress: {} .dat files", total_dat); }
            // EARLY STOP: if already picked, break to avoid scanning rest
            if enc.is_some() { break; }

            // Read small header
            let mut hdr = [0u8; 4];
            if let Ok(mut f) = std::fs::File::open(e.path()) {
                use std::io::Read;
                if f.read_exact(&mut hdr).is_err() { continue; }
            } else { continue; }

            if &hdr == b"\x07\x08\x56\x32" {
                v632_dat += 1;
                if is_thumb { cand_v632_thumb.push(e.path().to_path_buf()); }
                else {
                    cand_v632_main.push(e.path().to_path_buf());
                    // Prefer first v632 main image: try immediately and early-break if success
                    if enc.is_none() {
                        if let Some(blk) = read_block(e.path()) {
                            picked = Some(e.path().to_path_buf());
                            enc = Some(blk);
                            println!("[dbg] Early img encrypted block picked (v632 main): {}", e.path().display());
                            break; // early stop scanning
                        }
                    }
                }
            } else if &hdr == b"\x07\x08\x56\x31" {
                v631_dat += 1;
                if is_thumb { cand_v631_thumb.push(e.path().to_path_buf()); }
                else { cand_v631_main.push(e.path().to_path_buf()); }
            }
        }

        // Only run fallback candidate search if early pick not found
        if enc.is_none() {
            for lst in [&cand_v632_main, &cand_v632_thumb, &cand_v631_main, &cand_v631_thumb] {
                if enc.is_some() { break; }
                for p in lst {
                    if let Some(blk) = read_block(p) {
                        picked = Some(p.clone());
                        enc = Some(blk);
                        break;
                    }
                }
            }
        }

        if let Some(p) = &picked {
            println!("[dbg] Using img encrypted block from: {}", p.to_string_lossy());
        }
        if enc.is_none() {
            println!("[dbg] ImgKeyValidator: no .dat block found. total .dat={}, v631={}, v632={}", total_dat, v631_dat, v632_dat);
        } else {
            println!("[dbg] ImgKeyValidator initialized (v632={}, v631={}, thumb_used={})",
                v632_dat, v631_dat, picked.as_ref().map(|pp| pp.file_name().and_then(|n| n.to_str()).map(|n| n.ends_with("_t.dat")).unwrap_or(false)).unwrap_or(false)
            );
        }
        Ok(Self{ encrypted_block: enc })
    }

    pub fn validate_img_key(&self, key: &[u8]) -> bool {
        use aes::cipher::{BlockDecrypt, KeyInit};
        use aes::Aes128;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static MISS_COUNT: AtomicUsize = AtomicUsize::new(0);
        if self.encrypted_block.is_none() || key.len() < 16 { return false; }
        let mut blk = aes::cipher::generic_array::GenericArray::from(self.encrypted_block.unwrap());
        let k = aes::cipher::generic_array::GenericArray::from_slice(&key[..16]);
        let cipher = Aes128::new(k);
        cipher.decrypt_block(&mut blk);
        let dec = blk.as_slice();
        let ok = dec.starts_with(b"\xFF\xD8\xFF") || dec.starts_with(b"wxgf");
        if cfg!(debug_assertions) && !ok {
            let n = MISS_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            if n <= 10 || n % 500 == 0 { // only first 10 and then every 500th miss
                println!("[dbg] decrypt miss #{} (first 4) = {:02x?}", n, &dec[..4]);
            }
        }
        ok
    }
}
