use std::path::PathBuf;
use anyhow::{Result, anyhow};

pub fn app_data_dir() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            return Ok(PathBuf::from(home).join(".we-sync"));
        }
        if let Some(home) = std::env::var_os("HOMEPATH") {
            return Ok(PathBuf::from(home).join(".we-sync"));
        }
        Err(anyhow!("cannot resolve USERPROFILE"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(home).join(".we-sync"));
        }
        Err(anyhow!("cannot resolve home dir"))
    }
}
