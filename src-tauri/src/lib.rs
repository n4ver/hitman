use std::fs::{copy, create_dir_all, read_dir, remove_file, rename, write, File};
use std::io::BufReader;
use std::path::{PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, State, Manager};
use tauri_plugin_dialog::DialogExt;
use rodio::{Decoder, OutputStream, Sink};

mod helpers;
mod manifest;
mod tf2;

pub use helpers::*;
pub use manifest::*;
pub use tf2::*;

struct AppState {
    tf2_dir: Mutex<Option<PathBuf>>,
    app_data_dir: PathBuf,
    pitch_manifest: Mutex<PitchManifest>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::self;
    use std::path::PathBuf;

    #[test]
    fn test_sanitize_cfg_name() {
        assert_eq!(sanitize_cfg_name("hello world"), "hello_world");
        assert_eq!(sanitize_cfg_name("Weird/Name#123"), "Weird_Name_123");
    }

    #[test]
    fn test_normalize_hitsound_filename() {
        assert_eq!(normalize_hitsound_filename("ding"), "ding.wav");
        assert_eq!(normalize_hitsound_filename("ding.wav"), "ding.wav");
    }

    #[test]
    fn test_build_cfg_alias_name() {
        let alias = build_cfg_alias_name("ding.wav", "0123456789abcdef");
        assert!(alias.starts_with("hitman_apply_ding_01234567"));
    }

    #[test]
    fn test_build_hitman_cfg_content_empty() {
        let content = build_hitman_cfg_content(&[]);
        assert!(content.contains("No remembered hitsounds"));
    }

    #[test]
    fn test_upsert_find_remove_pitch_record() {
        let mut manifest = PitchManifest::default();
        upsert_pitch_record(&mut manifest, "a.wav".into(), "hash1".into(), 100, 200);
        assert!(find_pitch_record(&manifest, "a.wav", "hash1").is_some());
        remove_pitch_record(&mut manifest, "a.wav", Some("hash1"));
        assert!(find_pitch_record(&manifest, "a.wav", "hash1").is_none());
    }

    #[test]
    fn test_compute_file_hash_and_ensure_exec_line() {
        let mut tmp = PathBuf::from(std::env::temp_dir());
        tmp.push("hitman_test_file.txt");
        let _ = fs::remove_file(&tmp);
        fs::write(&tmp, b"abc").unwrap();

        let hash = compute_file_hash(&tmp).unwrap();
        assert_eq!(hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");

        let mut exec_path = PathBuf::from(std::env::temp_dir());
        exec_path.push("hitman_autoexec_test.cfg");
        let _ = fs::remove_file(&exec_path);

        let added = ensure_exec_line(&exec_path, "exec hitman.cfg").unwrap();
        assert!(added);
        let added_again = ensure_exec_line(&exec_path, "exec hitman.cfg").unwrap();
        assert!(!added_again);

        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_file(&exec_path);
    }
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