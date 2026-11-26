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
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    getOutputDir,
    getCookiesFile,
    getBrowserCookies,
    loadSettings,
    saveSettings,
  } from '$lib/stores/social_media_settings_store.svelte';
  import { detectBrowsers, extractBrowserCookies, type DetectedBrowser } from '$lib/api/social-media';
  import { showToast } from '$lib/stores/toast-store.svelte';
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';

  let outputDir = $state('');
  let cookiesFile = $state('');
  let browserCookies = $state('auto');
  let detectedBrowsers = $state<DetectedBrowser[]>([]);
  let saving = $state(false);
  let testingBridge = $state(false);

  let isDirty = $derived(
    outputDir.trim() !== getOutputDir().trim() ||
    cookiesFile.trim() !== getCookiesFile().trim() ||
    browserCookies.trim() !== getBrowserCookies().trim()
  );

  onMount(async () => {
    await loadSettings();
    outputDir = getOutputDir();
    cookiesFile = getCookiesFile();
    browserCookies = getBrowserCookies();

    try {
      detectedBrowsers = await detectBrowsers();
    } catch (err) {
      console.warn('Failed to detect browsers:', err);
    }
  });

  async function browseOutputDir() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Social Media Output Directory',
      });
      if (selected && typeof selected === 'string') {
        outputDir = selected;
      }
    } catch (err) {
      console.error('Failed to open directory dialog:', err);
    }
  }

  async function browseCookiesFile() {
    try {
      const selected = await open({
        directory: false,
        multiple: false,
        title: 'Select Netscape Cookies File',
        filters: [
          { name: 'Netscape Cookies (*.txt, *.cookies)', extensions: ['txt', 'cookies'] },
          { name: 'All Files (*.*)', extensions: ['*'] },
        ],
      });
      if (selected && typeof selected === 'string') {
        cookiesFile = selected;
      }
    } catch (err) {
      console.error('Failed to open file dialog:', err);
    }
  }

  function clearCookies() {
    cookiesFile = '';
  }

  async function handleTestBridge() {
    testingBridge = true;
    try {
      const result = await extractBrowserCookies(browserCookies, 'instagram');
      showToast('success', result || 'Browser bridge connected successfully');
    } catch (err: unknown) {
      const msg = typeof err === 'string' ? err : err instanceof Error ? err.message : 'Bridge test failed';
      showToast('error', msg);
    } finally {
      testingBridge = false;
    }
  }

  async function handleSave() {
    saving = true;
    try {
      await saveSettings(outputDir, cookiesFile, browserCookies);
      outputDir = getOutputDir();
      cookiesFile = getCookiesFile();
      browserCookies = getBrowserCookies();
      showToast('success', 'Social Media settings saved successfully');
    } catch (error) {
      console.error('Failed to save settings:', error);
      showToast('error', 'Failed to save settings');
    } finally {
      saving = false;
    }
  }
</script>

<div class="sm-page">
  <div class="section-header">
    <span class="section-header-title">STORAGE & DESTINATION</span>
  </div>

  <div class="surface-card sm-group">
    <div class="sm-row">
      <div class="sm-label-col">
        <span class="sm-title">Output Directory</span>
        <span class="sm-desc">Destination directory for all saved media and archives.</span>
      </div>
      <div class="sm-action-col">
        <span class="sm-path-text" title={outputDir || 'Not configured'}>
          {outputDir || 'Not configured'}
        </span>
        <button type="button" class="btn btn-secondary" onclick={browseOutputDir}>
          Browse...
        </button>
      </div>
    </div>
  </div>

  <div class="section-header" style="margin-top: var(--space-4);">
    <span class="section-header-title">AUTOMATIC BROWSER COOKIE BRIDGE</span>
  </div>

  <div class="surface-card sm-group">
    <div class="sm-row">
      <div class="sm-label-col">
        <span class="sm-title">Browser Session Bridge</span>
        <span class="sm-desc">Automatically reads session credentials to download private posts, stories, and highlights without manual file exports.</span>
      </div>
      <div class="sm-action-col">
        <select bind:value={browserCookies} class="sm-select">
          <option value="auto">System Default (Automatic)</option>
          {#each detectedBrowsers as browser (browser.id)}
            <option value={browser.id}>
              {browser.name} {browser.is_default ? '(Default)' : ''}
            </option>
          {/each}
          <option value="none">Disabled (No Browser Cookies)</option>
        </select>

        <button
          type="button"
          class="btn btn-secondary"
          onclick={handleTestBridge}
          disabled={testingBridge || browserCookies === 'none'}
          title="Verify session cookies can be reached"
        >
          {testingBridge ? 'Testing...' : 'Test Bridge'}
        </button>
      </div>
    </div>

    <div class="sm-row">
      <div class="sm-label-col">
        <span class="sm-title">Manual Cookie Jar (Optional)</span>
        <span class="sm-desc">Use a Netscape cookies.txt file instead of the automatic browser bridge.</span>
      </div>
      <div class="sm-action-col">
        {#if cookiesFile}
          <span class="sm-path-text" title={cookiesFile}>{cookiesFile}</span>
          <button type="button" class="btn btn-ghost" onclick={clearCookies}>Clear</button>
        {/if}
        <button type="button" class="btn btn-secondary" onclick={browseCookiesFile}>
          {cookiesFile ? 'Change...' : 'Select File...'}
        </button>
      </div>
    </div>
  </div>

  <!-- Save Button -->
  <div class="sm-save-row">
    <button
      type="button"
      class="btn btn-lg btn-primary"
      onclick={handleSave}
      disabled={saving || !isDirty}
    >
      {saving ? 'Saving...' : 'Save Settings'}
    </button>
  </div>
</div>

<style>
  .sm-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: 100%;
  }

  .sm-group {
    display: flex;
    flex-direction: column;
  }

  .sm-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4);
    border-bottom: var(--hairline) solid var(--separator);
  }

  .sm-row:last-child {
    border-bottom: none;
  }

  .sm-label-col {
    display: flex;
    flex-direction: column;
    gap: 3px;
    flex: 1;
    min-width: 0;
  }

  .sm-title {
    font-size: var(--text-base);
    font-weight: 500;
    color: var(--text);
  }

  .sm-desc {
    font-size: var(--text-sm);
    color: var(--text-dim);
    line-height: 1.4;
  }

  .sm-action-col {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-shrink: 0;
  }

  .sm-path-text {
    max-width: 200px;
    font-size: var(--text-sm);
    color: var(--text-muted);
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sm-select {
    min-width: 190px;
  }

  .sm-save-row {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--space-4);
  }
</style>
