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
  import { open } from "@tauri-apps/plugin-shell";
  import { getVersion } from "@tauri-apps/api/app";
  import { t } from "$lib/i18n";
  import BrandMark from "$lib/ui/BrandMark.svelte";

  const GITHUB_URL = "https://github.com/nughnguyen/gumballz-get";
  let appVersion = $state("");

  $effect(() => {
    getVersion().then((v) => { appVersion = v; }).catch(() => {});
  });

  async function openGitHub() {
    await open(GITHUB_URL);
  }
</script>

<div class="project-bento">
  <section class="bento-panel project-hero tone-cyan">
    <BrandMark size={64} decorative={false} />
    <p class="project-description">{$t('about.description')}</p>
  </section>

  <section class="bento-panel star-panel tone-amber">
    <p class="star-description">{$t('about.star_description')}</p>
    <div class="star-iframe-wrapper">
      <a class="btn" href={GITHUB_URL} target="_blank" rel="noopener" onclick={openGitHub}>View on GitHub</a>
    </div>
  </section>

  <section class="bento-panel card tone-lime">
    <h5 class="card-title">{$t('about.features_title')}</h5>
    <div class="features-list">
      <p class="feature-item">{$t('about.feature_platforms')}</p>
      <p class="feature-item">{$t('about.feature_hotmart')}</p>
      <p class="feature-item">{$t('about.feature_progress')}</p>
      <p class="feature-item">{$t('about.feature_mascot')}</p>
      <p class="feature-item">{$t('about.feature_themes')}</p>
      <p class="feature-item">{$t('about.feature_i18n')}</p>
      <p class="feature-item">{$t('about.feature_tech')}</p>
    </div>
  </section>

  {#if appVersion}
    <p class="version">{$t('about.version')} {appVersion}</p>
  {/if}
</div>

<style>
  .project-bento {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
  }

  .bento-panel {
    background: var(--surface);
    border: 2px solid var(--border-ink);
    border-radius: 12px;
    box-shadow: 4px 4px 0 var(--shadow-hard);
    position: relative;
    overflow: hidden;
  }

  /* One accent per panel, as a top bar (decorative only). */
  .tone-cyan::before,
  .tone-lime::before,
  .tone-amber::before {
    content: "";
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 4px;
  }
  .tone-cyan::before { background: var(--accent); }
  .tone-lime::before { background: #A3E635; }
  .tone-amber::before { background: #FBBF24; }

  .project-hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    text-align: center;
    padding: calc(var(--space-5) + 4px) var(--space-5) var(--space-5);
  }

  .project-description {
    font-size: var(--text-base);
    font-weight: 400;
    line-height: 1.8;
    color: var(--text-muted);
    max-width: 52ch;
    margin: 0;
  }

  .star-panel {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    padding: calc(var(--space-4) + 4px) var(--space-5) var(--space-4);
    text-align: center;
  }

  .star-description {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--text);
    margin: 0;
  }

  .star-iframe-wrapper {
    display: flex;
    justify-content: center;
  }

  .card {
    padding: calc(var(--space-5) + 4px) var(--space-5) var(--space-5);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .card-title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text);
    font-family: var(--font-display);
    font-size: var(--text-md);
    font-weight: 700;
    letter-spacing: var(--track-tight);
    margin: 0;
  }

  .card-title::before {
    content: "";
    flex-shrink: 0;
    width: 10px;
    height: 10px;
    background: #A3E635;
    border: 2px solid var(--border-ink);
    border-radius: 2px;
  }

  .features-list {
    display: flex;
    flex-direction: column;
  }

  .feature-item {
    position: relative;
    font-size: 13px;
    font-weight: 400;
    line-height: 1.65;
    color: var(--text-muted);
    padding: 10px 0 10px 18px;
    margin: 0;
  }

  .feature-item::before {
    content: "";
    position: absolute;
    left: 0;
    top: 17px;
    width: 8px;
    height: 8px;
    background: var(--accent);
    border-radius: 2px;
  }

  .feature-item + .feature-item {
    border-top: var(--hairline) solid var(--separator);
  }

  .version {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: 400;
    color: var(--text-dim);
    text-align: center;
    margin: 0;
  }
</style>
