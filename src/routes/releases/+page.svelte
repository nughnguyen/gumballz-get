<!--
  GumballZ Get - Modern cross-platform download manager
  Copyright (C) 2026 Nguyen Quoc Hung (GumballZ) <https://github.com/nughnguyen/gumballz-get>

  This program is free software: you can redistribute it and/or modify
  it under the terms of the GNU General Public License as published by
  the Free Software Foundation, either version 3 of the License, or
  (at your option) any later version.

  This program is distributed in the hope that it will be useful,
  but WITHOUT ANY WARRANTY; without even the implied warranty of
  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
  GNU General Public License for more details.

  You should have received a copy of the GNU General Public License
  along with this program. If not, see <https://www.gnu.org/licenses/>.
-->

<script lang="ts">
  import { onMount } from "svelte";
  import { fetchLatestRelease } from "$lib/releases/releases";
  import type { ReleaseDownloads } from "$lib/releases/types";
  import BrandMark from "$lib/ui/BrandMark.svelte";
  import DonateTrigger from "$components/donate/DonateTrigger.svelte";

  let release = $state<ReleaseDownloads | null>(null);
  let loading = $state(true);

  onMount(async () => {
    try {
      release = await fetchLatestRelease();
    } finally {
      loading = false;
    }
  });
</script>

<svelte:head>
  <title>Releases & Downloads — GumballZ Get</title>
</svelte:head>

