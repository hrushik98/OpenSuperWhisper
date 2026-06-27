import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// State
let isRecording = false;
let recordTimerInterval: number | null = null;
let recordingDurationSeconds = 0;
let currentSettings = {
  selected_engine: "whisper",
  selected_model_path: "",
  selected_microphone: "",
  selected_language: "auto",
  cjk_spacing: true,
  play_sounds: true,
  auto_paste: true,
  launch_at_login: false,
  global_shortcut: "F9",
  hold_to_record: false
};
let downloadingModelName: string | null = null;

// DOM Elements
const navItems = document.querySelectorAll(".nav-item");
const tabContents = document.querySelectorAll(".tab-content");
const recordBtn = document.getElementById("record-btn") as HTMLButtonElement;
const recordingTimer = document.getElementById("recording-timer") as HTMLDivElement;
const recorderStatus = document.getElementById("recorder-status") as HTMLDivElement;
const resultText = document.getElementById("result-text") as HTMLTextAreaElement;
const wordCount = document.getElementById("word-count") as HTMLSpanElement;
const charCount = document.getElementById("char-count") as HTMLSpanElement;
const micSelect = document.getElementById("mic-select") as HTMLSelectElement;
const modelSelect = document.getElementById("model-select") as HTMLSelectElement;
const languageSelect = document.getElementById("language-select") as HTMLSelectElement;
const cjkToggle = document.getElementById("cjk-toggle") as HTMLInputElement;
const autopasteToggle = document.getElementById("autopaste-toggle") as HTMLInputElement;
const soundsToggle = document.getElementById("sounds-toggle") as HTMLInputElement;
const shortcutInput = document.getElementById("shortcut-input") as HTMLInputElement;
const activeModelName = document.getElementById("active-model-name") as HTMLSpanElement;
const historyList = document.getElementById("history-list") as HTMLDivElement;
const historySearch = document.getElementById("history-search") as HTMLInputElement;
const clearHistoryBtn = document.getElementById("clear-history-btn") as HTMLButtonElement;
const copyLatestBtn = document.getElementById("copy-latest-btn") as HTMLButtonElement;
const cancelRecordBtn = document.getElementById("cancel-record-btn") as HTMLButtonElement;
const recorderActions = document.getElementById("recorder-actions") as HTMLDivElement;

// Download HUD elements
const downloadHud = document.getElementById("download-progress-hud") as HTMLDivElement;
const hudModelName = document.getElementById("hud-model-name") as HTMLSpanElement;
const hudPercentage = document.getElementById("hud-percentage") as HTMLSpanElement;
const hudProgressBar = document.getElementById("hud-progress-bar") as HTMLDivElement;
const cancelHudDownload = document.getElementById("cancel-hud-download") as HTMLButtonElement;

// Initialize
window.addEventListener("DOMContentLoaded", async () => {
  setupTabs();
  await loadSettings();
  await loadHardware();
  await loadModels();
  await loadHistory();
  setupEventListeners();
  setupDragAndDrop();
  setupDownloadListener();
});

// Navigation / Tabs
function setupTabs() {
  navItems.forEach((item) => {
    item.addEventListener("click", () => {
      const tabId = item.getAttribute("data-tab");
      if (!tabId) return;

      navItems.forEach((nav) => nav.classList.remove("active"));
      tabContents.forEach((tab) => tab.classList.remove("active"));

      item.classList.add("active");
      const targetTab = document.getElementById(`tab-${tabId}`);
      if (targetTab) {
        targetTab.classList.add("active");
      }
      
      if (tabId === "history") {
        loadHistory();
      }
    });
  });
}

// Load Settings
async function loadSettings() {
  try {
    currentSettings = await invoke("get_settings");
    
    languageSelect.value = currentSettings.selected_language;
    cjkToggle.checked = currentSettings.cjk_spacing;
    autopasteToggle.checked = currentSettings.auto_paste;
    soundsToggle.checked = currentSettings.play_sounds;
    shortcutInput.value = currentSettings.global_shortcut;
    
    updateActiveModelBadge();
  } catch (err) {
    console.error("Failed to load settings:", err);
  }
}

// Update Active Model Badge in Sidebar
function updateActiveModelBadge() {
  if (currentSettings.selected_model_path) {
    // Get last component of path
    const parts = currentSettings.selected_model_path.split(/[/\\]/);
    activeModelName.textContent = parts[parts.length - 1];
  } else {
    activeModelName.textContent = "No Model Selected";
  }
}

