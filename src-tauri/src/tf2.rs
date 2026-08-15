use std::fs::{create_dir_all, read_to_string, write};
use std::path::PathBuf;

pub fn resolve_tf2_root(tf2_custom_dir: &PathBuf) -> Result<PathBuf, String> {
    let folder_name = tf2_custom_dir
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("Invalid TF2 custom folder")?;

    if folder_name.eq_ignore_ascii_case("custom") {
        tf2_custom_dir
            .parent()
            .map(|path| path.to_path_buf())
            .ok_or("Could not resolve TF2 root folder".to_string())
    } else {
        Ok(tf2_custom_dir.clone())
    }
}

pub fn resolve_cfg_dir(tf2_dir: &PathBuf) -> Result<PathBuf, String> {
    Ok(resolve_tf2_root(tf2_dir)?.join("cfg"))
}

pub fn resolve_autoexec_path_with_mode(
    tf2_dir: &PathBuf,
    config_mode: Option<&str>,
) -> Result<PathBuf, String> {
    let cfg_dir = resolve_cfg_dir(tf2_dir)?;
    match config_mode.map(|value| value.to_ascii_lowercase()) {
        Some(mode) if mode == "vanilla" => Ok(cfg_dir.join("autoexec.cfg")),
        Some(mode) if mode == "mastercomfig" => Ok(cfg_dir.join("overrides").join("autoexec.cfg")),
        _ => {
            let mastercomfig_autoexec = cfg_dir.join("overrides").join("autoexec.cfg");
            if mastercomfig_autoexec.exists() {
                return Ok(mastercomfig_autoexec);
            }

            Ok(cfg_dir.join("autoexec.cfg"))
        }
    }
}

pub fn ensure_exec_line(path: &PathBuf, exec_line: &str) -> Result<bool, String> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let existing = read_to_string(path).unwrap_or_default();
    if existing.lines().any(|line| line.trim() == exec_line) {
        return Ok(false);
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(exec_line);
    updated.push('\n');

    write(path, updated).map_err(|e| e.to_string())?;
    Ok(true)
}