<div class="releases-container">
  <!-- Top Hero Bento -->
  <header class="releases-hero bento-card">
    <div class="hero-header">
      <BrandMark size={48} />
      <div class="hero-titles">
        <div class="version-row">
          <h1>GumballZ Get</h1>
          <span class="version-badge">{release?.version ?? "v1.0.0"}</span>
          <span class="live-tag">Latest Stable</span>
        </div>
        <p class="hero-sub">Universal Downloader & Media Player — Desktop, Web & Companion Extension</p>
      </div>
    </div>
    <div class="hero-actions">
      <a
        href="https://github.com/nughnguyen/gumballz-get/releases"
        target="_blank"
        rel="noopener noreferrer"
        class="brutal-btn secondary"
      >
        View on GitHub Releases ↗
      </a>
      <DonateTrigger />
    </div>
  </header>

  <!-- Bento Grid -->
  <main class="releases-grid">
    <!-- Desktop App Card (Windows, macOS, Linux) -->
    <section class="bento-card bento-desktop">
      <div class="card-header">
        <span class="step-num">01 / DESKTOP APPS</span>
        <h2>Native Desktop Experience</h2>
        <p class="card-desc">
          Full power with embedded yt-dlp, FFmpeg, 4K/8K stream extraction, multi-threaded acceleration, and 156 utility tools.
        </p>
      </div>

      <div class="download-buttons-group">
        <!-- Windows -->
        <div class="platform-block">
          <div class="platform-title">
            <strong>Windows</strong>
            <span class="meta-tag">10 / 11 (64-bit)</span>
          </div>
          <div class="btn-row">
            <a
              href={release?.windowsExe ?? "https://github.com/nughnguyen/gumballz-get/releases/latest"}
              target="_blank"
              rel="noopener noreferrer"
              class="brutal-btn primary"
            >
              Download .exe (Setup)
            </a>
            {#if release?.windowsMsi}
              <a
                href={release.windowsMsi}
                target="_blank"
                rel="noopener noreferrer"
                class="brutal-btn subtle"
              >
                .msi
              </a>
            {/if}
          </div>
        </div>

        <!-- macOS -->
        <div class="platform-block">
          <div class="platform-title">
            <strong>macOS</strong>
            <span class="meta-tag">Apple Silicon & Intel</span>
          </div>
          <div class="btn-row">
            <a
              href={release?.macDmg ?? "https://github.com/nughnguyen/gumballz-get/releases/latest"}
              target="_blank"
              rel="noopener noreferrer"
              class="brutal-btn primary"
            >
              Download .dmg
            </a>
          </div>
        </div>

        <!-- Linux -->
        <div class="platform-block">
          <div class="platform-title">
            <strong>Linux</strong>
            <span class="meta-tag">AppImage & Debian</span>
          </div>
          <div class="btn-row">
            <a
              href={release?.linuxAppImage ?? "https://github.com/nughnguyen/gumballz-get/releases/latest"}
              target="_blank"
              rel="noopener noreferrer"
              class="brutal-btn primary"
            >
              Download .AppImage
            </a>
            {#if release?.linuxDeb}
              <a
                href={release.linuxDeb}
                target="_blank"
                rel="noopener noreferrer"
                class="brutal-btn subtle"
              >
                .deb
              </a>
            {/if}
          </div>
        </div>
      </div>
    </section>

    <!-- Chrome Extension Card -->
    <section class="bento-card bento-extension">
      <div class="card-header">
        <span class="step-num">02 / CHROME EXTENSION</span>
        <h2>GumballZ Get Companion (MV3)</h2>
        <p class="card-desc">
          Dual-mode extension: sniffs media streams directly in Chrome or bridges seamlessly to the Desktop app via local RPC.
        </p>
      </div>

      <div class="extension-actions">
        <a
          href={release?.chromeExtensionZip ?? "https://github.com/nughnguyen/gumballz-get/tree/main/browser-extension"}
          target="_blank"
          rel="noopener noreferrer"
          class="brutal-btn extension-btn"
        >
          Download Extension (.zip)
        </a>
        <a
          href="https://github.com/nughnguyen/gumballz-get/tree/main/browser-extension"
          target="_blank"
          rel="noopener noreferrer"
          class="brutal-btn subtle"
        >
          Source & Docs ↗
        </a>
      </div>

      <div class="install-guide">
        <h3>Quick Installation (3 Steps)</h3>
        <ol>
          <li>Extract the downloaded <code>.zip</code> file to a folder.</li>
          <li>Open <code>chrome://extensions</code> in Chrome and enable <strong>Developer mode</strong> (top-right).</li>
          <li>Click <strong>Load unpacked</strong> and select the extracted folder.</li>
        </ol>
      </div>
    </section>

    <!-- Release Notes Bento Card -->
    <section class="bento-card bento-notes">
      <div class="card-header">
        <span class="step-num">03 / RELEASE NOTES</span>
        <h2>What's New in {release?.version ?? "v1.0.0"}</h2>
      </div>
      <div class="notes-content">
        <p>{release?.notes ?? "Initial release of GumballZ Get with Neo-Brutalism + Bento Grid UI, Chrome Companion MV3, and Plugin SDK."}</p>
        <ul class="feature-bullets">
          <li><strong>Clean Architecture:</strong> 100% independent codebase by Nguyen Quoc Hung.</li>
          <li><strong>Neo-Brutalism Design:</strong> High contrast, sharp borders, zero visual clutter.</li>
          <li><strong>VietQR Dynamic Support:</strong> Instant 350ms debounced QR generator for OCB donations.</li>
          <li><strong>Plugin SDK:</strong> Listening & Vocab Studio, Batch Media Asset Forge, and extension hooks.</li>
        </ul>
      </div>
    </section>
  </main>
</div>

<style>
  .releases-container {
    max-width: 1200px;
    margin: 0 auto;
    padding: clamp(16px, 3vw, 36px);
    display: flex;
    flex-direction: column;
    gap: 24px;
    color: var(--text, #f8fafc);
  }

  .bento-card {
    background: var(--surface, #111827);
    border: 2px solid var(--border-ink, #334155);
    border-radius: 14px;
    padding: clamp(18px, 2.5vw, 28px);
    box-shadow: 4px 4px 0px rgba(0, 0, 0, 0.4);
    transition: transform 0.15s ease, box-shadow 0.15s ease;
  }

  .releases-hero {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 20px;
  }

  .hero-header {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .version-row {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .version-row h1 {
    margin: 0;
    font-size: clamp(22px, 3vw, 32px);
    font-weight: 800;
    letter-spacing: -0.03em;
  }

  .version-badge {
    background: #22d3ee;
    color: #090d16;
    padding: 3px 10px;
    border: 2px solid var(--border-ink);
    border-radius: 6px;
    font-weight: 700;
    font-size: 13px;
    font-family: var(--font-mono, monospace);
  }

  .live-tag {
    background: #a3e635;
    color: #090d16;
    padding: 3px 8px;
    border: 2px solid var(--border-ink);
    border-radius: 6px;
    font-weight: 700;
    font-size: 11px;
    text-transform: uppercase;
  }

  .hero-sub {
    margin: 4px 0 0;
    color: var(--text-dim);
    font-size: 14px;
  }

  .hero-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .releases-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: 24px;
  }

  .step-num {
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--accent-text);
    margin-bottom: 6px;
    display: block;
  }

  .card-header h2 {
    margin: 0 0 8px;
    font-size: 20px;
    font-weight: 700;
  }

  .card-desc {
    margin: 0 0 20px;
    font-size: 14px;
    line-height: 1.5;
    color: var(--text-muted);
  }

  .download-buttons-group {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .platform-block {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    border-radius: 8px;
  }

  .platform-title {
    display: flex;
    justify-content: space-between;
    font-size: 14px;
  }

  .meta-tag {
    font-size: 12px;
    color: var(--text-dim);
  }

  .btn-row {
    display: flex;
    gap: 8px;
  }

  .brutal-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 9px 16px;
    border-radius: 8px;
    font-weight: 700;
    font-size: 13px;
    text-decoration: none;
    cursor: pointer;
    border: 2px solid var(--border-ink);
    transition: transform 0.1s ease, box-shadow 0.1s ease;
  }

  .brutal-btn.primary {
    background: var(--accent);
    color: var(--on-accent);
    box-shadow: 3px 3px 0px var(--shadow-hard);
  }

  .brutal-btn.extension-btn {
    background: #a3e635;
    color: #090d16;
    box-shadow: 3px 3px 0px var(--shadow-hard);
  }

  .brutal-btn.secondary {
    background: #f8fafc;
    color: #0f172a;
    box-shadow: 3px 3px 0px var(--shadow-hard);
  }

  .brutal-btn.subtle {
    background: #1e293b;
    color: #f8fafc;
    border-color: #334155;
    box-shadow: 2px 2px 0px rgba(0, 0, 0, 0.4);
  }

  .brutal-btn:hover {
    transform: translate(-1px, -1px);
    box-shadow: 4px 4px 0px var(--shadow-hard);
  }

  .brutal-btn:active {
    transform: translate(2px, 2px);
    box-shadow: 1px 1px 0px #0f172a;
  }

  .extension-actions {
    display: flex;
    gap: 10px;
    margin-bottom: 20px;
  }

  .install-guide {
    background: var(--surface-hi);
    padding: 16px;
    border-radius: 8px;
    border: 2px solid var(--border-ink);
    border-left: 3px solid #a3e635;
  }

  .install-guide h3 {
    margin: 0 0 10px;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text);
  }

  .install-guide ol {
    margin: 0;
    padding-left: 18px;
    font-size: 13px;
    line-height: 1.6;
    color: var(--text-muted);
  }

  .install-guide code {
    background: rgba(255, 255, 255, 0.1);
    padding: 1px 5px;
    border-radius: 4px;
    font-family: var(--font-mono, monospace);
  }

  .bento-notes {
    grid-column: 1 / -1;
  }

  .feature-bullets {
    margin: 12px 0 0;
    padding-left: 20px;
    font-size: 14px;
    line-height: 1.7;
    color: var(--text-muted);
  }
</style>