// Save Settings Helper
async function saveSettings() {
  try {
    currentSettings.selected_language = languageSelect.value;
    currentSettings.cjk_spacing = cjkToggle.checked;
    currentSettings.auto_paste = autopasteToggle.checked;
    currentSettings.play_sounds = soundsToggle.checked;
    currentSettings.selected_microphone = micSelect.value || "";
    currentSettings.selected_model_path = modelSelect.value || "";
    
    await invoke("save_settings", { settings: currentSettings });
    updateActiveModelBadge();
  } catch (err) {
    console.error("Failed to save settings:", err);
  }
}

// Load Hardware Inputs
async function loadHardware() {
  try {
    const microphones: string[] = await invoke("get_microphones");
    
    // Clear list but keep default option
    micSelect.innerHTML = '<option value="">Default Microphone</option>';
    
    microphones.forEach((mic) => {
      const option = document.createElement("option");
      option.value = mic;
      option.textContent = mic;
      if (currentSettings.selected_microphone === mic) {
        option.selected = true;
      }
      micSelect.appendChild(option);
    });
  } catch (err) {
    console.error("Failed to load microphones:", err);
  }
}

// Load Available Models
async function loadModels() {
  try {
    const models: string[] = await invoke("get_models");
    
    modelSelect.innerHTML = "";
    if (models.length === 0) {
      modelSelect.innerHTML = '<option value="">No models downloaded</option>';
      return;
    }
    
    models.forEach((model) => {
      const option = document.createElement("option");
      option.value = model; // Keep as filename, Rust will resolve it
      option.textContent = model;
      // We check if settings matches the filename
      if (currentSettings.selected_model_path.endsWith(model) || currentSettings.selected_model_path === model) {
        option.selected = true;
      }
      modelSelect.appendChild(option);
    });
    
    // Update active model from selection if not set
    if (!currentSettings.selected_model_path && models.length > 0) {
      modelSelect.selectedIndex = 0;
      saveSettings();
    }
  } catch (err) {
    console.error("Failed to load models:", err);
  }
}

// Load History
async function loadHistory() {
  try {
    const history: any[] = await invoke("get_history");
    renderHistory(history);
  } catch (err) {
    console.error("Failed to load history:", err);
  }
}

// Render History
function renderHistory(items: any[]) {
  const query = historySearch.value.toLowerCase().trim();
  
  const filtered = items.filter((item) => {
    return item.text.toLowerCase().includes(query);
  });

  if (filtered.length === 0) {
    historyList.innerHTML = `
      <div class="empty-state">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
        <h3>No matches found</h3>
        <p>Try searching for different terms.</p>
      </div>
    `;
    return;
  }

  historyList.innerHTML = "";
  filtered.forEach((item) => {
    const card = document.createElement("div");
    card.className = "card history-item-card";
    
    const date = new Date(item.timestamp);
    const dateStr = date.toLocaleString();
    const durationStr = `${item.duration.toFixed(1)}s`;
    
    card.innerHTML = `
      <div class="history-item-header">
        <div class="history-item-meta">
          <span>
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
            ${dateStr}
          </span>
          <span>
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>
            ${durationStr}
          </span>
        </div>
        <div class="history-item-actions">
          ${item.audio_path ? `
            <button class="btn btn-icon btn-sm play-btn" data-audio="${item.audio_path}" title="Play audio">
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>
            </button>
          ` : ""}
          <button class="btn btn-icon btn-sm copy-btn" data-text="${encodeURIComponent(item.text)}" title="Copy text">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
          </button>
          <button class="btn btn-icon btn-sm delete-btn" data-id="${item.id}" title="Delete record">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>
          </button>
        </div>
      </div>
      <div class="history-item-text">${item.text}</div>
    `;
    historyList.appendChild(card);
  });
}

