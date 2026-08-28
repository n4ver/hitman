use sha2::{Digest, Sha256};
use std::path::Path;

pub fn sanitize_cfg_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

pub fn compute_file_hash(file_path: &Path) -> Result<String, String> {
    let file_bytes = std::fs::read(file_path).map_err(|e| e.to_string())?;
    let digest = Sha256::digest(file_bytes);
    Ok(digest.iter().map(|byte| format!("{:02x}", byte)).collect())
}

pub fn normalize_hitsound_filename(name: &str) -> String {
    if name.ends_with(".wav") {
        name.to_string()
    } else {
        format!("{}.wav", name)
    }
}

pub fn build_cfg_alias_name(filename: &str) -> String {
    let stem = filename.trim_end_matches(".wav");
    format!("hitman_apply_{}", sanitize_cfg_name(stem))
}
