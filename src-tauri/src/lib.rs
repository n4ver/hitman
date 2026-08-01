use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, State, Manager};
use tauri_plugin_dialog::DialogExt;
use std::fs::File;
use std::io::BufReader;
use rodio::{Decoder, OutputStream, Sink};

// Application state to store the TF2 directory and app data dir
struct AppState {
    tf2_dir: Mutex<Option<PathBuf>>,
    app_data_dir: PathBuf,
}

#[tauri::command]
async fn select_tf2_folder(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    // Open folder picker
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
    // Open file picker
    let file_path = app.dialog().file()
        .add_filter("Audio", &["wav"])
        .blocking_pick_file();
        
    if let Some(path) = file_path {
        if let Ok(raw_path) = path.into_path() {
            let file_name = raw_path.file_name().unwrap().to_string_lossy().to_string();
            let dest_path = state.app_data_dir.join(&file_name);
            
            fs::copy(&raw_path, &dest_path).map_err(|e| e.to_string())?;
            return Ok(file_name);
        }
    }
    Err("No file selected".into())
}

#[tauri::command]
fn list_hitsounds(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut hitsounds = Vec::new();
    if let Ok(entries) = fs::read_dir(&state.app_data_dir) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if name.ends_with(".wav") {
                    hitsounds.push(name);
                }
            }
        }
    }
    Ok(hitsounds)
}

#[tauri::command]
fn rename_hitsound(state: State<'_, AppState>, old_name: String, new_name: String) -> Result<(), String> {
    let old_path = state.app_data_dir.join(&old_name);
    let new_name = if new_name.ends_with(".wav") { new_name } else { format!("{}.wav", new_name) };
    let new_path = state.app_data_dir.join(&new_name);
    
    fs::rename(old_path, new_path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn delete_hitsound(state: State<'_, AppState>, filename: String) -> Result<(), String> {
    let file_path = state.app_data_dir.join(&filename);
    fs::remove_file(file_path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn use_hitsound(state: State<'_, AppState>, hitsound_name: String) -> Result<String, String> {
    let tf2_guard = state.tf2_dir.lock().unwrap();
    let tf2_dir = tf2_guard.as_ref().ok_or("TF2 Custom folder not selected")?;
    
    // Construct path: hitsound/sound/ui/hitsound.wav
    let target_dir = tf2_dir.join("hitsound").join("sound").join("ui");
    fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
    
    let source_path = state.app_data_dir.join(&hitsound_name);
    let target_path = target_dir.join("hitsound.wav");
    
    fs::copy(&source_path, &target_path).map_err(|e| e.to_string())?;
    
    Ok("Hitsound successfully applied!".into())
}

#[tauri::command]
fn play_test_hitsound(state: State<'_, AppState>, filename: String, playback_rate: f32) -> Result<(), String> {
    let file_path = state.app_data_dir.join(&filename);
    
    // 1. Open the file
    let file = File::open(file_path).map_err(|e| e.to_string())?;
    
    // 2. Attempt to decode it BEFORE moving to the background thread.
    // If this fails (e.g., unsupported format), it immediately returns an error to your JS.
    let decoder = Decoder::new(BufReader::new(file))
        .map_err(|e| format!("Audio format error: {}", e))?;
    
    // 3. Play the audio on a separate thread
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
            // 1. Get the official OS AppData folder (e.g., %APPDATA% on Windows)
            let mut app_data_dir = app.path().app_data_dir().expect("Failed to find OS AppData folder");
            
            // 2. Add our custom folder name
            app_data_dir.push("hitsounds_storage");
            
            // 3. Create it if it doesn't exist
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create hitsounds storage");

            // 4. Register the state
            app.manage(AppState {
                tf2_dir: std::sync::Mutex::new(None),
                app_data_dir,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            select_tf2_folder,
            import_hitsound,
            list_hitsounds,
            rename_hitsound,
            use_hitsound,
            delete_hitsound,
            play_test_hitsound
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}