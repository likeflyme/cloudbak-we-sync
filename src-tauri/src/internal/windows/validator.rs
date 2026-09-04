use anyhow::Result;
use hmac::{Hmac, Mac};
use sha2::Sha512;
use std::collections::HashMap;
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

pub(crate) struct V4DbTarget {
    db_name: String,
    required_message: bool,
    validator: DBValidator,
}

impl V4DbTarget {
    pub(crate) fn db_name(&self) -> &str {
        &self.db_name
    }

    pub(crate) fn salt(&self) -> [u8; SALT_SIZE] {
        self.validator.salt
    }

    pub(crate) fn is_required_message(&self) -> bool {
        self.required_message
    }

    pub(crate) fn validate_derived_key(&self, key: &[u8]) -> bool {
        self.validator.validate_raw_db_key(key)
    }

    #[cfg(test)]
    pub(crate) fn from_first_page(db_name: &str, first_page: Vec<u8>) -> Result<Self> {
        Ok(Self {
            db_name: db_name.to_string(),
            required_message: true,
            validator: DBValidator::from_first_page(first_page)?,
        })
    }
}

impl DBValidator {
    fn collect_message_db_paths(data_dir: &str) -> Vec<PathBuf> {
        let message_dir = Path::new(data_dir).join("db_storage").join("message");
        let mut candidates: Vec<PathBuf> = Vec::new();
        let preferred = message_dir.join("message_0.db");
        if preferred.exists() {
            candidates.push(preferred);
        }
        if let Ok(rd) = fs::read_dir(&message_dir) {
            for ent in rd.flatten() {
                let p = ent.path();
                let Some(name) = p.file_name().and_then(|s| s.to_str()) else { continue; };
                let is_message_db = name == "message_fts.db"
                    || (name.starts_with("message_")
                        && name.ends_with(".db")
                        && name["message_".len()..name.len() - ".db".len()].chars().all(|c| c.is_ascii_digit()));
                if !is_message_db {
                    continue;
                }
                if !candidates.iter().any(|existing| existing == &p) {
                    candidates.push(p);
                }
            }
        }
        candidates.sort_by(|a, b| {
            let a_name = a.file_name().and_then(|s| s.to_str()).unwrap_or_default();
            let b_name = b.file_name().and_then(|s| s.to_str()).unwrap_or_default();
            a_name.cmp(b_name)
        });
        candidates
    }

    fn load_encrypted_first_page(db_path: &Path) -> Result<Option<Vec<u8>>> {
        let mut file = std::fs::File::open(db_path)?;
        if file.metadata()?.len() < PAGE_SIZE as u64 {
            return Ok(None);
        }
        let buf = {
            use std::io::Read;
            let mut page = vec![0u8; PAGE_SIZE];
            file.read_exact(&mut page)?;
            page
        };
        if &buf[..15] == b"SQLite format 3" {
            return Ok(None);
        }
        Ok(Some(buf))
    }

