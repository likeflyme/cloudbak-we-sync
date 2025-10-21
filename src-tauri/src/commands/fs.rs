#[tauri::command]
pub async fn open_in_os(path: String, reveal: Option<bool>) -> Result<(), String> {
    // Normalize Windows long path prefix handled by explorer
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let mut p = path.clone();
        if p.starts_with("\\\\?\\") {
            p = p.trim_start_matches("\\\\?\\").to_string();
        }
        let use_reveal = reveal.unwrap_or(false);
        let mut cmd = Command::new("explorer.exe");
        if use_reveal {
            // reveal file or folder
            cmd.arg("/select,").arg(p.clone());
        } else {
            cmd.arg(p.clone());
        }
        tracing::debug!(%p, reveal = use_reveal, "open_in_os windows");
        cmd.spawn().map_err(|e| {
            tracing::warn!(%p, error = %e, "explorer spawn failed");
            e.to_string()
        })?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        tracing::debug!(%path, "open_in_os mac" );
        Command::new("open").arg(&path).spawn().map_err(|e| {
            tracing::warn!(%path, error = %e, "open spawn failed");
            e.to_string()
        })?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        tracing::debug!(%path, "open_in_os linux");
        Command::new("xdg-open").arg(&path).spawn().map_err(|e| {
            tracing::warn!(%path, error = %e, "xdg-open spawn failed");
            e.to_string()
        })?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("unsupported platform".into())
}
