use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{copy, create_dir_all, read_dir, read_to_string, remove_file, rename, write, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, State, Manager};
use tauri_plugin_dialog::DialogExt;
use rodio::{Decoder, OutputStream, Sink};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PitchRecord {
    filename: String,
    content_hash: String,
    min_pitch: u32,
    max_pitch: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PitchManifest {
    records: Vec<PitchRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PitchSettings {
    min_pitch: u32,
    max_pitch: u32,
}

struct AppState {
    tf2_dir: Mutex<Option<PathBuf>>,
    app_data_dir: PathBuf,
    pitch_manifest: Mutex<PitchManifest>,
}

fn sanitize_cfg_name(name: &str) -> String {
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

fn pitch_manifest_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("pitch_manifest.json")
}

fn load_pitch_manifest(app_data_dir: &Path) -> PitchManifest {
    let manifest_path = pitch_manifest_path(app_data_dir);
    let manifest_text = read_to_string(manifest_path).unwrap_or_default();

    serde_json::from_str(&manifest_text).unwrap_or_default()
}

fn save_pitch_manifest(app_data_dir: &Path, manifest: &PitchManifest) -> Result<(), String> {
    let manifest_path = pitch_manifest_path(app_data_dir);
    let manifest_text = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    write(manifest_path, manifest_text).map_err(|e| e.to_string())
}

fn compute_file_hash(file_path: &Path) -> Result<String, String> {
    let file_bytes = std::fs::read(file_path).map_err(|e| e.to_string())?;
    let digest = Sha256::digest(file_bytes);
    Ok(digest.iter().map(|byte| format!("{:02x}", byte)).collect())
}

fn normalize_hitsound_filename(name: &str) -> String {
    if name.ends_with(".wav") {
        name.to_string()
    } else {
        format!("{}.wav", name)
    }
}

fn build_cfg_alias_name(filename: &str, content_hash: &str) -> String {
    let stem = filename.trim_end_matches(".wav");
    let hash_prefix = &content_hash[..content_hash.len().min(8)];
    format!(
        "hitman_apply_{}_{}",
        sanitize_cfg_name(stem),
        sanitize_cfg_name(hash_prefix)
    )
}

fn find_pitch_record<'a>(manifest: &'a PitchManifest, filename: &str, content_hash: &str) -> Option<&'a PitchRecord> {
    manifest
        .records
        .iter()
        .find(|record| record.filename == filename || record.content_hash == content_hash)
}

fn upsert_pitch_record(
    manifest: &mut PitchManifest,
    filename: String,
    content_hash: String,
    min_pitch: u32,
    max_pitch: u32,
) {
    if let Some(record) = manifest
        .records
        .iter_mut()
        .find(|record| record.filename == filename || record.content_hash == content_hash)
    {
        record.filename = filename;
        record.content_hash = content_hash;
        record.min_pitch = min_pitch;
        record.max_pitch = max_pitch;
        return;
    }

    manifest.records.push(PitchRecord {
        filename,
        content_hash,
        min_pitch,
        max_pitch,
    });
}

fn remove_pitch_record(manifest: &mut PitchManifest, filename: &str, content_hash: Option<&str>) {
    manifest.records.retain(|record| {
        let filename_match = record.filename == filename;
        let hash_match = content_hash
            .map(|hash| record.content_hash == hash)
            .unwrap_or(false);
        !(filename_match || hash_match)
    });
}

fn update_pitch_manifest(
    app_data_dir: &Path,
    manifest: &mut PitchManifest,
    filename: &str,
    min_pitch: u32,
    max_pitch: u32,
) -> Result<(), String> {
    let file_path = app_data_dir.join(filename);
    let content_hash = compute_file_hash(&file_path)?;
    upsert_pitch_record(
        manifest,
        normalize_hitsound_filename(filename),
        content_hash,
        min_pitch,
        max_pitch,
    );
    save_pitch_manifest(app_data_dir, manifest)
}

fn get_pitch_settings_for_file(
    app_data_dir: &Path,
    manifest: &PitchManifest,
    filename: &str,
) -> Result<Option<PitchSettings>, String> {
    let file_path = app_data_dir.join(filename);
    let content_hash = compute_file_hash(&file_path)?;

    Ok(find_pitch_record(manifest, filename, &content_hash).map(|record| PitchSettings {
        min_pitch: record.min_pitch,
        max_pitch: record.max_pitch,
    }))
}

#[tauri::command]
fn get_hitsound_alias(state: State<'_, AppState>, hitsound_name: String) -> Result<String, String> {
    let file_path = state.app_data_dir.join(&hitsound_name);
    if !file_path.exists() {
        return Err("Hitsound file not found".into());
    }

    let content_hash = compute_file_hash(&file_path)?;
    Ok(build_cfg_alias_name(&hitsound_name, &content_hash))
}

