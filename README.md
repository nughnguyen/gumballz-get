<p align="center">
  <br />
  <svg width="96" height="96" viewBox="0 0 100 100" fill="none" xmlns="http://www.w3.org/2000/svg">
    <rect x="10" y="10" width="80" height="80" rx="20" fill="#22D3EE" stroke="#0F172A" stroke-width="4"/>
    <rect x="18" y="18" width="64" height="64" rx="14" fill="#090D16" stroke="#0F172A" stroke-width="3"/>
    <path d="M34 50h32M50 34v32" stroke="#22D3EE" stroke-width="5" stroke-linecap="round"/>
    <circle cx="50" cy="50" r="10" fill="#A3E635" stroke="#0F172A" stroke-width="2.5"/>
  </svg>
  <h1 align="center">GumballZ Get</h1>
  <p align="center">
    <strong>Universal Downloader, Course Grabber & Modular Media Suite</strong><br />
    <em>Commercial-grade, high-performance toolkit powered by Tauri v2, Rust, Svelte 5, and Chrome Manifest V3.</em>
  </p>
  <p align="center">
    <a href="https://github.com/nughnguyen/gumballz-get/releases/latest"><img src="https://img.shields.io/badge/version-1.0.0-22D3EE?style=for-the-badge&labelColor=090D16" alt="Version 1.0.0" /></a>
    <a href="https://github.com/nughnguyen/gumballz-get/releases"><img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20Web-A3E635?style=for-the-badge&labelColor=090D16" alt="Platforms" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-FBBF24?style=for-the-badge&labelColor=090D16" alt="License GPL-3.0" /></a>
    <a href="https://zypage.com/gumballz"><img src="https://img.shields.io/badge/support-Zypage%20%7C%20VietQR-EC4899?style=for-the-badge&labelColor=090D16" alt="Support" /></a>
  </p>
</p>

---

## ⚡ Overview

**GumballZ Get** is an independent, unified media ecosystem created by **Nguyen Quoc Hung (GumballZ)**. It blends desktop performance, web accessibility, and browser automation into a cohesive Neo-Brutalist experience:

1. **Native Desktop Application (Tauri v2 + Rust):** Fully offline-capable, private desktop client embedding `yt-dlp`, `FFmpeg`, `librqbit` (torrent engine), `whisper.cpp` (transcription), and 158+ specialized tools across 25 categories.
2. **Standalone Web Application (`build-web/`):** High-speed Neo-Brutalism + Bento Grid web portal ready for zero-config deployment on Vercel or Cloudflare Pages, with real-time dynamic VietQR donations and integrated GitHub Releases hub.
3. **Dual-Mode Chrome Companion Extension (Manifest V3):** Sniff and grab video/audio streams directly inside Chromium browsers, or relay cookies and session data to the desktop app via constant-time authenticated local loopback (`127.0.0.1:47720-47729`).

---

## 🎨 Design System: Neo-Brutalism + Bento Grid

GumballZ Get is built strictly on modern Neo-Brutalist design principles with zero visual noise:

* **Bento Grid Architecture:** Modular, responsive layouts (`gap: 16px`, `padding: 20px`) adapting fluidly from mobile to widescreen monitors.
* **Hard Offset Shadows:** Pure tactile feedback with hard `4px 4px 0px #0F172A` drop shadows and physical button depression on click (`:active`).
* **High-Contrast Palette:**
  * **Deep Ink Dark Mode (Default):** Canvas `#090D16`, Card Surface `#111827`, Electric Cyan `#22D3EE`, Acid Lime `#A3E635`, Amber `#FBBF24`, Foreground Text `#F8FAFC`.
  * **Clean Brutalist Light Mode:** Crisp white cards, high-contrast borders, and vivid accent badges.
* **Razor-Sharp Typography:** Clear visual hierarchy using `Space Grotesk` / `Plus Jakarta Sans` for titles, `Inter` for content, and `JetBrains Mono` for hashes, technical telemetry, and file paths.

---

## 📦 Three Ecosystem Products in One Monorepo

```
gumballz-get/
├── src-tauri/                 # Tauri v2 Desktop Core (Rust backend + IPC commands)
│   ├── gumballz-core/         # Download engines, extraction, FFmpeg pipelines
│   ├── gumballz-plugin-sdk/   # Plugin system API & ABI handshake
│   └── plugins/               # Built-in extension plugins (Listening Studio, Asset Forge, etc.)
├── src/                       # Svelte 5 + SvelteKit Frontend (Desktop & Web)
│   ├── lib/core/              # PlatformBridge (Runtime boundary for Browser & Tauri)
│   ├── lib/donate/            # Dynamic VietQR generator & Zypage donation config
│   ├── lib/releases/          # Live GitHub Releases API client with local cache
│   └── routes/releases/       # Releases Hub & Download Bento page
├── browser-extension/         # Chrome & Firefox Companion Extension (Manifest V3)
│   ├── chrome/                # Chromium MV3 bundle
│   └── firefox/               # Gecko bundle
└── web-worker/                # Optional self-hosted Rust Axum backend for web quick-grab
```

