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
  import { onMount, onDestroy } from "svelte";
  import DonateTrigger from "$components/donate/DonateTrigger.svelte";
  import BrandMark from "$lib/ui/BrandMark.svelte";
  import { t } from "$lib/i18n";
  import { fetchLatestRelease, fetchAllReleases } from "$lib/releases/releases";
  import type { ReleaseDownloads, ReleaseVersion } from "$lib/releases/types";
  import { formatBytes } from "$lib/download-format";

  const LATEST_RELEASE_URL = "https://github.com/nughnguyen/gumballz-get/releases/latest";
  const RELEASES_PAGE = "https://github.com/nughnguyen/gumballz-get/releases";
  const EXTENSION_SOURCE_URL = "https://github.com/nughnguyen/gumballz-get/tree/main/browser-extension";

  /**
   * Real counts, verified against src/lib/tools/catalog.ts:
   * TOOLS.length = 156 (127 ready, 27 beta, 2 soon), CATEGORIES.length = 24.
   * The old "158+ tools / 25 categories" copy was stale — do not reintroduce it.
   */
  const TOOLS_COUNT = 156;
  const CATEGORIES_COUNT = 24;
  const PLATFORMS_COUNT = 3;

  /** Proper-noun OS labels — identical in every locale. */
  const OS_LABELS: Record<string, string> = {
    windows: "Windows",
    macos: "macOS",
    linux: "Linux",
    extension: "Chrome Extension"
  };

  let release = $state<ReleaseDownloads | null>(null);
  let versions = $state<ReleaseVersion[] | null>(null);
  let versionsLoading = $state(true);
  let versionsFailed = $state(false);
  let showOlderVersions = $state(false);

  /** Only the newest version is shown at first; older versions stay
      collapsed behind the toggle below the list. */
  let olderVersions = $derived((versions ?? []).slice(1));

  onMount(async () => {
    // The desktop app shell locks the document (body overflow: hidden in
    // app.css) because the Tauri window scrolls inside its own panes. The
    // web landing page is a normal scrolling document — opt out of that lock.
    document.documentElement.classList.add("web-scroll");
    try {
      release = await fetchLatestRelease();
    } catch {
      release = null;
    }
    try {
      versions = await fetchAllReleases();
    } catch {
      versionsFailed = true;
    } finally {
      versionsLoading = false;
    }
  });

  onDestroy(() => {
    document.documentElement.classList.remove("web-scroll");
  });

  function formatDate(iso: string): string {
    if (!iso) return "";
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return "";
    return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  }
</script>

<svelte:head>
  <title>GumballZ Get — Universal Downloader & Media Suite</title>
  <meta name="description" content="GumballZ Get — free desktop downloader with 156 media tools: 4K/8K video, courses, torrents, and a Chrome Companion extension." />
</svelte:head>

