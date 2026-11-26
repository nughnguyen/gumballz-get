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

<section class="star-section">
  <p class="star-description">{$t('about.star_description')}</p>
  <div class="star-iframe-wrapper">
    <a class="btn" href={GITHUB_URL} target="_blank" rel="noopener" onclick={openGitHub}>View on GitHub</a>
  </div>
</section>

<section class="project-header">
  <BrandMark size={64} decorative={false} />
  <p class="project-description">{$t('about.description')}</p>
</section>

<section class="card">
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

<style>
  .star-section {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--padding);
    width: 100%;
  }

  .star-description {
    font-size: var(--text-base);
    font-weight: 500;
    color: var(--secondary);
  }

  .star-iframe-wrapper {
    display: flex;
    justify-content: center;
  }

  .star-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: calc(var(--padding) / 2);
    margin: 0 auto;
    padding: calc(var(--padding) / 2) calc(var(--padding) * 1.5);
    font-size: var(--text-base);
    font-weight: 500;
    color: var(--text);
    background: var(--surface);
    border: none;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .star-button svg {
    pointer-events: none;
    flex-shrink: 0;
    color: var(--warning);
  }

  @media (hover: hover) {
    .star-button:hover {
      background: var(--surface-hi);
    }
  }

  .star-button:active {
    background: var(--fill-2);
  }

  .star-button:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }

  .project-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: calc(var(--padding) / 2);
    text-align: center;
  }

  .project-logo {
    border-radius: calc(var(--border-radius) + 4px);
    pointer-events: none;
  }

  .project-description {
    font-size: var(--text-base);
    font-weight: 400;
    line-height: 1.8;
    color: var(--text-muted);
    max-width: 320px;
  }

  .card {
    width: 100%;
    background: var(--surface);
    border: 2px solid var(--border-ink);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    border-radius: 12px;
    padding: calc(var(--padding) + 4px);
    display: flex;
    flex-direction: column;
    gap: calc(var(--padding) / 2);
  }

  .card-title {
    color: var(--secondary);
    font-size: var(--text-md);
    font-weight: 600;
  }

  .features-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .feature-item {
    font-size: 13px;
    font-weight: 400;
    line-height: 1.6;
    color: var(--text-muted);
  }

  .feature-item::before {
    content: "• ";
    color: var(--accent-text);
  }

  .version {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-muted);
    text-align: center;
  }
</style>
