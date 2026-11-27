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
  import { t } from "$lib/i18n";
  import type { PreviewQuality } from "$lib/stores/media-preview-store.svelte";

  let {
    qualities,
    onDownload,
  }: {
    qualities: PreviewQuality[];
    /** label of the quality to download, or null for all photos. */
    onDownload: (label: string | null) => void;
  } = $props();

  const IMAGE_FORMATS = ["jpg", "jpeg", "png", "webp", "gif"];

  let photos = $derived(qualities.filter((q) => IMAGE_FORMATS.includes(q.format)));
  let videoOptions = $derived(
    qualities.filter(
      (q) => q.format === "tiktok_slideshow" || q.format.startsWith("tiktok_direct"),
    ),
  );
  let audioOptions = $derived(qualities.filter((q) => q.format === "mp3"));

  function optionLabel(q: PreviewQuality): string {
    switch (q.label) {
      case "slideshow":
        return $t("downloadbox.slideshow_video");
      case "slideshow_wm":
        return $t("downloadbox.slideshow_video_watermark");
      case "video_wm":
        return $t("downloadbox.video_with_watermark");
      case "video_no_wm":
        return $t("downloadbox.video_no_watermark");
      case "audio":
        return $t("downloadbox.audio_track");
      default:
        return q.label;
    }
  }
</script>

{#if photos.length > 0 || videoOptions.length > 0 || audioOptions.length > 0}
  <div class="photos-panel">
    {#if photos.length > 0}
      <div class="photos-head">
        <span class="photos-title"
          >{$t("downloadbox.photos_title", { count: photos.length })}</span
        >
        <button type="button" class="photos-all-btn" onclick={() => onDownload(null)}>
          {$t("downloadbox.photos_download_all")}
        </button>
      </div>
      <div class="photos-list">
        {#each photos as photo, i (photo.label)}
          <div class="photo-row">
            <img
              class="photo-thumb"
              src={photo.url}
              alt={$t("downloadbox.photo_label", { n: i + 1 })}
              loading="lazy"
              draggable="false"
            />
            <span class="photo-name">
              {$t("downloadbox.photo_label", { n: i + 1 })}
              {#if photo.width && photo.height}
                <span class="photo-dims"> · {photo.width}×{photo.height}</span>
              {/if}
            </span>
            <button
              type="button"
              class="button photo-dl-btn"
              onclick={() => onDownload(photo.label)}
            >
              {$t("downloadbox.photo_download")}
            </button>
          </div>
        {/each}
      </div>
    {/if}

    {#if videoOptions.length > 0 || audioOptions.length > 0}
      <div class="photos-options">
        {#each videoOptions as opt (opt.label)}
          <div class="photo-row">
            <span class="photo-name">{optionLabel(opt)}</span>
            <button
              type="button"
              class="button photo-dl-btn"
              onclick={() => onDownload(opt.label)}
            >
              {$t("downloadbox.photo_download")}
            </button>
          </div>
        {/each}
        {#each audioOptions as opt (opt.label)}
          <div class="photo-row">
            <span class="photo-name">{optionLabel(opt)}</span>
            <button
              type="button"
              class="button photo-dl-btn"
              onclick={() => onDownload(opt.label)}
            >
              {$t("downloadbox.photo_download")}
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .photos-panel {
    display: flex;
    flex-direction: column;
    gap: calc(var(--padding) / 2);
    width: 100%;
    background: var(--surface);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    padding: var(--padding);
    max-height: 400px;
    overflow: hidden;
  }

  .photos-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--padding);
  }

  .photos-title {
    font-size: var(--text-sm);
    font-weight: 700;
    color: var(--text);
  }

  .photos-all-btn {
    padding: 6px 14px;
    font-size: 13px;
    font-weight: 600;
    color: var(--on-accent);
    background: var(--accent);
    border: 2px solid var(--border-ink);
    border-radius: 999px;
    cursor: pointer;
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  .photos-all-btn:active {
    transform: translate(2px, 2px);
    box-shadow: none;
  }

  .photos-list {
    max-height: 240px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    box-shadow: var(--button-box-shadow);
    scrollbar-width: none;
  }

  .photos-list::-webkit-scrollbar {
    display: none;
  }

  .photos-options {
    display: flex;
    flex-direction: column;
    gap: calc(var(--padding) / 2);
  }

  .photo-row {
    display: flex;
    align-items: center;
    gap: calc(var(--padding) / 2);
    padding: calc(var(--padding) / 2);
  }

  .photo-thumb {
    width: 56px;
    height: 56px;
    object-fit: cover;
    border: 2px solid var(--border-ink);
    border-radius: calc(var(--border-radius) - 2px);
    flex-shrink: 0;
    background: var(--surface-hi);
  }

  .photo-name {
    flex: 1;
    min-width: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .photo-dims {
    font-weight: 500;
    color: var(--text-dim);
  }

  .photo-dl-btn {
    font-size: var(--text-sm);
    padding: 4px 12px;
    flex-shrink: 0;
  }
</style>