<main class="web-hub">
  <!-- Top Navigation Bar -->
  <header class="hub-header">
    <div class="brand-group">
      <img src="/favicon.png" alt="GumballZ Get logo" width="38" height="38" class="brand-mascot" draggable="false" />
      <a class="brand" href="/">GumballZ <span>Get</span></a>
      {#if release}<span class="version-tag">{release.version}</span>{/if}
    </div>
    <nav class="hub-nav">
      <a href="https://github.com/nughnguyen/gumballz-get/releases" target="_blank" rel="noopener noreferrer" class="nav-link">
        Releases ↗
      </a>
      <a href="https://github.com/nughnguyen/gumballz-get/tree/main/browser-extension" target="_blank" rel="noopener noreferrer" class="nav-link">
        Extension ↗
      </a>
      <DonateTrigger inline />
    </nav>
  </header>

  <!-- Hero Section -->
  <section class="hero-section">
    <div class="hero-top">
      <div class="hero-copy">
        <div class="hero-badge">FREE &amp; OPEN SOURCE</div>
        <h1 class="hero-title">Media, <span>your way.</span></h1>
        <p class="hero-subtitle">{$t("web.hero_subtitle")}</p>

        <!-- Latest release download -->
        <div class="download-cta">
          <a href={LATEST_RELEASE_URL} target="_blank" rel="noopener noreferrer" class="download-primary">
            Download GumballZ Get — Latest release ↓
          </a>
          <div class="platform-links">
            <a href={release?.windowsExe ?? LATEST_RELEASE_URL} target="_blank" rel="noopener noreferrer">Windows (.exe)</a>
            <span aria-hidden="true">·</span>
            <a href={release?.macDmg ?? LATEST_RELEASE_URL} target="_blank" rel="noopener noreferrer">macOS (.dmg)</a>
            <span aria-hidden="true">·</span>
            <a href={release?.linuxAppImage ?? LATEST_RELEASE_URL} target="_blank" rel="noopener noreferrer">Linux (.AppImage)</a>
            {#if release}<span class="latest-note">Latest: {release.version}</span>{/if}
          </div>
        </div>
      </div>
      <div class="hero-art">
        <BrandMark size={208} />
        {#if release}<span class="hero-version">{release.version}</span>{/if}
      </div>
    </div>
  </section>

  <!-- How it works — animated demo (this static page has no live backend) -->
  <section class="demo-section" aria-label={$t("web.demo_title")}>
    <div class="section-head">
      <span class="section-eyebrow">{$t("web.demo_eyebrow")}</span>
      <h2>{$t("web.demo_title")}</h2>
    </div>
    <figure class="demo-figure">
      <img src="/demo-download.gif" alt="Demo: paste a link, press Download, the file downloads" class="demo-gif" width="640" height="360" loading="lazy" />
      <figcaption class="demo-caption">{$t("web.demo_caption")}</figcaption>
    </figure>
  </section>

  <!-- Stats — real numbers from the codebase -->
  <section class="stats-section" aria-label={$t("web.stats_title")}>
    <h2 class="stats-title"><span class="stats-title-num">{TOOLS_COUNT}</span> {$t("web.stats_title")}</h2>
    <div class="stats-row">
      <div class="stat-card stat-tools">
        <span class="stat-num">{TOOLS_COUNT}</span>
        <span class="stat-label">{$t("web.stats_tools")}</span>
        <span class="stat-sub">{$t("web.stats_tools_sub")}</span>
      </div>
      <div class="stat-card stat-cats">
        <span class="stat-num">{CATEGORIES_COUNT}</span>
        <span class="stat-label">{$t("web.stats_categories")}</span>
        <span class="stat-sub">{$t("web.stats_categories_sub")}</span>
      </div>
      <div class="stat-card stat-platforms">
        <span class="stat-num">{PLATFORMS_COUNT}</span>
        <span class="stat-label">{$t("web.stats_platforms")}</span>
        <span class="stat-sub">{$t("web.stats_platforms_sub")}</span>
      </div>
      <div class="stat-card stat-sites">
        <span class="stat-label">{$t("web.stats_sites_label")}</span>
        <span class="stat-sites-list">YouTube · TikTok · Facebook · Instagram · X · Twitch</span>
      </div>
    </div>
  </section>

  <!-- All versions — direct downloads, no GitHub detour -->
  <section class="versions-section" aria-label={$t("web.versions_title")}>
    <div class="versions-head">
      <h2>{$t("web.versions_title")}</h2>
      <p>{$t("web.versions_subtitle")}</p>
    </div>

    {#if versionsLoading}
      <div class="versions-status">{$t("web.versions_loading")}</div>
    {:else if versionsFailed || !versions}
      <div class="versions-status error" role="alert">
        <span>{$t("web.versions_failed")}</span>
        <a href={LATEST_RELEASE_URL} target="_blank" rel="noopener noreferrer">{$t("web.download")} ↓</a>
      </div>
    {:else}
      {#each versions as v, i (v.version)}
        {#if i === 0 || showOlderVersions}
        <article class="version-card">
          <header class="version-head">
            <span class="version-name">{v.version}</span>
            {#if v.isLatest}<span class="latest-badge">{$t("web.latest_badge")}</span>{/if}
            {#if v.prerelease}<span class="prerelease-badge">{$t("web.prerelease_badge")}</span>{/if}
            {#if v.publishedAt}<span class="version-date">{$t("web.published", { date: formatDate(v.publishedAt) })}</span>{/if}
          </header>
          {#if v.files.length === 0}
            <p class="no-files">{$t("web.no_files")}</p>
          {:else}
            <ul class="file-list">
              {#each v.files as file (file.name)}
                <li class="file-row">
                  <span class="file-format">{file.format}</span>
                  <span class="file-meta">
                    <span class="file-os">{OS_LABELS[file.os] ?? $t("web.os_other")}</span>
                    {#if file.variant === "setup"}
                      <span class="file-variant">· {$t("web.format_setup")}</span>
                    {:else if file.variant === "portable"}
                      <span class="file-variant">· {$t("web.format_portable")}</span>
                    {/if}
                    <span class="file-name" title={file.name}>{file.name}</span>
                  </span>
                  <span class="file-size">{formatBytes(file.size)}</span>
                  <!-- Direct asset link: GitHub serves it as an attachment, so the
                       browser opens its native save-file dialog for this exact file. -->
                  <a class="file-download" href={file.url}>{$t("web.download")} ↓</a>
                </li>
              {/each}
            </ul>
          {/if}
        </article>
        {/if}
      {/each}
      {#if olderVersions.length > 0}
        <button
          type="button"
          class="versions-toggle"
          aria-expanded={showOlderVersions}
          onclick={() => (showOlderVersions = !showOlderVersions)}
        >
          {showOlderVersions ? $t("web.versions_hide_older") : $t("web.versions_show_older")}
        </button>
      {/if}
      <a class="versions-github" href={RELEASES_PAGE} target="_blank" rel="noopener noreferrer">{$t("web.view_all_github")} ↗</a>
    {/if}
  </section>

  <!-- Bento Grid Showcase -->
  <section class="bento-grid" aria-label="GumballZ Get Products">
    <!-- 01 Desktop App -->
    <a class="bento-card card-desktop" href={LATEST_RELEASE_URL} target="_blank" rel="noopener noreferrer">
      <div class="card-top">
        <span class="card-num">01 / DESKTOP APP</span>
        <h2>GumballZ Get</h2>
        <p class="card-sub">Windows · macOS · Linux</p>
        <p class="card-text">
          Local performance with embedded yt-dlp & FFmpeg, playlist extraction, 4K/8K media streams, and zero cloud tracking.
        </p>
      </div>
      <div class="card-bottom">
        <span class="action-pill">Download releases ↗</span>
      </div>
    </a>

    <!-- 02 Chrome Extension -->
    <a class="bento-card card-extension" href={release?.chromeExtensionZip ?? EXTENSION_SOURCE_URL} target="_blank" rel="noopener noreferrer">
      <div class="card-top">
        <span class="card-num">02 / CHROME EXTENSION</span>
        <h2>GumballZ Get Companion</h2>
        <p class="card-sub">Manifest V3 · Dual-Mode</p>
        <p class="card-text">
          Sniff media streams directly in Chrome or bridge authentication cookies to your Desktop app with 1 click.
        </p>
      </div>
      <div class="card-bottom">
        <span class="action-pill">Get Extension ↗</span>
      </div>
    </a>

    <!-- 03 Utility Tools -->
    <div class="bento-card card-tools">
      <div class="card-top">
        <span class="card-num">03 / MODULAR SUITE</span>
        <h2>{TOOLS_COUNT} Media Tools</h2>
        <p class="card-sub">{CATEGORIES_COUNT} Specialized Categories</p>
        <p class="card-text">
          Audio extractors, Whisper subtitle clippers, image converters, video compressors, PDF tools, and course scrapers.
        </p>
      </div>
      <div class="card-bottom">
        <span class="badge-pill">Plugin SDK Enabled</span>
      </div>
    </div>

    <!-- 04 Support & Donate -->
    <div class="bento-card card-support">
      <div class="card-top">
        <span class="card-num">04 / SUPPORT THE PROJECT</span>
        <h2>Support GumballZ</h2>
        <p class="card-sub">OCB Bank (VietQR) & Zypage</p>
        <p class="card-text">
          Independent, open-source software under GPL-3.0. Your support powers continuous releases and feature updates.
        </p>
      </div>
      <div class="card-bottom">
        <DonateTrigger inline />
      </div>
    </div>
  </section>

  <!-- Credits -->
  <section class="credits-section" aria-label="Credits">
    <div class="credits-card">
      <span class="card-num credits-num">CREDITS</span>
      <h2>Who makes GumballZ Get?</h2>
      <div class="credits-grid">
        <div><span class="credit-label">Designed & developed by</span><strong>GumballZ</strong></div>
        <div><span class="credit-label">License</span><strong><a href="https://github.com/nughnguyen/gumballz-get/blob/main/LICENSE" target="_blank" rel="noopener noreferrer">GPL-3.0</a></strong></div>
        <div><span class="credit-label">Source code</span><strong><a href="https://github.com/nughnguyen/gumballz-get" target="_blank" rel="noopener noreferrer">github.com/nughnguyen/gumballz-get</a></strong></div>
      </div>
      <p class="credits-note">Free and open-source software. Made with love by GumballZ — your support keeps the releases coming.</p>
    </div>
  </section>

  <!-- Clean Brutalist Footer -->
  <footer class="hub-footer">
    <div class="footer-left">
      <strong>GumballZ Get{#if release} {release.version}{/if}</strong> — Designed & Developed by <strong>GumballZ</strong>
    </div>
    <div class="footer-right">
      <a href="https://github.com/nughnguyen/gumballz-get" target="_blank" rel="noopener noreferrer">GitHub Repo ↗</a>
      <span>·</span>
      <a href="https://github.com/nughnguyen/gumballz-get/releases" target="_blank" rel="noopener noreferrer">Releases ↗</a>
      <span>·</span>
      <a href="https://zypage.com/gumballz" target="_blank" rel="noopener noreferrer">Zypage Donate ↗</a>
      <span>·</span>
      <span>GPL-3.0 License</span>
    </div>
  </footer>
</main>

<style>
  /* The desktop app shell locks the document (body overflow: hidden in
     app.css) because the Tauri window scrolls inside its own panes. The
     web landing page is a normal document — restore page scrolling.
     (Without this, body overflow:hidden propagates to the viewport and the
     page below the hero is unreachable.) */
  :global(html.web-scroll) {
    height: auto;
  }

  :global(html.web-scroll body) {
    height: auto;
    min-height: 100vh;
    overflow: visible;
    overscroll-behavior: auto;
  }

  /* Light pastel landing page — bright and friendly, neo-brutalist structure
     kept: 2px ink borders, hard shadows, 12px card radii, 16px gaps. */
  .web-hub {
    min-height: 100vh;
    padding: 0 clamp(16px, 4vw, 56px) clamp(24px, 3vw, 40px);
    background-color: #f4f6ff;
    background-image: radial-gradient(#dde3fd 1.3px, transparent 1.3px);
    background-size: 24px 24px;
    color: #0f172a;
    font-family: 'Space Grotesk', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 28px;
  }

  /* Header — sticky so nav stays reachable on a long page */
  .hub-header {
    position: sticky;
    top: 0;
    z-index: 40;
    width: 100%;
    max-width: 1200px;
    margin: 0 auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px 16px;
    padding: 12px 0;
    background: #f4f6ff;
    border-bottom: 2px solid #0f172a;
  }

  .brand-group {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .brand-mascot {
    width: 38px;
    height: 38px;
    border: 2px solid #0f172a;
    border-radius: 10px;
    box-shadow: 2px 2px 0 #0f172a;
    background: #7ec8f7;
  }

  .brand {
    color: #0f172a;
    font-size: 24px;
    font-weight: 900;
    text-decoration: none;
    letter-spacing: -0.03em;
  }

  .brand span {
    color: #0e7490;
  }

  .version-tag {
    background: #ddd6fe;
    color: #0f172a;
    border: 2px solid #0f172a;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 700;
    font-family: 'JetBrains Mono', monospace;
  }

  .hub-nav {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .nav-link {
    color: #334155;
    text-decoration: none;
    font-weight: 700;
    font-size: 14px;
    padding: 6px 12px;
    border-radius: 8px;
    border: 2px solid transparent;
    transition: color 0.15s ease, background 0.15s ease, border-color 0.15s ease;
  }

  .nav-link:hover {
    color: #0e7490;
    background: #cffafe;
    border-color: #0f172a;
  }

  /* Hero Section */
  .hero-section {
    width: 100%;
    max-width: 1200px;
    margin: 0 auto;
    padding-top: 8px;
  }

  .hero-badge {
    display: inline-block;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    font-weight: 800;
    letter-spacing: 0.12em;
    color: #0f172a;
    background: #fef08a;
    border: 2px solid #0f172a;
    padding: 4px 10px;
    border-radius: 6px;
    margin-bottom: 14px;
    box-shadow: 2px 2px 0 #0f172a;
  }

  .hero-title {
    margin: 0 0 14px;
    font-size: clamp(40px, 6vw, 76px);
    line-height: 0.98;
    letter-spacing: -0.05em;
    font-weight: 900;
    color: #0f172a;
  }

  .hero-title span {
    color: #0e7490;
  }

  .hero-subtitle {
    max-width: 640px;
    font-size: clamp(15px, 1.8vw, 18px);
    line-height: 1.6;
    color: #334155;
    margin: 0 0 26px;
  }

  /* Hero top: copy + big mascot panel (fills the old empty right side) */
  .hero-top {
    display: flex;
    align-items: center;
    gap: 32px;
  }

  .hero-copy {
    flex: 1;
    min-width: 0;
  }

  .hero-art {
    position: relative;
    flex-shrink: 0;
    width: 224px;
    height: 224px;
  }

  .hero-art::before {
    content: "";
    position: absolute;
    inset: 0;
    background: #ddd6fe;
    border: 2px solid #0f172a;
    border-radius: 24px;
    box-shadow: 6px 6px 0 #0f172a;
    transform: rotate(-4deg);
  }

  .hero-art :global(svg) {
    position: relative;
    width: 208px;
    height: 208px;
    margin: 8px;
    filter: drop-shadow(4px 4px 0 #0f172a);
  }

  .hero-version {
    position: absolute;
    left: -6px;
    bottom: 2px;
    background: #fbbf24;
    color: #0f172a;
    border: 2px solid #0f172a;
    border-radius: 8px;
    padding: 3px 10px;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    font-weight: 800;
    box-shadow: 2px 2px 0 #0f172a;
  }

  /* Latest release download CTA */
  .download-cta {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
  }

  .download-primary {
    display: inline-block;
    padding: 14px 24px;
    background: #a3e635;
    color: #0f172a;
    border: 2px solid #0f172a;
    border-radius: 10px;
    font-weight: 800;
    font-size: 16px;
    text-decoration: none;
    box-shadow: 4px 4px 0 #0f172a;
    transition: transform 0.1s ease, box-shadow 0.1s ease;
  }

  .download-primary:hover {
    transform: translate(-1px, -1px);
    box-shadow: 5px 5px 0 #0f172a;
  }

  .download-primary:active {
    transform: translate(2px, 2px);
    box-shadow: 1px 1px 0 #0f172a;
  }

  .platform-links {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 13px;
    color: #64748b;
  }

  .platform-links a {
    color: #0e7490;
    font-weight: 700;
    text-decoration: none;
  }

  .platform-links a:hover {
    text-decoration: underline;
  }

  .latest-note {
    color: #4d7c0f;
    font-weight: 700;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
  }

  /* Demo + stats */
  .demo-section,
  .stats-section {
    width: 100%;
    max-width: 1200px;
    margin: 0 auto;
  }

  .section-head {
    margin-bottom: 16px;
  }

  .section-eyebrow {
    display: inline-block;
    font-family: 'JetBrains Mono', monospace;
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.14em;
    color: #0e7490;
    margin-bottom: 6px;
  }

  .section-head h2,
  .stats-title {
    margin: 0;
    font-size: clamp(24px, 3.4vw, 32px);
    font-weight: 900;
    letter-spacing: -0.02em;
    color: #0f172a;
  }

  .demo-figure {
    margin: 0;
    max-width: 720px;
  }

  .demo-gif {
    display: block;
    width: 100%;
    height: auto;
    border: 2px solid #0f172a;
    border-radius: 16px;
    box-shadow: 6px 6px 0 #0f172a;
    background: #f4f6ff;
  }

  .demo-caption {
    margin: 14px 0 0;
    text-align: center;
    font-family: 'JetBrains Mono', monospace;
    font-size: 13px;
    font-weight: 700;
    color: #475569;
  }

  .stats-title {
    margin-bottom: 16px;
  }

  .stats-title-num {
    color: #0e7490;
  }

  .stats-row {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 16px;
  }

  .stat-card {
    background: #ffffff;
    border: 2px solid #0f172a;
    border-radius: 12px;
    padding: 18px;
    box-shadow: 4px 4px 0 #0f172a;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stat-num {
    font-size: 44px;
    line-height: 1;
    font-weight: 900;
    letter-spacing: -0.03em;
    color: #0f172a;
  }

  .stat-tools .stat-num { color: #0e7490; }
  .stat-cats .stat-num { color: #4d7c0f; }
  .stat-platforms .stat-num { color: #b45309; }

  .stat-label {
    font-family: 'JetBrains Mono', monospace;
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.1em;
    color: #475569;
    margin-top: 6px;
  }

  .stat-sub {
    font-size: 13px;
    color: #64748b;
  }

  .stat-sites {
    background: #ede9fe;
    justify-content: center;
    gap: 8px;
  }

  .stat-sites .stat-label {
    color: #6d28d9;
    margin-top: 0;
  }

  .stat-sites-list {
    font-size: 15px;
    font-weight: 800;
    line-height: 1.5;
    color: #0f172a;
  }

  /* All versions — direct downloads */
  .versions-section {
    width: 100%;
    max-width: 1200px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .versions-head h2 {
    margin: 0 0 6px;
    font-size: clamp(22px, 3vw, 28px);
    font-weight: 800;
    letter-spacing: -0.02em;
    color: #0f172a;
  }

  .versions-head p {
    margin: 0;
    font-size: 14px;
    line-height: 1.5;
    color: #64748b;
  }

  .versions-status {
    font-size: 14px;
    color: #475569;
    padding: 14px 16px;
    background: #ffffff;
    border: 2px solid #0f172a;
    border-radius: 12px;
    box-shadow: 4px 4px 0 #0f172a;
  }

  .versions-status.error {
    color: #b91c1c;
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .versions-status.error a {
    color: #0e7490;
    font-weight: 700;
    text-decoration: none;
  }

  .version-card {
    background: #ffffff;
    border: 2px solid #0f172a;
    border-radius: 12px;
    padding: 18px 20px;
    box-shadow: 4px 4px 0 #0f172a;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .version-head {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .version-name {
    font-family: 'JetBrains Mono', monospace;
    font-size: 17px;
    font-weight: 800;
    color: #4d7c0f;
  }

  .latest-badge {
    background: #a3e635;
    color: #0f172a;
    border: 2px solid #0f172a;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .prerelease-badge {
    background: #fbbf24;
    color: #0f172a;
    border: 2px solid #0f172a;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .version-date {
    font-size: 13px;
    color: #64748b;
  }

  .no-files {
    margin: 0;
    font-size: 14px;
    color: #64748b;
  }

  .file-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    background: #f8fafc;
    border: 2px solid #e2e8f0;
    border-radius: 8px;
    padding: 10px 12px;
  }

  .file-format {
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    font-weight: 800;
    color: #0e7490;
    border: 1px solid #67e8f9;
    background: #cffafe;
    border-radius: 6px;
    padding: 3px 8px;
    white-space: nowrap;
  }

  .file-meta {
    flex: 1;
    min-width: 200px;
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
    font-size: 13px;
  }

  .file-os {
    font-weight: 800;
    color: #0f172a;
    white-space: nowrap;
  }

  .file-variant {
    color: #64748b;
    white-space: nowrap;
  }

  .file-name {
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    color: #64748b;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 420px;
  }

  .file-size {
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    color: #475569;
    white-space: nowrap;
  }

  .file-download {
    display: inline-block;
    padding: 8px 14px;
    background: #a3e635;
    color: #0f172a;
    border: 2px solid #0f172a;
    border-radius: 8px;
    font-weight: 800;
    font-size: 13px;
    text-decoration: none;
    box-shadow: 2px 2px 0 #0f172a;
    transition: transform 0.1s ease, box-shadow 0.1s ease;
    white-space: nowrap;
  }

  .file-download:hover {
    transform: translate(-1px, -1px);
    box-shadow: 3px 3px 0 #0f172a;
  }

  .file-download:active {
    transform: translate(2px, 2px);
    box-shadow: 1px 1px 0 #0f172a;
  }

  .versions-github {
    align-self: flex-start;
    color: #0e7490;
    font-weight: 700;
    font-size: 14px;
    text-decoration: none;
  }

  .versions-github:hover {
    text-decoration: underline;
  }

  /* Older-versions toggle — same brutalist button treatment as the
     per-file download buttons. */
  .versions-toggle {
    align-self: flex-start;
    padding: 10px 18px;
    background: #cffafe;
    color: #0f172a;
    border: 2px solid #0f172a;
    border-radius: 8px;
    font-weight: 800;
    font-size: 14px;
    font-family: inherit;
    cursor: pointer;
    box-shadow: 2px 2px 0 #0f172a;
    transition: transform 0.1s ease, box-shadow 0.1s ease;
  }

  .versions-toggle:hover {
    transform: translate(-1px, -1px);
    box-shadow: 3px 3px 0 #0f172a;
  }

  .versions-toggle:active {
    transform: translate(2px, 2px);
    box-shadow: 1px 1px 0 #0f172a;
  }

  /* Credits */
  .credits-section {
    width: 100%;
    max-width: 1200px;
    margin: 0 auto;
  }

  .credits-card {
    background: #ffffff;
    border: 2px solid #0f172a;
    border-radius: 14px;
    padding: 22px;
    box-shadow: 4px 4px 0 #0f172a;
  }

  .credits-num {
    color: #b45309;
  }

  .credits-card h2 {
    margin: 0 0 16px;
    font-size: 22px;
    font-weight: 800;
    color: #0f172a;
  }

  .credits-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 14px;
    margin-bottom: 14px;
  }

  .credits-grid > div {
    display: grid;
    gap: 4px;
    color: #0f172a;
  }

  .credits-grid a {
    color: #0e7490;
    text-decoration: none;
  }

  .credits-grid a:hover {
    text-decoration: underline;
  }

  .credit-label {
    color: #64748b;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .credits-note {
    margin: 0;
    font-size: 14px;
    line-height: 1.5;
    color: #475569;
  }

  /* Bento Grid */
  .bento-grid {
    width: 100%;
    max-width: 1200px;
    margin: 0 auto;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(270px, 1fr));
    gap: 16px;
  }

  .bento-card {
    border-radius: 14px;
    padding: 22px;
    border: 2px solid #0f172a;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    text-decoration: none;
    box-shadow: 4px 4px 0 #0f172a;
    transition: transform 0.15s ease, box-shadow 0.15s ease;
  }

  .bento-card:hover {
    transform: translate(-2px, -2px);
    box-shadow: 6px 6px 0 #0f172a;
  }

  .card-num {
    font-family: 'JetBrains Mono', monospace;
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.1em;
    display: block;
    margin-bottom: 8px;
  }

  .bento-card h2 {
    margin: 0 0 4px;
    font-size: 22px;
    font-weight: 800;
    color: #0f172a;
  }

  .card-sub {
    margin: 0 0 12px;
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #0f172a;
    opacity: 0.75;
  }

  .card-text {
    margin: 0 0 20px;
    font-size: 14px;
    line-height: 1.55;
    color: #334155;
  }

  /* Card variants — pastel fills, dark ink text */
  .card-desktop {
    background: #a5f3fc;
  }
  .card-desktop .card-num { color: #0e7490; }
  .card-desktop .action-pill {
    background: #0f172a;
    color: #67e8f9;
    border: 2px solid #0f172a;
  }

  .card-extension {
    background: #d9f99d;
  }
  .card-extension .card-num { color: #4d7c0f; }
  .card-extension .action-pill {
    background: #0f172a;
    color: #a3e635;
    border: 2px solid #0f172a;
  }

  .card-tools {
    background: #ede9fe;
  }
  .card-tools .card-num { color: #6d28d9; }
  .card-tools .badge-pill {
    display: inline-block;
    background: #ffffff;
    color: #6d28d9;
    border: 2px solid #0f172a;
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 700;
    font-family: 'JetBrains Mono', monospace;
    box-shadow: 2px 2px 0 #0f172a;
  }

  .card-support {
    background: #ffedd5;
  }
  .card-support .card-num { color: #b45309; }

  .action-pill {
    display: inline-flex;
    align-items: center;
    padding: 8px 16px;
    border-radius: 8px;
    font-weight: 800;
    font-size: 13px;
    box-shadow: 2px 2px 0 rgba(0, 0, 0, 0.25);
  }

  /* Footer */
  .hub-footer {
    width: 100%;
    max-width: 1200px;
    margin: 12px auto 0;
    padding-top: 20px;
    border-top: 2px solid #cbd5e1;
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 14px;
    font-size: 13px;
    color: #64748b;
  }

  .footer-right {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .footer-right a {
    color: #334155;
    text-decoration: none;
    transition: color 0.15s ease;
  }

  .footer-right a:hover {
    color: #0e7490;
  }

  @media (max-width: 900px) {
    .stats-row {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  @media (max-width: 768px) {
    .hub-header {
      flex-direction: column;
      align-items: flex-start;
    }
    .hero-top {
      flex-direction: column;
      align-items: flex-start;
      gap: 20px;
    }
    .hero-art {
      width: 160px;
      height: 160px;
    }
    .hero-art :global(svg) {
      width: 144px;
      height: 144px;
    }
    .hub-footer {
      flex-direction: column;
      align-items: flex-start;
    }
  }

  @media (max-width: 560px) {
    .stats-row {
      grid-template-columns: 1fr;
    }
    .file-download {
      flex: 1 1 100%;
      text-align: center;
    }
    .file-name {
      max-width: 100%;
    }
    .download-primary {
      width: 100%;
      text-align: center;
    }
  }
</style>