fn resolve_tf2_root(tf2_custom_dir: &PathBuf) -> Result<PathBuf, String> {
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

fn build_hitman_cfg_content(records: &[PitchRecord]) -> String {
    let mut sorted_records = records.to_vec();
    sorted_records.sort_by(|left, right| {
        left.filename
            .cmp(&right.filename)
            .then(left.content_hash.cmp(&right.content_hash))
    });

    let mut content = String::from("// Generated by Hitman\n");

    if sorted_records.is_empty() {
        content.push_str("// No remembered hitsounds yet\n");
        return content;
    }

    for record in sorted_records {
        let alias_name = build_cfg_alias_name(&record.filename, &record.content_hash);
        content.push_str(&format!(
            concat!(
                "// Hitsound: {filename}\n",
                "alias \"{alias_name}\" \"tf_dingaling_pitchmindmg {min_pitch}; tf_dingaling_pitchmaxdmg {max_pitch}\"\n"
            ),
            filename = record.filename,
            alias_name = alias_name,
            min_pitch = record.min_pitch,
            max_pitch = record.max_pitch
        ));
    }

    content
}

fn resolve_cfg_dir(tf2_dir: &PathBuf) -> Result<PathBuf, String> {
    Ok(resolve_tf2_root(tf2_dir)?.join("cfg"))
}

fn resolve_autoexec_path_with_mode(
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

fn ensure_exec_line(path: &PathBuf, exec_line: &str) -> Result<bool, String> {
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

#[tauri::command]
async fn select_tf2_folder(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let folder_path = app.dialog().file().blocking_pick_folder();
    
    if let Some(path) = folder_path {
        if let Ok(raw_path) = path.into_path() {
            *state.tf2_dir.lock().unwrap() = Some(raw_path.clone());
            return Ok(raw_path.to_string_lossy().to_string());
        }
    }
    Err("No folder selected".into())
}

#[tauri::command]
async fn import_hitsound(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let file_path = app.dialog().file()
        .add_filter("Audio", &["wav"])
        .blocking_pick_file();
        
    if let Some(path) = file_path {
        if let Ok(raw_path) = path.into_path() {
            let file_name = raw_path.file_name().unwrap().to_string_lossy().to_string();
            let dest_path = state.app_data_dir.join(&file_name);
            
            copy(&raw_path, &dest_path).map_err(|e| e.to_string())?;
            return Ok(file_name);
        }
    }
    Err("No file selected".into())
}

#[tauri::command]
fn list_hitsounds(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut hitsounds = Vec::new();
    if let Ok(entries) = read_dir(&state.app_data_dir) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if name.ends_with(".wav") {
                    hitsounds.push(name);
                }
            }
        }
    }
    hitsounds.sort();
    Ok(hitsounds)
}

#[tauri::command]
fn rename_hitsound(state: State<'_, AppState>, old_name: String, new_name: String) -> Result<(), String> {
    let old_path = state.app_data_dir.join(&old_name);
    let new_name = normalize_hitsound_filename(&new_name);
    let new_path = state.app_data_dir.join(&new_name);
    let old_hash = compute_file_hash(&old_path).ok();

    rename(old_path, new_path).map_err(|e| e.to_string())?;

    let mut manifest = state.pitch_manifest.lock().unwrap();
    if let Some(record) = manifest
        .records
        .iter_mut()
        .find(|record| record.filename == old_name || old_hash.as_deref() == Some(record.content_hash.as_str()))
    {
        record.filename = new_name;
        if let Some(hash) = old_hash {
            record.content_hash = hash;
        }
        save_pitch_manifest(&state.app_data_dir, &manifest)?;
    }

    Ok(())
}

#[tauri::command]
fn delete_hitsound(state: State<'_, AppState>, filename: String) -> Result<(), String> {
    let file_path = state.app_data_dir.join(&filename);
    let file_hash = compute_file_hash(&file_path).ok();
    remove_file(file_path).map_err(|e| e.to_string())?;

    let mut manifest = state.pitch_manifest.lock().unwrap();
    remove_pitch_record(&mut manifest, &filename, file_hash.as_deref());
    save_pitch_manifest(&state.app_data_dir, &manifest)?;

    Ok(())
}

#[tauri::command]
fn get_hitsound_pitch(state: State<'_, AppState>, hitsound_name: String) -> Result<Option<PitchSettings>, String> {
    let manifest = state.pitch_manifest.lock().unwrap();
    get_pitch_settings_for_file(&state.app_data_dir, &manifest, &hitsound_name)
}

#[tauri::command]
fn save_hitsound_pitch(
    state: State<'_, AppState>,
    hitsound_name: String,
    min_pitch: u32,
    max_pitch: u32,
) -> Result<(), String> {
    let mut manifest = state.pitch_manifest.lock().unwrap();
    update_pitch_manifest(&state.app_data_dir, &mut manifest, &hitsound_name, min_pitch, max_pitch)
}