    fn is_required_message_db(db_storage_dir: &Path, db_path: &Path) -> bool {
        let Some(parent) = db_path.parent() else { return false; };
        if parent.parent() != Some(db_storage_dir) {
            return false;
        }
        let is_message_dir = parent
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("message"))
            .unwrap_or(false);
        if !is_message_dir {
            return false;
        }
        let Some(name) = db_path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        let name = name.to_ascii_lowercase();
        name == "message_fts.db"
            || (name.starts_with("message_")
                && name.ends_with(".db")
                && name["message_".len()..name.len() - ".db".len()]
                    .chars()
                    .all(|c| c.is_ascii_digit()))
    }

    fn collect_v4_db_paths(data_dir: &str) -> Vec<(PathBuf, String, bool)> {
        let db_storage_dir = Path::new(data_dir).join("db_storage");
        let mut candidates = Vec::new();
        for entry in walkdir::WalkDir::new(&db_storage_dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.into_path();
            let is_db = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("db"))
                .unwrap_or(false);
            if !is_db {
                continue;
            }
            let db_name = path
                .strip_prefix(&db_storage_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let required_message = Self::is_required_message_db(&db_storage_dir, &path);
            candidates.push((path, db_name, required_message));
        }
        candidates.sort_by(|left, right| left.1.cmp(&right.1));
        candidates
    }

    fn from_first_page(first_page: Vec<u8>) -> Result<Self> {
        if first_page.len() < PAGE_SIZE {
            anyhow::bail!("encrypted database page is smaller than {PAGE_SIZE} bytes");
        }
        let mut salt = [0u8; SALT_SIZE];
        salt.copy_from_slice(&first_page[..SALT_SIZE]);
        Ok(Self { first_page, salt })
    }

    pub(crate) fn load_v4_targets(data_dir: &str) -> Result<Vec<V4DbTarget>> {
        let mut targets = Vec::new();
        let mut skipped_unencrypted = 0usize;
        let mut unreadable_optional = 0usize;
        for (db_path, db_name, required_message) in Self::collect_v4_db_paths(data_dir) {
            match Self::load_encrypted_first_page(&db_path) {
                Ok(Some(first_page)) => targets.push(V4DbTarget {
                    db_name,
                    required_message,
                    validator: Self::from_first_page(first_page)?,
                }),
                Ok(None) => skipped_unencrypted += 1,
                Err(error) if required_message => {
                    return Err(anyhow::anyhow!("无法读取加密消息数据库候选: {error}"));
                }
                Err(_) => unreadable_optional += 1,
            }
        }
        tracing::info!(
            target_count = targets.len(),
            required_message_count = targets
                .iter()
                .filter(|target| target.is_required_message())
                .count(),
            skipped_unencrypted_count = skipped_unencrypted,
            unreadable_optional_count = unreadable_optional,
            "loaded Windows v4 database validation targets"
        );
        Ok(targets)
    }

    pub fn new(data_dir: &str) -> Result<Self> {
        let candidates = Self::collect_message_db_paths(data_dir);
        let mut first_page: Option<Vec<u8>> = None;
        for cand in candidates {
            if !cand.exists() { continue; }
            match Self::load_encrypted_first_page(&cand) {
                Ok(Some(buf)) => {
                    println!("[dbg] Using DB file: {}", cand.to_string_lossy());
                    first_page = Some(buf);
                    break;
                }
                Ok(None) => continue,
                Err(_) => continue,
            }
        }
        let first_page = first_page.ok_or_else(|| anyhow::anyhow!("No suitable encrypted message_*.db found"))?;
        Self::from_first_page(first_page)
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

    fn derive_raw_key_mac_key(raw_key: &[u8; KEY_SIZE], salt: &[u8; SALT_SIZE]) -> [u8; KEY_SIZE] {
        let mut mac_salt = [0u8; SALT_SIZE];
        for i in 0..SALT_SIZE { mac_salt[i] = salt[i] ^ 0x3a; }

        let mut mac_key = [0u8; KEY_SIZE];
        Self::pbkdf2_hmac_sha512(raw_key, &mac_salt, 2u64, &mut mac_key)
            .expect("PBKDF2 (raw mac_key) failed");
        mac_key
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

    pub fn validate_raw_db_key(&self, key: &[u8]) -> bool {
        if key.len() != KEY_SIZE { return false; }
        let mut raw_key = [0u8; KEY_SIZE];
        raw_key.copy_from_slice(key);
        let mac_key = Self::derive_raw_key_mac_key(&raw_key, &self.salt);
        let data_end = PAGE_SIZE - (IV_SIZE + HMAC_SHA512_SIZE) + IV_SIZE;
        let mut mac = Hmac::<Sha512>::new_from_slice(&mac_key).unwrap();
        mac.update(&self.first_page[SALT_SIZE..data_end]);
        mac.update(&(1u32.to_le_bytes()));
        let calc = mac.finalize().into_bytes();
        let stored = &self.first_page[data_end..data_end+HMAC_SHA512_SIZE];
        subtle::ConstantTimeEq::ct_eq(&calc[..], stored).unwrap_u8() == 1
    }

    fn parse_v4_salt_key_map(db_keys_hex: &[String]) -> HashMap<String, [u8; KEY_SIZE]> {
        let mut salt_key_map = HashMap::new();
        for key in db_keys_hex {
            let key = key.trim();
            if key.len() != 96 || !key.as_bytes().iter().all(|b| b.is_ascii_hexdigit()) {
                continue;
            }
            let Some(sub_key_hex) = key.get(..64) else { continue; };
            let Some(salt_hex) = key.get(64..96) else { continue; };
            let Ok(decoded) = hex::decode(sub_key_hex) else { continue; };
            if decoded.len() != KEY_SIZE {
                continue;
            }
            let mut arr = [0u8; KEY_SIZE];
            arr.copy_from_slice(&decoded);
            salt_key_map.insert(salt_hex.to_ascii_lowercase(), arr);
        }
        salt_key_map
    }

    pub fn find_first_invalid_v4_db(data_dir: &str, db_keys_hex: &[String]) -> Result<Option<String>> {
        let targets = Self::load_v4_targets(data_dir)?;
        let required_target_count = targets
            .iter()
            .filter(|target| target.is_required_message())
            .count();
        if required_target_count == 0 {
            tracing::warn!("no encrypted message db candidates found while validating derived keys");
            return Ok(None);
        }

        let salt_key_map = Self::parse_v4_salt_key_map(db_keys_hex);

        tracing::info!(
            candidate_count = required_target_count,
            parsed_key_count = salt_key_map.len(),
            "start validating derived keys for message db files"
        );

        if salt_key_map.is_empty() {
            tracing::warn!(
                raw_key_count = db_keys_hex.len(),
                "no salt->derived-key pairs parsed from extractor output"
            );
            return Ok(targets
                .iter()
                .find(|target| target.is_required_message())
                .map(|target| target.db_name().to_string()));
        }

        let mut succeeded_count = 0usize;
        let mut failed_count = 0usize;
        let mut first_failed = None;

        for target in targets.into_iter().filter(|target| target.is_required_message()) {
            let db_name = target.db_name();
            let salt_hex = hex::encode(target.salt());
            let matched = salt_key_map
                .get(&salt_hex)
                .map(|key| target.validate_derived_key(key))
                .unwrap_or(false);
            if !matched {
                failed_count += 1;
                if first_failed.is_none() {
                    first_failed = Some(db_name.to_string());
                }
                continue;
            }
            succeeded_count += 1;
        }

        if failed_count > 0 {
            tracing::warn!(
                succeeded_count,
                failed_count,
                "derived key validation completed with failures"
            );
            return Ok(first_failed);
        }
        tracing::info!(succeeded_count, "derived key validation succeeded for all message db files");
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pbkdf2::pbkdf2_hmac_array;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let sequence = TEST_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "we-sync-v4-validator-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_file(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn valid_page(key: [u8; KEY_SIZE], salt: [u8; SALT_SIZE]) -> Vec<u8> {
        let mut page = vec![0x5au8; PAGE_SIZE];
        page[..SALT_SIZE].copy_from_slice(&salt);
        let mac_salt: [u8; SALT_SIZE] = salt.map(|byte| byte ^ 0x3a);
        let mac_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(&key, &mac_salt, 2);
        let data_end = PAGE_SIZE - (IV_SIZE + HMAC_SHA512_SIZE) + IV_SIZE;
        let mut mac = Hmac::<Sha512>::new_from_slice(&mac_key).unwrap();
        mac.update(&page[SALT_SIZE..data_end]);
        mac.update(&1u32.to_le_bytes());
        page[data_end..data_end + HMAC_SHA512_SIZE]
            .copy_from_slice(&mac.finalize().into_bytes());
        page
    }

    #[test]
    fn loads_encrypted_databases_recursively_and_classifies_message_targets() {
        let test_dir = TestDir::new();
        let encrypted_page = vec![0x5au8; PAGE_SIZE];
        let mut plaintext_page = vec![0u8; PAGE_SIZE];
        plaintext_page[..15].copy_from_slice(b"SQLite format 3");

        write_file(
            test_dir.path(),
            "db_storage/message/message_0.db",
            &encrypted_page,
        );
        write_file(
            test_dir.path(),
            "db_storage/contact/contact.db",
            &encrypted_page,
        );
        write_file(
            test_dir.path(),
            "db_storage/search/index.DB",
            &encrypted_page,
        );
        write_file(
            test_dir.path(),
            "db_storage/session/session.db",
            &plaintext_page,
        );
        write_file(test_dir.path(), "db_storage/other/tiny.db", &[0u8; 128]);
        write_file(
            test_dir.path(),
            "db_storage/contact/contact.db-wal",
            &encrypted_page,
        );

        let targets = DBValidator::load_v4_targets(test_dir.path().to_str().unwrap()).unwrap();
        let names: Vec<&str> = targets.iter().map(V4DbTarget::db_name).collect();

        assert_eq!(targets.len(), 3);
        assert!(names.contains(&"message/message_0.db"));
        assert!(names.contains(&"contact/contact.db"));
        assert!(names.contains(&"search/index.DB"));
        assert_eq!(
            targets
                .iter()
                .filter(|target| target.is_required_message())
                .count(),
            1
        );
    }

    #[test]
    fn missing_optional_key_does_not_fail_required_message_validation() {
        let test_dir = TestDir::new();
        let message_key = std::array::from_fn(|index| (index + 1) as u8);
        let message_salt = std::array::from_fn(|index| (index + 41) as u8);
        let optional_key = std::array::from_fn(|index| (index + 81) as u8);
        let optional_salt = std::array::from_fn(|index| (index + 121) as u8);
        write_file(
            test_dir.path(),
            "db_storage/message/message_0.db",
            &valid_page(message_key, message_salt),
        );
        write_file(
            test_dir.path(),
            "db_storage/contact/contact.db",
            &valid_page(optional_key, optional_salt),
        );
        let db_keys = vec![format!(
            "{}{}",
            hex::encode(message_key),
            hex::encode(message_salt)
        )];

        let invalid = DBValidator::find_first_invalid_v4_db(
            test_dir.path().to_str().unwrap(),
            &db_keys,
        )
        .unwrap();

        assert_eq!(invalid, None);
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
