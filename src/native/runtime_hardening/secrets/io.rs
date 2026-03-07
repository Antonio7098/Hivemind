use super::*;

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        RuntimeHardeningError::new(
            "native_atomic_write_failed",
            format!("Path '{}' does not have a parent directory", path.display()),
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        RuntimeHardeningError::new(
            "native_atomic_write_failed",
            format!(
                "Failed to create parent dir '{}': {error}",
                parent.display()
            ),
        )
    })?;
    let tmp_path = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("native"),
        Uuid::new_v4()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut tmp_file = options.open(&tmp_path).map_err(|error| {
        RuntimeHardeningError::new(
            "native_atomic_write_failed",
            format!(
                "Failed to create temp file '{}': {error}",
                tmp_path.display()
            ),
        )
    })?;
    tmp_file.write_all(bytes).map_err(|error| {
        RuntimeHardeningError::new(
            "native_atomic_write_failed",
            format!(
                "Failed to write temp file '{}': {error}",
                tmp_path.display()
            ),
        )
    })?;
    drop(tmp_file);
    fs::rename(&tmp_path, path).map_err(|error| {
        let _ = fs::remove_file(&tmp_path);
        RuntimeHardeningError::new(
            "native_atomic_write_failed",
            format!(
                "Failed to atomically replace '{}' with '{}': {error}",
                path.display(),
                tmp_path.display()
            ),
        )
    })?;
    Ok(())
}