#[tauri::command]
fn use_hitsound(state: State<'_, AppState>, hitsound_name: String) -> Result<String, String> {
    let tf2_guard = state.tf2_dir.lock().unwrap();
    let tf2_dir = tf2_guard.as_ref().ok_or("TF2 Custom folder not selected")?;
    
    // Construct path: hitsound/sound/ui/hitsound.wav
    let target_dir = tf2_dir.join("hitsound").join("sound").join("ui");
    create_dir_all(&target_dir).map_err(|e| e.to_string())?;
    
    let source_path = state.app_data_dir.join(&hitsound_name);
    let target_path = target_dir.join("hitsound.wav");
    
    copy(&source_path, &target_path).map_err(|e| e.to_string())?;
    
    Ok("Hitsound successfully applied!".into())
}

#[tauri::command]
fn write_hitman_cfg(state: State<'_, AppState>, config_mode: Option<String>) -> Result<String, String> {
    let tf2_guard = state.tf2_dir.lock().unwrap();
    let tf2_dir = tf2_guard.as_ref().ok_or("TF2 Custom folder not selected")?;

    let cfg_dir = resolve_cfg_dir(tf2_dir)?;
    create_dir_all(&cfg_dir).map_err(|e| e.to_string())?;

    let cfg_path = cfg_dir.join("hitman.cfg");
    let manifest = state.pitch_manifest.lock().unwrap();
    let cfg_content = build_hitman_cfg_content(&manifest.records);
    write(cfg_path, cfg_content).map_err(|e| e.to_string())?;

    let _ = config_mode;
    Ok("hitman.cfg written successfully!".into())
}

#[tauri::command]
fn link_hitman_cfg_to_autoexec(state: State<'_, AppState>) -> Result<String, String> {
    link_hitman_cfg_to_autoexec_with_mode(state, None)
}

#[tauri::command]
fn link_hitman_cfg_to_autoexec_with_mode(
    state: State<'_, AppState>,
    config_mode: Option<String>,
) -> Result<String, String> {
    let tf2_guard = state.tf2_dir.lock().unwrap();
    let tf2_dir = tf2_guard.as_ref().ok_or("TF2 Custom folder not selected")?;

    let cfg_dir = resolve_cfg_dir(tf2_dir)?;
    create_dir_all(&cfg_dir).map_err(|e| e.to_string())?;

    let hitman_cfg_path = cfg_dir.join("hitman.cfg");
    if !hitman_cfg_path.exists() {
        return Err("hitman.cfg does not exist yet".into());
    }

    let autoexec_path = resolve_autoexec_path_with_mode(tf2_dir, config_mode.as_deref())?;
    let exec_line = "exec hitman.cfg";
    let added = ensure_exec_line(&autoexec_path, exec_line)?;

    if added {
        Ok(format!("Added '{}' to {}", exec_line, autoexec_path.display()))
    } else {
        Ok(format!("'{}' already exists in {}", exec_line, autoexec_path.display()))
    }
}

#[tauri::command]
fn play_test_hitsound(state: State<'_, AppState>, filename: String, playback_rate: f32) -> Result<(), String> {
    let file_path = state.app_data_dir.join(&filename);
    
    let file = File::open(file_path).map_err(|e| e.to_string())?;
    
    // Attempt to decode it BEFORE moving to the background thread.
    // If this fails (e.g., unsupported format), it immediately returns an error.
    let decoder = Decoder::new(BufReader::new(file))
        .map_err(|e| format!("Audio format error: {}", e))?;
    
    std::thread::spawn(move || {
        if let Ok((_stream, stream_handle)) = OutputStream::try_default() {
            if let Ok(sink) = Sink::try_new(&stream_handle) {
                sink.append(decoder);
                sink.set_speed(playback_rate);
                sink.sleep_until_end();
            }
        }
    });
    
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init()) 
        .setup(|app| {
            let mut app_data_dir = app.path().app_data_dir().expect("Failed to find OS AppData folder");
            app_data_dir.push("hitsounds_storage");
            create_dir_all(&app_data_dir).expect("Failed to create hitsounds storage");
            let pitch_manifest = load_pitch_manifest(&app_data_dir);
            app.manage(AppState {
                tf2_dir: std::sync::Mutex::new(None),
                app_data_dir,
                pitch_manifest: Mutex::new(pitch_manifest),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            select_tf2_folder,
            import_hitsound,
            list_hitsounds,
            rename_hitsound,
            get_hitsound_pitch,
            get_hitsound_alias,
            save_hitsound_pitch,
            use_hitsound,
            write_hitman_cfg,
            link_hitman_cfg_to_autoexec,
            link_hitman_cfg_to_autoexec_with_mode,
            delete_hitsound,
            play_test_hitsound
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}