// Event Listeners setup
function setupEventListeners() {
  // Record Button toggle
  recordBtn.addEventListener("click", () => {
    if (isRecording) {
      stopRecording();
    } else {
      startRecording();
    }
  });

  cancelRecordBtn.addEventListener("click", () => {
    cancelRecording();
  });

  // Settings change listeners
  micSelect.addEventListener("change", saveSettings);
  modelSelect.addEventListener("change", saveSettings);
  languageSelect.addEventListener("change", saveSettings);
  cjkToggle.addEventListener("change", saveSettings);
  autopasteToggle.addEventListener("change", saveSettings);
  soundsToggle.addEventListener("change", saveSettings);

  // Copy latest text button
  copyLatestBtn.addEventListener("click", () => {
    const text = resultText.value;
    if (text) {
      navigator.clipboard.writeText(text);
      
      // Temporary check mark icon change
      const originalSvg = copyLatestBtn.innerHTML;
      copyLatestBtn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#22c55e" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>`;
      setTimeout(() => {
        copyLatestBtn.innerHTML = originalSvg;
      }, 1500);
    }
  });

  // History search
  historySearch.addEventListener("input", loadHistory);

  // Clear all history
  clearHistoryBtn.addEventListener("click", async () => {
    if (confirm("Are you sure you want to delete all transcription history and audio files?")) {
      try {
        await invoke("clear_history");
        await loadHistory();
      } catch (err) {
        console.error("Failed to clear history:", err);
      }
    }
  });

  // Delegated events for Play, Copy, Delete in History List
  historyList.addEventListener("click", async (e) => {
    const target = e.target as HTMLElement;
    
    // Find closest button
    const btn = target.closest("button");
    if (!btn) return;

    if (btn.classList.contains("play-btn")) {
      const audioPath = btn.getAttribute("data-audio");
      if (audioPath) {
        try {
          await invoke("play_audio_file", { filePath: audioPath });
        } catch (err) {
          console.error("Playback failed:", err);
        }
      }
    } else if (btn.classList.contains("copy-btn")) {
      const encodedText = btn.getAttribute("data-text");
      if (encodedText) {
        const text = decodeURIComponent(encodedText);
        navigator.clipboard.writeText(text);
        
        const orig = btn.innerHTML;
        btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#22c55e" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>`;
        setTimeout(() => { btn.innerHTML = orig; }, 1500);
      }
    } else if (btn.classList.contains("delete-btn")) {
      const idStr = btn.getAttribute("data-id");
      if (idStr) {
        const id = parseInt(idStr, 10);
        if (confirm("Delete this recording?")) {
          try {
            await invoke("delete_history_item", { id });
            await loadHistory();
          } catch (err) {
            console.error("Delete failed:", err);
          }
        }
      }
    }
  });

  // Model Preset Download buttons
  const downloadBtns = document.querySelectorAll(".model-download-item .download-btn");
  downloadBtns.forEach((btn) => {
    btn.addEventListener("click", async () => {
      const item = btn.closest(".model-download-item") as HTMLElement;
      const modelName = item.getAttribute("data-model");
      if (modelName) {
        startModelDownload(modelName);
      }
    });
  });

  // Cancel download HUD
  cancelHudDownload.addEventListener("click", async () => {
    if (downloadingModelName) {
      try {
        await invoke("cancel_model_download", { modelName: downloadingModelName });
      } catch (err) {
        console.error("Failed to cancel download:", err);
      }
    }
  });
}

// Start Recording
async function startRecording() {
  if (!currentSettings.selected_model_path) {
    alert("Please select a Whisper model in settings first!");
    return;
  }

  try {
    await invoke("start_recording");
    
    isRecording = true;
    document.querySelector(".main-recorder-card")?.classList.add("recording");
    recorderStatus.textContent = "Recording...";
    recorderActions.style.display = "block";
    
    // Timer
    recordingDurationSeconds = 0;
    updateTimerText();
    recordTimerInterval = setInterval(() => {
      recordingDurationSeconds++;
      updateTimerText();
    }, 1000);
  } catch (err) {
    console.error("Failed to start recording:", err);
    alert("Failed to start recording: " + err);
  }
}

// Stop Recording
async function stopRecording() {
  if (recordTimerInterval) {
    clearInterval(recordTimerInterval);
    recordTimerInterval = null;
  }

  isRecording = false;
  document.querySelector(".main-recorder-card")?.classList.remove("recording");
  recorderStatus.textContent = "Transcribing speech...";
  recorderActions.style.display = "none";

  try {
    const historyItem: any = await invoke("stop_recording");
    
    if (historyItem) {
      resultText.value = historyItem.text;
      updateTextCounts(historyItem.text);
      recorderStatus.textContent = "Completed & Copied to clipboard!";
      
      if (currentSettings.auto_paste) {
        recorderStatus.textContent = "Completed, Copied & Autopasted!";
      }
    } else {
      recorderStatus.textContent = "Recording was too short or empty.";
    }
  } catch (err: any) {
    console.error("Transcription failed:", err);
    recorderStatus.textContent = "Error: " + err;
    alert("Transcription failed: " + err);
  }
}

