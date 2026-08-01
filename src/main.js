const { invoke } = window.__TAURI__.core;

// State
let selectedHitsound = null;

// DOM Elements
const elements = {
    hitsoundList: document.getElementById('hitsound-list'),
    btnSelectFolder: document.getElementById('btn-select-folder'),
    folderPath: document.getElementById('folder-path'),
    btnImport: document.getElementById('btn-import'),
    minPitchSlider: document.getElementById('min-pitch-slider'),
    maxPitchSlider: document.getElementById('max-pitch-slider'),
    minPitchVal: document.getElementById('min-pitch-val'),
    maxPitchVal: document.getElementById('max-pitch-val'),
    damageInput: document.getElementById('damage-input'),
    btnTestPlay: document.getElementById('btn-test-play'),
    btnUse: document.getElementById('btn-use'),
    consoleCommands: document.getElementById('console-commands')
};

// Initialize
document.addEventListener('DOMContentLoaded', refreshHitsoundList);

// Event Listeners
elements.btnSelectFolder.addEventListener('click', async () => {
    try {
        const path = await invoke('select_tf2_folder');
        elements.folderPath.textContent = path;
    } catch (e) {
        console.error(e);
    }
});

elements.btnImport.addEventListener('click', async () => {
    try {
        await invoke('import_hitsound');
        refreshHitsoundList();
    } catch (e) {
        console.error(e);
    }
});

elements.minPitchSlider.addEventListener('input', updateConsoleCommands);
elements.maxPitchSlider.addEventListener('input', updateConsoleCommands);

elements.minPitchSlider.addEventListener('input', (e) => {
    elements.minPitchVal.textContent = e.target.value; // Updates the text
    updateConsoleCommands();
});

elements.maxPitchSlider.addEventListener('input', (e) => {
    elements.maxPitchVal.textContent = e.target.value; // Updates the text
    updateConsoleCommands();
});

elements.btnTestPlay.addEventListener('click', () => {
    if (selectedHitsound) {
        playAudio(selectedHitsound);
    }
});

elements.btnUse.addEventListener('click', async () => {
    if (!selectedHitsound) return;
    try {
        const msg = await invoke('use_hitsound', { hitsoundName: selectedHitsound });
        alert(msg);
    } catch (e) {
        alert("Error: " + e);
    }
});

// Functions
async function refreshHitsoundList() {
    try {
        const hitsounds = await invoke('list_hitsounds');
        elements.hitsoundList.innerHTML = '';
        
        hitsounds.forEach(name => {
            const div = document.createElement('div');
            div.className = 'hitsound-item';
            
            // The text span
            const nameSpan = document.createElement('span');
            nameSpan.className = 'hitsound-name';
            nameSpan.textContent = name;
            
            // The delete button
            const delBtn = document.createElement('button');
            delBtn.className = 'btn-delete';
            delBtn.textContent = '-';
            
            div.appendChild(nameSpan);
            div.appendChild(delBtn);
            
            // Selection logic
            div.addEventListener('click', () => {
                document.querySelectorAll('.hitsound-item').forEach(el => el.classList.remove('selected'));
                div.classList.add('selected');
                selectedHitsound = name;
                elements.btnUse.disabled = false;
            });
            
            // Double click to rename
            div.addEventListener('dblclick', async () => {
                const newName = prompt("Rename hitsound:", name.replace('.wav', ''));
                if (newName && newName !== name) {
                    await invoke('rename_hitsound', { oldName: name, newName });
                    refreshHitsoundList();
                }
            });

            // Delete logic
            delBtn.addEventListener('click', async (e) => {
                e.stopPropagation(); // Stops the div selection click from firing
                if (confirm(`Are you sure you want to delete ${name}?`)) {
                    try {
                        await invoke('delete_hitsound', { filename: name });
                        // Clear selection if they deleted the currently selected item
                        if (selectedHitsound === name) {
                            selectedHitsound = null;
                            elements.btnUse.disabled = true;
                        }
                        refreshHitsoundList();
                    } catch (err) {
                        alert("Failed to delete: " + err);
                    }
                }
            });

            elements.hitsoundList.appendChild(div);
        });
    } catch (e) {
        console.error("Failed to load hitsounds:", e);
    }
}

function calculateTF2Pitch(damage, minPitch, maxPitch) {
    // TF2 scales pitch based on damage values (10 to 150 DMG)
    const clampedDamage = Math.max(10, Math.min(150, damage));
    const progress = (clampedDamage - 10) / 140; 
    const pitch = minPitch + progress * (maxPitch - minPitch);
    return pitch / 100;
}

async function playAudio(filename) {
    try {
        // Read slider & input values
        const minPitch = Number(elements.minPitchSlider.value);
        const maxPitch = Number(elements.maxPitchSlider.value);
        const damage = Number(elements.damageInput.value);

        // Calculate pitch scale (e.g., 100 pitch = 1.0 rate, 50 pitch = 0.5 rate)
        const playbackRate = calculateTF2Pitch(damage, minPitch, maxPitch);
        
        // Tell Rust to play it natively!
        await invoke('play_test_hitsound', { 
            filename: filename, 
            playbackRate: playbackRate 
        });
        
    } catch (e) {
        console.error("Audio playback failed:", e);
        alert(`Failed to play audio: ${e}`);
    }
}

function updateConsoleCommands() {
    const minPitch = elements.minPitchSlider.value;
    const maxPitch = elements.maxPitchSlider.value;
    elements.consoleCommands.value = 
`tf_dingaling_pitchmindmg ${minPitch}
tf_dingaling_pitchmaxdmg ${maxPitch}`;
}