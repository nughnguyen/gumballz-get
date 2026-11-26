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

  // Three download modes for a video opened inside a list: the clip
  // itself, the whole playlist, or the whole auto-mix. Only the list
  // button that matches this link is shown (playlist for PL lists,
  // mix for RD mixes); the clip button is always available.
  let {
    choice = $bindable("clip"),
    isMix = false,
    onChoose,
  }: {
    choice?: "clip" | "playlist" | "mix";
    isMix?: boolean;
    onChoose?: (next: "clip" | "playlist" | "mix") => void;
  } = $props();

  function select(next: "clip" | "playlist" | "mix") {
    choice = next;
    onChoose?.(next);
  }
</script>

<fieldset class="video-or-list">
  <legend>{$t(isMix ? "downloadbox.video_or_list_mix" : "downloadbox.video_or_list")}</legend>
  <div class="options">
    <button
      type="button"
      aria-pressed={choice === "clip"}
      class:active={choice === "clip"}
      onclick={() => select("clip")}
    >
      {$t("downloadbox.video_or_list_video")}
    </button>
    {#if !isMix}
      <button
        type="button"
        aria-pressed={choice === "playlist"}
        class:active={choice === "playlist"}
        onclick={() => select("playlist")}
      >
        {$t("downloadbox.video_or_list_list")}
      </button>
    {:else}
      <button
        type="button"
        aria-pressed={choice === "mix"}
        class:active={choice === "mix"}
        onclick={() => select("mix")}
      >
        {$t("downloadbox.video_or_list_mix_list")}
      </button>
    {/if}
  </div>
</fieldset>

<style>
  .video-or-list {
    border: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2, 8px);
  }

  legend {
    padding: 0;
    margin-block-end: var(--space-2, 8px);
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }

  .options {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2, 8px);
  }

  button {
    flex: 1 1 140px;
    min-height: 34px;
    padding: 0 var(--space-3, 12px);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md, 8px);
    background: var(--surface-hi);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: background var(--duration-fast, 120ms) ease, color var(--duration-fast, 120ms) ease, box-shadow var(--duration-fast, 120ms) ease, transform var(--duration-fast, 120ms) ease;
  }

  button:hover {
    background: var(--surface-hi);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  button:active {
    transform: translate(2px, 2px);
    box-shadow: none;
  }

  button.active {
    background: var(--accent);
    border: 2px solid var(--border-ink);
    box-shadow: 2px 2px 0 var(--shadow-hard);
    color: var(--on-accent, #090D16);
    font-weight: 600;
  }

  button:focus-visible {
    outline: var(--focus-ring, 2px solid var(--accent));
    outline-offset: var(--focus-ring-offset, 2px);
  }
</style>
