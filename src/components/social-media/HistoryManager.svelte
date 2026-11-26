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
  import { onMount } from 'svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import {
    getRecords,
    getFilteredRecords,
    isLoading,
    isReconciling,
    getPlatformFilter,
    setPlatformFilter,
    getSearchQuery,
    setSearchQuery,
    getReconcileStats,
    clearReconcileStats,
    getError,
    clearError,
    loadHistory,
    reconcileDisk,
    deleteRecord,
    openFile,
    openFolder,
    formatFileSize,
  } from '$lib/stores/social_media_history_store.svelte';
  import { showToast } from '$lib/stores/toast-store.svelte';
  import { t } from '$lib/i18n';
  import type { SocialMediaRecord } from '$lib/api/social-media';

  interface Props {
    onSwitchToDownload?: () => void;
  }

  let { onSwitchToDownload }: Props = $props();

  const platforms = [
    { key: 'all', label: 'All' },
    { key: 'instagram', label: 'Instagram' },
    { key: 'tiktok', label: 'TikTok' },
    { key: 'facebook', label: 'Facebook' },
    { key: 'x', label: 'X' },
  ];

  const BATCH_SIZE = 30;
  let visibleCount = $state(BATCH_SIZE);
  let filteredRecords = $derived(getFilteredRecords());
  let visibleRecords = $derived(filteredRecords.slice(0, visibleCount));
  let sentinelEl = $state<HTMLElement | null>(null);
  let failedImages = $state<Record<string, boolean>>({});

  $effect(() => {
    getPlatformFilter();
    getSearchQuery();
    visibleCount = BATCH_SIZE;
  });

  $effect(() => {
    if (!sentinelEl || visibleCount >= filteredRecords.length) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) {
          visibleCount = Math.min(filteredRecords.length, visibleCount + BATCH_SIZE);
        }
      },
      { rootMargin: '500px' },
    );

    observer.observe(sentinelEl);
    return () => observer.disconnect();
  });

  onMount(() => {
    loadHistory();
  });

  function formatDate(unixSeconds: number): string {
    if (!unixSeconds) return '';
    const d = new Date(unixSeconds * 1000);
    return d.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
    });
  }

  function isImageFile(path: string): boolean {
    return /\.(jpe?g|png|webp|gif|bmp|avif)$/i.test(path);
  }

  async function handleDelete(record: SocialMediaRecord) {
    try {
      await deleteRecord(record.id);
      showToast('info', 'Record removed from history');
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      showToast('error', `Failed to remove: ${msg}`);
    }
  }

  async function handleReconcile() {
    try {
      await reconcileDisk();
      const stats = getReconcileStats();
      if (stats) {
        showToast('success', `Reconcile complete: checked ${stats.checked}, cleaned ${stats.removed} missing files`);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      showToast('error', `Reconcile failed: ${msg}`);
    }
  }
</script>

<div class="hm-page">
  <!-- Toolbar: Platform filter + Reconcile Disk -->
  <div class="hm-toolbar">
    <div class="segmented segmented-lg" role="tablist">
      {#each platforms as platform (platform.key)}
        <button
          type="button"
          role="tab"
          aria-selected={getPlatformFilter() === platform.key}
          class="segmented-btn"
          class:active={getPlatformFilter() === platform.key}
          onclick={() => setPlatformFilter(platform.key)}
        >
          {platform.label}
        </button>
      {/each}
    </div>

    <button
      type="button"
      class="btn btn-sm btn-secondary"
      onclick={handleReconcile}
      disabled={isReconciling() || isLoading()}
      title="Verify media existence on disk and prune missing items"
    >
      <svg
        viewBox="0 0 16 16"
        width="13"
        height="13"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        class:spin={isReconciling()}
      >
        <path d="M1.5 8a6.5 6.5 0 0 1 11.1-4.6L14 5" />
        <path d="M14 2v3h-3" />
        <path d="M14.5 8a6.5 6.5 0 0 1-11.1 4.6L2 11" />
        <path d="M2 14v-3h3" />
      </svg>
      <span>{isReconciling() ? 'Reconciling...' : 'Reconcile Disk'}</span>
    </button>
  </div>

  <!-- Search Input -->
  <div class="hm-search-row">
    <input
      type="search"
      class="input input-lg input-search"
      value={getSearchQuery()}
      oninput={(e) => setSearchQuery((e.target as HTMLInputElement).value)}
      placeholder="Search history by author, filename, or post ID..."
      autocomplete="off"
      spellcheck="false"
    />
  </div>

  {#if getError()}
    <div class="notice notice-danger" role="alert">
      <span>{getError()}</span>
      <button type="button" class="btn btn-sm btn-ghost" onclick={clearError}>×</button>
    </div>
  {/if}

  {#if getReconcileStats()}
    <div class="notice notice-info" role="status">
      <span>Checked {getReconcileStats()?.checked ?? 0} items, removed {getReconcileStats()?.removed ?? 0} missing files.</span>
      <button type="button" class="btn btn-sm btn-ghost" onclick={clearReconcileStats}>×</button>
    </div>
  {/if}

  <!-- History Items -->
  {#if isLoading()}
    <div class="empty-state">
      <div class="spinner"></div>
      <p class="empty-state-hint">Loading download ledger...</p>
    </div>
  {:else if getFilteredRecords().length === 0}
    <div class="empty-state surface-card">
      <p class="empty-state-title">No download history found</p>
      <p class="empty-state-hint">Files archived from Instagram, TikTok, Facebook, and X will appear here automatically.</p>
      {#if onSwitchToDownload}
        <button type="button" class="btn btn-primary" onclick={onSwitchToDownload}>
          Start Archiving
        </button>
      {/if}
    </div>
  {:else}
    <div class="surface-card">
      {#each visibleRecords as record (record.id)}
        <div
          class="list-row interactive"
          role="button"
          tabindex="0"
          onclick={() => openFile(record.file_path)}
          onkeydown={(e) => {
            if (e.key === 'Enter') openFile(record.file_path);
          }}
        >
          <div class="hm-media-icon" data-platform={record.platform}>
            {#if isImageFile(record.file_path) && !failedImages[record.id]}
              <img
                src={convertFileSrc(record.file_path)}
                alt={record.author || record.post_id}
                class="hm-media-thumb"
                loading="lazy"
                onerror={() => {
                  failedImages[record.id] = true;
                }}
              />
            {:else if record.file_path.endsWith('.mp4') || record.file_path.endsWith('.mov')}
              <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor">
                <polygon points="5 3 19 12 5 21 5 3" />
              </svg>
            {:else}
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
                <circle cx="8.5" cy="8.5" r="1.5" />
                <polyline points="21 15 16 10 5 21" />
              </svg>
            {/if}
          </div>

          <span class="list-row-content">
            <span class="list-row-title">
              {record.author ? `@${record.author}` : `Post ${record.post_id}`}
            </span>
            <span class="list-row-sub">
              {record.platform.toUpperCase()} • {formatDate(record.downloaded_at)} • {formatFileSize(record.file_size)}
            </span>
          </span>

          <span class="list-row-trailing">
            <button
              type="button"
              class="btn btn-sm btn-secondary"
              onclick={(e) => {
                e.stopPropagation();
                openFile(record.file_path);
              }}
              title="Open Media File"
            >
              Open
            </button>

            <button
              type="button"
              class="btn btn-sm btn-secondary"
              onclick={(e) => {
                e.stopPropagation();
                openFolder(record.file_path);
              }}
              title="Open Containing Folder"
            >
              Folder
            </button>

            <button
              type="button"
              class="btn btn-sm btn-ghost"
              title="Delete ledger record"
              onclick={(e) => {
                e.stopPropagation();
                handleDelete(record);
              }}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
              </svg>
            </button>
          </span>
        </div>
      {/each}

      {#if visibleCount < filteredRecords.length}
        <div bind:this={sentinelEl} class="hm-sentinel" aria-hidden="true"></div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .hm-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    width: 100%;
  }

  .hm-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
  }

  .hm-search-row {
    width: 100%;
  }

  .hm-search-row .input {
    width: 100%;
  }

  .hm-media-icon {
    position: relative;
    width: 38px;
    height: 38px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--fill-2);
    color: var(--text-muted);
    flex-shrink: 0;
    overflow: hidden;
  }

  .hm-media-thumb {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .hm-media-icon[data-platform="instagram"] {
    color: #e1306c;
  }

  .hm-media-icon[data-platform="tiktok"] {
    color: var(--accent-text);
  }

  .hm-media-icon[data-platform="x"] {
    color: var(--text);
  }

  .hm-media-icon[data-platform="facebook"] {
    color: #1877f2;
  }

  .spin {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .list-row {
    content-visibility: auto;
    contain-intrinsic-size: 0 54px;
    contain: layout paint style;
  }

  .hm-sentinel {
    height: 1px;
    width: 100%;
    pointer-events: none;
    opacity: 0;
  }
</style>
