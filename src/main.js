const { invoke } = window.__TAURI__.core;

// State
let selectedHitsound = null;
let selectionNonce = 0;
let selectedAlias = null;

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
    damageInputVal: document.getElementById('damage-input-val'),
    btnTestPlay: document.getElementById('btn-test-play'),
    btnApply: document.getElementById('btn-apply'),
    btnWriteCfg: document.getElementById('btn-write-cfg'),
    btnResetPitch: document.getElementById('btn-reset-pitch'),
    consoleCommands: document.getElementById('console-commands'),
    configMode: document.getElementById('config-mode')
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

elements.minPitchSlider.addEventListener('input', () => {
    elements.minPitchVal.textContent = elements.minPitchSlider.value;
    void updateConsoleCommands();
});

elements.maxPitchSlider.addEventListener('input', () => {
    elements.maxPitchVal.textContent = elements.maxPitchSlider.value;
    void updateConsoleCommands();
});
elements.configMode.addEventListener('change', () => void updateConsoleCommands());

elements.damageInput.addEventListener('input', (e) => {
    elements.damageInputVal.textContent = e.target.value;
})

elements.btnTestPlay.addEventListener('click', () => {
    if (selectedHitsound) {
        playAudio(selectedHitsound);
    }
});

elements.btnApply.addEventListener('click', async () => {
    if (!selectedHitsound) return;
    try {
        await persistCurrentPitch();
        const msg = await invoke('use_hitsound', { hitsoundName: selectedHitsound });
        alert(msg);
    } catch (e) {
        alert("Error: " + e);
    }
});

elements.btnWriteCfg.addEventListener('click', async () => {
    if (!selectedHitsound) return;
    try {
        await persistCurrentPitch();

        const cfgMsg = await invoke('write_hitman_cfg', {
            configMode: elements.configMode.value
        });

        let linkMsg = 'Skipped autoexec integration.';
        if (confirm('Add "exec hitman.cfg" to your TF2 autoexec file now?')) {
            linkMsg = await invoke('link_hitman_cfg_to_autoexec_with_mode', {
                configMode: elements.configMode.value
            });
        }

        alert(`${cfgMsg}\n${linkMsg}`);
    } catch (e) {
        alert("Error: " + e);
    }
});

elements.btnResetPitch.addEventListener('click', async () => {
    if (!selectedHitsound) return;

    try {
        const selectionToken = ++selectionNonce;
        await restorePitchForHitsound(selectedHitsound, selectionToken);
    } catch (e) {
        console.error("Failed to reset pitch:", e);
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

            const contentWrap = document.createElement('div');
            contentWrap.className = 'hitsound-content';
            
            // The text span
            const nameSpan = document.createElement('span');
            nameSpan.className = 'hitsound-name';
            nameSpan.textContent = name;

            const metaSpan = document.createElement('span');
            metaSpan.className = 'hitsound-meta';
            metaSpan.textContent = selectedHitsound === name ? buildHitsoundMetaText() : '';
            
            // The delete button
            const delBtn = document.createElement('button');
            delBtn.className = 'btn-delete';
            delBtn.textContent = '-';
            
            contentWrap.appendChild(nameSpan);
            contentWrap.appendChild(metaSpan);
            div.appendChild(contentWrap);
            div.appendChild(delBtn);
            
            // Selection logic
            div.addEventListener('click', async () => {
                document.querySelectorAll('.hitsound-item').forEach(el => el.classList.remove('selected'));
                if (selectedHitsound === name) {
                    div.classList.add('selected');
                    return;
                }

                const previousHitsound = selectedHitsound;
                const currentSelection = ++selectionNonce;

                if (previousHitsound) {
                    await persistPitchForHitsound(previousHitsound);
                    if (currentSelection !== selectionNonce) return;
                }

                selectedHitsound = name;
                selectedAlias = null;
                div.classList.add('selected');
                elements.btnApply.disabled = false;
                elements.btnWriteCfg.disabled = false;
                elements.btnResetPitch.disabled = false;
                const alias = await fetchHitsoundAlias(name);
                if (currentSelection !== selectionNonce) return;
                selectedAlias = alias;
                void updateConsoleCommands();
                syncSelectedItemMeta();
                void restorePitchForHitsound(name, currentSelection);
            });
            
            // Double click to rename
            div.addEventListener('dblclick', async () => {
                const newName = prompt("Rename hitsound:", name.replace('.wav', ''));
                if (newName && newName !== name) {
                    await invoke('rename_hitsound', { oldName: name, newName });
                    if (selectedHitsound === name) {
                        selectedHitsound = newName.endsWith('.wav') ? newName : `${newName}.wav`;
                    }
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
                            selectedAlias = null;
                            elements.btnApply.disabled = true;
                            elements.btnWriteCfg.disabled = true;
                            elements.btnResetPitch.disabled = true;
                        }
                        refreshHitsoundList();
                    } catch (err) {
                        alert("Failed to delete: " + err);
                    }
                }
            });

            elements.hitsoundList.appendChild(div);

            if (selectedHitsound === name) {
                div.classList.add('selected');
                elements.btnApply.disabled = false;
                elements.btnWriteCfg.disabled = false;
                elements.btnResetPitch.disabled = false;
                void refreshSelectedAlias(name);
            }
        });
    } catch (e) {
        console.error("Failed to load hitsounds:", e);
    }
}