---

## 🚀 Download & Installation

### 1. Native Desktop App

Download the latest verified stable binaries from [GumballZ Get Releases](https://github.com/nughnguyen/gumballz-get/releases):

| Platform | Format | Architecture | Direct Download |
| :--- | :--- | :--- | :--- |
| **Windows** | `.exe` (Installer) | x64 (Windows 10 / 11) | [Download Setup .exe](https://github.com/nughnguyen/gumballz-get/releases/latest) |
| **Windows** | `.msi` (Enterprise) | x64 | [Download .msi](https://github.com/nughnguyen/gumballz-get/releases/latest) |
| **macOS** | `.dmg` | Apple Silicon (M1/M2/M3/M4) & Intel | [Download .dmg](https://github.com/nughnguyen/gumballz-get/releases/latest) |
| **Linux** | `.AppImage` | x86_64 / arm64 | [Download .AppImage](https://github.com/nughnguyen/gumballz-get/releases/latest) |
| **Linux** | `.deb` | Debian / Ubuntu | [Download .deb](https://github.com/nughnguyen/gumballz-get/releases/latest) |

### 2. Chrome Companion Extension (MV3)

1. Download **`gumballz-get-chrome-extension-v1.0.0.zip`** from [Releases](https://github.com/nughnguyen/gumballz-get/releases) or the `dist/` directory.
2. Unpack the `.zip` archive to a local folder.
3. Open `chrome://extensions` in your browser and enable **Developer mode** (top-right toggle).
4. Click **Load unpacked** and select the unzipped directory.

### 3. Deploying the Standalone Web App

The web frontend can be built and deployed independently without any Tauri dependencies:

```bash
# 1. Install dependencies
pnpm install

# 2. Build standalone web static bundle
pnpm build:web

# 3. Output directory is ready for Vercel / Cloudflare Pages:
# ./build-web/
```

---

## 🧩 Built-in Plugin Ecosystem

GumballZ Get features a modular Plugin SDK architecture defined in [`docs/PLUGIN_SDK.md`](docs/PLUGIN_SDK.md):

* **🎧 Listening & Vocab Studio (`src-tauri/plugins/listening-studio/`):**
  Parses subtitle files (`.srt`, `.vtt`) associated with downloaded video or audio, leverages `FFmpeg` to cleanly slice spoken dialogue into targeted shadowing clips, and generates Cloze comprehension worksheets in Markdown and CSV.
* **⚡ Batch Media & Asset Forge (`src-tauri/plugins/asset-forge/`):**
  A high-throughput, multi-threaded asset converter utilizing Rust's `rayon` and `FFmpeg` to transform entire directories of video, audio, and images into optimized web formats (`.webp`, `.ogg`, `.mp3`, `.flac`) with real-time progress monitoring.

---

## 🛡️ Security & Privacy Guard

* **Zero Tracking:** 100% telemetry-free. Your downloads, search queries, and local files remain strictly on your machine.
* **Strict CSP:** No `unsafe-eval`. External image loading is locked down exclusively to verified endpoints (`https://img.vietqr.io` for dynamic QR codes).
* **Process Boundary:** All external arguments passed to `yt-dlp` or `FFmpeg` are segmented via distinct `.arg(...)` vectors with path traversal guards.
* **Loopback RPC Security:** Local desktop bridge binds exclusively to `127.0.0.1` using constant-time token comparison and origin validation.

---

## ☕ Support & Donations

GumballZ Get is completely free and open-source under the GPL-3.0 license. If you find the software valuable, support its ongoing maintenance and development:

* **Zypage:** [https://zypage.com/gumballz](https://zypage.com/gumballz)
* **VietQR Bank Transfer:**
  * **Bank:** OCB (Ngân hàng Phương Đông)
  * **Account Number:** `0388205003`
  * **Account Name:** `NGUYEN QUOC HUNG`
  * **Memo:** `Ung Ho GumballZ Get`

<p align="center">
  <a href="https://zypage.com/gumballz">
    <img src="https://img.vietqr.io/image/OCB-0388205003-payos.svg?addInfo=Ung+Ho+GumballZ+Get&accountName=NGUYEN+QUOC+HUNG" alt="VietQR GumballZ Get" width="280" />
  </a>
</p>

---

## License

**GumballZ Get** is free and open-source software licensed under the **GNU General Public License v3.0 (GPL-3.0-or-later)**.

- Copyright © 2026 **Nguyen Quoc Hung (GumballZ)**
- Source code: [https://github.com/nughnguyen/gumballz-get](https://github.com/nughnguyen/gumballz-get)

You are free to use, study, modify, and distribute this software under the terms of the [LICENSE](./LICENSE) file. This program comes with **NO WARRANTY**, to the extent permitted by applicable law.
