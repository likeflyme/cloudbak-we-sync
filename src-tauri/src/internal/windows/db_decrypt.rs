use std::{fs, path::Path};
use aes::Aes256;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::NoPadding};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac_array;
use sha2::Sha512;

const IV_SIZE: usize = 16;
const HMAC_SHA256_SIZE: usize = 64; // actually SHA512 digest length
const KEY_SIZE: usize = 32;
const AES_BLOCK_SIZE: usize = 16;
const ROUND_COUNT: u32 = 256000;
const PAGE_SIZE: usize = 4096;
const SALT_SIZE: usize = 16;
const SQLITE_HEADER: &[u8] = b"SQLite format 3";

type HmacSha512 = Hmac<Sha512>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;

pub fn decrypt_db_file_v4(input: &Path, pkey_hex: &str, output: &Path) -> anyhow::Result<bool> {
    let buf = fs::read(input)?;
    if buf.starts_with(SQLITE_HEADER) {
        fs::write(output, &buf)?;
        return Ok(true);
    }

    let mut out: Vec<u8> = Vec::with_capacity(buf.len());
    // salt
    if buf.len() < SALT_SIZE { anyhow::bail!("input too small"); }
    let salt = &buf[..SALT_SIZE];
    let mut mac_salt = [0u8; SALT_SIZE];
    for (i, b) in salt.iter().enumerate() { mac_salt[i] = b ^ 0x3a; }

    // pass key from hex
    let pass_key = hex::decode(pkey_hex)?;

    // derive key via PBKDF2-HMAC-SHA512
    let key_arr: [u8; KEY_SIZE] = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(&pass_key, salt, ROUND_COUNT);
    let key = key_arr;
    // derive mac_key via PBKDF2 with iter=2
    let mac_key_arr: [u8; KEY_SIZE] = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(&key, &mac_salt, 2);

    // header + 0x00
    out.extend_from_slice(SQLITE_HEADER);
    out.push(0x00);

    let reserve_raw = IV_SIZE + HMAC_SHA256_SIZE;
    let reserve = ((reserve_raw + AES_BLOCK_SIZE - 1) / AES_BLOCK_SIZE) * AES_BLOCK_SIZE;

    let total_page = buf.len() / PAGE_SIZE;
    for cur_page in 0..total_page {
        let offset = if cur_page == 0 { SALT_SIZE } else { 0 };
        let start = cur_page * PAGE_SIZE;
        let end = start + PAGE_SIZE;
        if end > buf.len() { break; }

        // compute hmac over data + page number (LE u32)
        let data_for_mac = &buf[start + offset..end - reserve + IV_SIZE];
        let mut mac = HmacSha512::new_from_slice(&mac_key_arr).unwrap();
        mac.update(data_for_mac);
        mac.update(&(cur_page as u32 + 1).to_le_bytes());
        let expected_mac = mac.finalize().into_bytes();

        let mac_region_start = end - reserve + IV_SIZE;
        let mac_region_end = mac_region_start + expected_mac.len();
        let actual_mac = &buf[mac_region_start..mac_region_end];
        // verify when not all zeros
        if actual_mac.iter().any(|&b| b != 0) {
            if expected_mac.as_slice() != actual_mac {
                anyhow::bail!("HMAC verification failed at page {}", cur_page + 1);
            }
        }

        // decrypt AES-256-CBC
        let iv = &buf[end - reserve..end - reserve + IV_SIZE];
        let mut dec = Aes256CbcDec::new(&key.into(), iv.into());
        let mut cipher_text = buf[start + offset..end - reserve].to_vec();
        // CBC decrypt requires multiple of block size
        if cipher_text.len() % AES_BLOCK_SIZE != 0 { anyhow::bail!("cipher text not block aligned at page {}", cur_page + 1); }
        // Use NoPadding since page data is block-aligned
        let decrypted = dec
            .decrypt_padded_mut::<NoPadding>(&mut cipher_text)
            .map_err(|_| anyhow::anyhow!("AES decrypt failed at page {}", cur_page + 1))?;

        out.extend_from_slice(decrypted);
        out.extend_from_slice(&buf[end - reserve..end]);
    }

    fs::write(output, &out)?;
    Ok(true)
}