// Cancel Recording
async function cancelRecording() {
  if (recordTimerInterval) {
    clearInterval(recordTimerInterval);
    recordTimerInterval = null;
  }

  isRecording = false;
  document.querySelector(".main-recorder-card")?.classList.remove("recording");
  recorderStatus.textContent = "Recording cancelled.";
  recorderActions.style.display = "none";

  try {
    await invoke("cancel_recording");
  } catch (err) {
    console.error("Failed to cancel recording:", err);
  }
}

// Update Timer Text
function updateTimerText() {
  const minutes = Math.floor(recordingDurationSeconds / 60);
  const seconds = recordingDurationSeconds % 60;
  const pad = (num: number) => num.toString().padStart(2, "0");
  recordingTimer.textContent = `${pad(minutes)}:${pad(seconds)}`;
}

// Update character and word counts
function updateTextCounts(text: string) {
  charCount.textContent = `${text.length} characters`;
  const words = text.trim() ? text.trim().split(/\s+/).length : 0;
  wordCount.textContent = `${words} words`;
}

// Setup system Drag & Drop
function setupDragAndDrop() {
  // Listen to Tauri native drag and drop events
  listen("tauri://drag-over", () => {
    document.getElementById("drop-zone")?.classList.add("dragover");
  });

  listen("tauri://drag-drop", async (event: any) => {
    document.getElementById("drop-zone")?.classList.remove("dragover");
    
    const paths = event.payload.paths as string[];
    if (paths && paths.length > 0) {
      const file = paths[0];
      if (file.toLowerCase().endsWith(".wav")) {
        await transcribeDroppedFile(file);
      } else {
        alert("Dropped file must be a .wav format.");
      }
    }
  });

  listen("tauri://drag-leave", () => {
    document.getElementById("drop-zone")?.classList.remove("dragover");
  });
}

// Transcribe dropped WAV file
async function transcribeDroppedFile(filePath: string) {
  recorderStatus.textContent = "Transcribing dropped file...";
  resultText.value = "";
  
  // Highlight dictate tab
  navItems.forEach((nav) => nav.classList.remove("active"));
  document.querySelector('[data-tab="dictate"]')?.classList.add("active");
  tabContents.forEach((tab) => tab.classList.remove("active"));
  document.getElementById("tab-dictate")?.classList.add("active");

  try {
    const historyItem: any = await invoke("transcribe_file", { filePath });
    resultText.value = historyItem.text;
    updateTextCounts(historyItem.text);
    recorderStatus.textContent = "File transcribed successfully & copied!";
  } catch (err: any) {
    console.error("Dropped file transcription failed:", err);
    recorderStatus.textContent = "Error: " + err;
    alert("Dropped file transcription failed: " + err);
  }
}

// Model Downloads
async function startModelDownload(modelName: string) {
  try {
    downloadingModelName = modelName;
    hudModelName.textContent = modelName;
    hudPercentage.textContent = "0%";
    hudProgressBar.style.width = "0%";
    downloadHud.style.display = "block";
    
    await invoke("download_model", { modelName });
  } catch (err) {
    console.error("Failed to start model download:", err);
    alert("Failed to start download: " + err);
    downloadHud.style.display = "none";
    downloadingModelName = null;
  }
}

// Listen for download progress updates
function setupDownloadListener() {
  listen("download-progress", async (event: any) => {
    const payload = event.payload as {
      model_name: string;
      progress: number;
      status: string;
      error: string | null;
    };
    
    if (payload.model_name !== downloadingModelName) return;
    
    if (payload.status === "downloading") {
      const percentage = Math.round(payload.progress);
      hudPercentage.textContent = `${percentage}%`;
      hudProgressBar.style.width = `${percentage}%`;
    } else if (payload.status === "completed") {
      downloadHud.style.display = "none";
      downloadingModelName = null;
      alert(`Model ${payload.model_name} downloaded successfully!`);
      await loadModels();
    } else if (payload.status === "cancelled") {
      downloadHud.style.display = "none";
      downloadingModelName = null;
      alert("Model download cancelled.");
    } else if (payload.status === "error") {
      downloadHud.style.display = "none";
      downloadingModelName = null;
      alert(`Model download failed: ${payload.error}`);
    }
  });
}
