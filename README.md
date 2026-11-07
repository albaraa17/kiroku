# 記録 Kiroku

> **High-Performance Visual Novel Text Hooking Bridge & WebSocket Server**  
> Built with Rust, Tauri v2, and SvelteKit.

[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%20x64-0078D6.svg?logo=windows)](https://github.com/albaraa17/kiroku)
[![Rust](https://img.shields.io/badge/Rust-2024%20Edition-DEA584.svg?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB.svg?logo=tauri)](https://tauri.app/)
[![SvelteKit](https://img.shields.io/badge/SvelteKit-5-FF3E00.svg?logo=svelte)](https://kit.svelte.dev/)

---

## Overview

**Kiroku (記録)** is a fast, lightweight text hook bridge and real-time streaming engine tailored for Japanese visual novel immersion reading.

Many readers rely on tools like Textractor or LunaTranslator to feed text to [exSTATic](https://github.com/Kuuuube/exSTATic), [Tadoku](https://tadoku.app), [Texthooker UI](https://github.com/Renji-XD/texthooker-ui), or clipboard lookups (Yomitan). However, legacy suites can be resource-heavy, consume excessive idle RAM, or struggle with background window automation.

Kiroku replaces heavy UI runtimes with a native **Rust + Tauri v2** architecture backed by the high-performance **LunaHook** engine via direct FFI. It provides sub-millisecond hook ingestion, an intelligent dialogue scoring algorithm, background input bypass, and a **Zero-RAM System Tray mode** that idles at **~4 MB of RAM and 0% CPU**.

---

## ✨ Features

- **Native LunaHook Engine (FFI)**:
  Direct FFI integration with `LunaHost64.dll` supporting both 32-bit (`x86`) and 64-bit (`x64`) visual novel engines (DirectX, KiriKiri, SiglusEngine, Unity, Artemis, WolfRPG, BGI/Ethornell, System4, Majiro, and more).

- **Zero-RAM Tray Mode (~4 MB RAM / 0% CPU)**:
  When minimized or closed to the tray, Kiroku unloads the WebView2 runtime entirely while keeping the Rust hooking thread and WebSocket server active in the background. Clicking the tray icon instantly recreates the UI on demand.

- **Intelligent Hook Auto-Selection (New Games)**:
  Heuristically evaluates hooked strings in real time (measuring Hiragana ratio, sentence length, and structural markers) to automatically lock onto genuine dialogue threads and ignore memory dumps, short speaker labels, or duplicate system strings.

- **Automatic Tadoku Library Sync**:
  Detects `%APPDATA%\tadoku\store.json` with zero-I/O mtime caching. Games in your Tadoku library are automatically recognized as saved targets and auto-attached as soon as they launch.

- **Simultaneous Dual-Format WebSocket Server (`:9001`)**:
  Connect multiple consumers with different format requirements at the same time:
  - `ws://127.0.0.1:9001/raw` or `/text` → Plain Japanese text (Texthooker UI, Kizuna, LapClipboard).
  - `ws://127.0.0.1:9001/exstatic` or `/json` → Structured JSON payloads `{ process_id, process_path, sentence }` (exSTATic, Tadoku).
  - Configurable root format for default connections.

- **Background Input Bypass (`kiroku_bginput`)**:
  Bundles native 32-bit and 64-bit helper DLLs (`kiroku_bginput32/64.dll`) that intercept Windows message routing in the target game. This allows instant `ControlClick` background inputs without focus stealing or interference from foreground web browsers.

- **Extensive Text Cleaning & Deduplication Pipeline**:
  - Repetition filter for stutter characters (`お、お、おはよう` → `おはよう`) and looped dialogue phrases.
  - Native dual-language Chinese/Japanese filter (strips embedded Chinese subtitle clauses from dual-language releases while keeping Japanese kanji/kana intact).
  - Speaker bracket and quote-prefix normalizer.
  - Custom regex filter and replacement list.

- **Knife / External WebSocket Ingest**:
  Connects to Knife or external OCR/hook sources (`ws://127.0.0.1:6677`) to multiplex external text alongside native hooks.

- **Legacy Config Import**:
  One-click import of your existing `config64.json`, `SavedRegexFilters.txt`, and `SavedReplacements.txt` from LunaTranslator or Textractor.

- **Windows Integration**:
  Built-in support for Windows autostart (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`) with optional "Start Minimized in Zero-RAM Mode".

---

## 🚀 Getting Started

### Installation

1. Download the latest release from the [Releases](https://github.com/albaraa17/kiroku/releases) page:
   - **`Kiroku_x.x.x_x64-setup.exe`**: Standard Windows installer.
   - **`kiroku.exe`**: Portable standalone executable.
2. Launch Kiroku.
3. Open your visual novel. Kiroku will detect running windowed games and auto-attach to saved or Tadoku library titles.
4. Point your favorite reader or texthooker to `ws://127.0.0.1:9001`.

---

## 🛡️ Antivirus / Windows Defender False-Positive Notice

> [!WARNING]
> Because Kiroku injects hooking DLLs (`LunaHook32.dll`, `LunaHook64.dll`, `kiroku_bginput*.dll`) into game processes via Windows APIs (`CreateRemoteThread`, `VirtualAllocEx`, and `SetWindowsHookEx`), **Windows Defender or third-party antivirus software may flag the executable as a generic game hack or trojan.**

This is a well-known false positive common to all visual novel text-hooking software (including Textractor and LunaTranslator).
- The full source code is open and verifiable in this repository.
- You can build the binaries directly from source if you prefer not to run precompiled releases.
- If flagged, add a Windows Defender folder exclusion for the Kiroku installation folder.

---

## 🛠️ Building from Source

### Prerequisites

1. **Rust**: Latest stable toolchain (`2024` edition compatible).
   ```powershell
   rustup default stable
   ```
2. **Node.js / Bun**: For compiling the SvelteKit frontend.
   ```powershell
   bun --version   # or npm / pnpm
   ```
3. **Microsoft C++ Build Tools**: Required for Windows native compilation.
4. **WebView2 Runtime**: Standard on Windows 10/11.

### Build Steps

```powershell
# 1. Clone the repository
git clone https://github.com/albaraa17/kiroku.git
cd kiroku

# 2. Install frontend dependencies
bun install

# 3. Run development build
bun run tauri dev

# 4. Compile release binaries & installer
bun run tauri build
```

Compiled outputs will be located in:
- Portable executable: `src-tauri/target/release/kiroku.exe`
- NSIS installer: `src-tauri/target/release/bundle/nsis/Kiroku_*_x64-setup.exe`

---

## 📡 WebSocket API Specification

Kiroku broadcasts every newly hooked and cleaned sentence across `ws://127.0.0.1:9001`:

### 1. Plain Text Endpoint (`/raw` or `/text`)
Yields raw string UTF-8 messages:
```text
「桜の木の下で、彼女は立ち止まった。」
```

### 2. Structured JSON Endpoint (`/exstatic` or `/json`)
Yields JSON payloads compliant with exSTATic and Tadoku:
```json
{
  "process_id": 14220,
  "process_path": "D:\\Games\\VisualNovels\\Game.exe",
  "sentence": "「桜の木の下で、彼女は立ち止まった。」"
}
```

---

## 🤝 Acknowledgments & Credits

Kiroku is built upon the foundational work of the visual novel reverse engineering and text-hooking community:

- **[LunaHook](https://github.com/AuroraWright/LunaHook)** by **AuroraWright** — The high-performance C++ text hooking core.
- **[LunaTranslator](https://github.com/HIllya51/LunaTranslator)** by **HIllya51** — Tooling ecosystem and hook definitions.
- **[Textractor](https://github.com/Artikash/Textractor)** by **Artikash** — Foundation of modern visual novel text hooking.
- **[Tadoku](https://github.com/albaraa17/tadoku)** & **[exSTATic](https://github.com/Kuuuube/exSTATic)** — Inspiration for low-friction immersion reading workflows.

---

## 📄 License

This project is licensed under the **GNU General Public License v3.0 (GPL-3.0)**. See the [LICENSE](LICENSE) file for details.