function getCurrentPitchValues() {
    return {
        minPitch: Number(elements.minPitchSlider.value),
        maxPitch: Number(elements.maxPitchSlider.value),
    };
}

function setPitchValues(minPitch, maxPitch) {
    elements.minPitchSlider.value = minPitch;
    elements.maxPitchSlider.value = maxPitch;
    elements.minPitchVal.textContent = minPitch;
    elements.maxPitchVal.textContent = maxPitch;
    updateConsoleCommands();
    syncSelectedItemMeta();
}

async function restorePitchForHitsound(hitsoundName, selectionId) {
    try {
        const pitchSettings = await invoke('get_hitsound_pitch', { hitsoundName });
        if (selectionId !== selectionNonce) return;

        if (!pitchSettings) {
            setPitchValues(100, 100);
            return;
        }

        setPitchValues(pitchSettings.min_pitch, pitchSettings.max_pitch);
    } catch (e) {
        console.error("Failed to restore saved pitch:", e);
    }
}

async function fetchHitsoundAlias(hitsoundName) {
    try {
        return await invoke('get_hitsound_alias', { hitsoundName });
    } catch (e) {
        console.error("Failed to fetch hitsound alias:", e);
        return hitsoundName.replace(/\.wav$/i, '').replace(/[^a-z0-9_-]/gi, '_');
    }
}

async function refreshSelectedAlias(hitsoundName) {
    selectedAlias = await fetchHitsoundAlias(hitsoundName);
    void updateConsoleCommands();
    syncSelectedItemMeta();
}

async function persistCurrentPitch() {
    if (!selectedHitsound) return;

    try {
        const { minPitch, maxPitch } = getCurrentPitchValues();
        await invoke('save_hitsound_pitch', {
            hitsoundName: selectedHitsound,
            minPitch,
            maxPitch
        });
    } catch (e) {
        console.error("Failed to save pitch:", e);
    }
}

async function persistPitchForHitsound(hitsoundName) {
    const { minPitch, maxPitch } = getCurrentPitchValues();
    await invoke('save_hitsound_pitch', {
        hitsoundName,
        minPitch,
        maxPitch
    });
}

function buildHitsoundMetaText() {
    if (!selectedHitsound) return '';

    const minPitch = elements.minPitchSlider.value;
    const maxPitch = elements.maxPitchSlider.value;
    const aliasPart = selectedAlias ? `Alias: ${selectedAlias}` : 'Alias loading...';
    return `${aliasPart} • Pitch ${minPitch}/${maxPitch}`;
}

function syncSelectedItemMeta() {
    const selectedItem = document.querySelector('.hitsound-item.selected .hitsound-meta');
    if (selectedItem) {
        selectedItem.textContent = buildHitsoundMetaText();
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

async function updateConsoleCommands() {
    const minPitch = elements.minPitchSlider.value;
    const maxPitch = elements.maxPitchSlider.value;
    const configMode = elements.configMode.value;
    const modeLabel = configMode === 'auto' ? 'Auto-detect' : configMode === 'mastercomfig' ? 'Mastercomfig' : 'Vanilla TF2';

    if (!selectedHitsound) {
        // This should never run, but just in case
        elements.consoleCommands.value =
`Select a hitsound to preview the generated hitman.cfg.
Config mode: ${modeLabel}
tf_dingaling_pitchmindmg ${minPitch}
tf_dingaling_pitchmaxdmg ${maxPitch}`;
        return;
    }

    if (!selectedAlias) {
        selectedAlias = await fetchHitsoundAlias(selectedHitsound);
    }
    syncSelectedItemMeta();

    elements.consoleCommands.value = 
`alias "${selectedAlias}" "tf_dingaling_pitchmindmg ${minPitch}; tf_dingaling_pitchmaxdmg ${maxPitch};"
${selectedAlias};`;
}