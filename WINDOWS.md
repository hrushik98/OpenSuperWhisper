# OpenSuperWhisper Windows Port Build Guide

This directory contains the Tauri-based Windows port of OpenSuperWhisper. The app uses Rust for audio capture and SQLite state and a HTML/CSS/TypeScript front-end for visual feedback. Local transcription runs via native `whisper.cpp` linked through the `whisper-rs` crate.

---

## 🛠️ Local Build Requirements

To compile OpenSuperWhisper locally on a Windows machine:

1.  **Rust Toolchain**: Install via [rustup.rs](https://rustup.rs).
2.  **Node.js**: Install Node.js 18+ (LTS recommended) from [nodejs.org](https://nodejs.org).
3.  **C++ Build Tools**: Install Visual Studio Build Tools with the "Desktop development with C++" workload (needed to compile the native `whisper.cpp` C++ engine).
4.  **Wix Toolset (for MSI bundle)**: Download and install Wix Toolset v3 from [wixtoolset.org](https://wixtoolset.org/).
5.  **NSIS (for EXE installer)**: Download and install NSIS from [nsis.sourceforge.io](https://nsis.sourceforge.io/). Ensure NSIS is added to your System `PATH`.

---

## 🚀 Commands

Navigate into the `opensuperwhisper-tauri` directory:

```bash
cd opensuperwhisper-tauri
```

### Dev Mode
Starts the hot-reloading Vite server and opens the Tauri native window framework wrapper:
```bash
npm run tauri dev
```

### Production Build
Compiles all files, compiles the Rust backend in release mode, and packages the installers (`.msi` and `.exe`):
```bash
npm run tauri build
```
Once completed, the installers will be available in:
`src-tauri/target/release/bundle/nsis/` (Executable Installer)
`src-tauri/target/release/bundle/msi/` (MSI Installer)

---

## 📁 System Paths & Local Storage

*   **Settings File**: User preferences are saved in `%APPDATA%\com.starmel.opensuperwhisper\settings.json`.
*   **Database File**: SQLite transcription history and records are stored in `%APPDATA%\com.starmel.opensuperwhisper\history.db`.
*   **WAV Recordings**: Temporary audio recordings are saved in `%APPDATA%\com.starmel.opensuperwhisper\recordings\`.
*   **Whisper Models**: Model binary `.bin` files downloaded from HuggingFace are located in `%APPDATA%\com.starmel.opensuperwhisper\models\`.

---

## 🌐 GitHub Actions CI/CD Build

The repository includes a GitHub Action workflow configured in [.github/workflows/windows-tauri.yml](.github/workflows/windows-tauri.yml). 

Whenever you push to `main` or trigger manually, the runner installs the dependencies, builds the installer, and uploads the NSIS `.exe` and `.msi` installers as artifacts to the workflow run page.
