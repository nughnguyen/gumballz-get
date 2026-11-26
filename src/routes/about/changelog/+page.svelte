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
  import DOMPurify from "dompurify";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { getCurrentVersion } from "$lib/stores/changelog-store.svelte";
  import { fetchAllReleases } from "$lib/releases/releases";
  import type { ReleaseVersion } from "$lib/releases/types";

  let version = $derived(getCurrentVersion());
  let entries = $state<ReleaseVersion[]>([]);
  let loading = $state(true);
  let showOlder = $state(false);

  /** Only the newest version's changelog is shown at first; older
      versions stay collapsed behind the toggle below it. */
  let newestEntry = $derived(entries[0] ?? null);
  let olderEntries = $derived(entries.slice(1));

  onMount(async () => {
    try {
      entries = await fetchAllReleases();
    } catch {
      entries = [];
    }
    loading = false;
  });

  function renderMarkdown(md: string): string {
    const out: string[] = [];
    let inFence = false;
    let fenceLines: string[] = [];
    for (const line of md.split("\n")) {
      if (line.trim().startsWith("```")) {
        if (inFence) {
          out.push(`<pre><code>${escapeHtml(fenceLines.join("\n"))}</code></pre>`);
          fenceLines = [];
        }
        inFence = !inFence;
        continue;
      }
      if (inFence) {
        fenceLines.push(line);
        continue;
      }
      if (/^\s*(---|\*\*\*|___)\s*$/.test(line)) {
        out.push("<hr />");
      } else if (line.startsWith("### ")) {
        out.push(`<h4>${escapeHtml(line.slice(4))}</h4>`);
      } else if (line.startsWith("## ")) {
        out.push(`<h3>${escapeHtml(line.slice(3))}</h3>`);
      } else if (line.startsWith("# ")) {
        out.push(`<h2>${escapeHtml(line.slice(2))}</h2>`);
      } else if (line.startsWith("- ") || line.startsWith("* ")) {
        out.push(`<li>${formatInline(line.slice(2))}</li>`);
      } else if (line.trim() === "") {
        out.push("<br />");
      } else {
        out.push(`<p>${formatInline(line)}</p>`);
      }
    }
    if (inFence && fenceLines.length) {
      out.push(`<pre><code>${escapeHtml(fenceLines.join("\n"))}</code></pre>`);
    }
    const html = out.join("");
    return typeof window === "undefined"
      ? html
      : DOMPurify.sanitize(html, { ALLOWED_TAGS: ["a", "br", "code", "h2", "h3", "h4", "hr", "li", "pre", "p", "strong"], ALLOWED_ATTR: ["href", "target", "rel"] });
  }

  function escapeHtml(str: string): string {
    return str
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function formatInline(str: string): string {
    let result = escapeHtml(str);
    result = result.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
    result = result.replace(/`(.+?)`/g, "<code>$1</code>");
    result = result.replace(
      /\[(.+?)\]\((.+?)\)/g,
      (match, label: string, href: string) => {
        try {
          if (new URL(href.replace(/&amp;/g, "&")).protocol !== "https:") return label;
          return `<a href="${href}" target="_blank" rel="noopener">${label}</a>`;
        } catch {
          return label;
        }
      }
    );
    return result;
  }
</script>

<div class="changelog-page">
  {#if version}
    <div class="version-row">
      <span class="version-label">{$t("about.version")}</span>
      <span class="version-value">{version}</span>
    </div>
  {/if}

  {#if loading}
    <div class="loading">
      <span class="spinner"></span>
    </div>
  {:else if entries.length === 0}
    <div class="card empty-card">
      <p class="empty-text">{$t("changelog.empty")}</p>
    </div>
  {:else}
    {#if newestEntry}
      <div class="card">
        <div class="entry-head">
          <span class="entry-version">{newestEntry.version}</span>
          {#if newestEntry.publishedAt}
            <span class="entry-date">{newestEntry.publishedAt.slice(0, 10)}</span>
          {/if}
        </div>
        <div class="markdown-content">
          {@html renderMarkdown(newestEntry.notes)}
        </div>
      </div>
    {/if}
    {#if olderEntries.length > 0}
      <button
        type="button"
        class="older-toggle"
        aria-expanded={showOlder}
        onclick={() => (showOlder = !showOlder)}
      >
        {showOlder ? $t("web.versions_hide_older") : $t("web.versions_show_older")}
      </button>
    {/if}
    {#if showOlder}
      {#each olderEntries as entry (entry.version)}
        <div class="card">
          <div class="entry-head">
            <span class="entry-version">{entry.version}</span>
            {#if entry.publishedAt}
              <span class="entry-date">{entry.publishedAt.slice(0, 10)}</span>
            {/if}
          </div>
          <div class="markdown-content">
            {@html renderMarkdown(entry.notes)}
          </div>
        </div>
      {/each}
    {/if}
  {/if}
</div>

<style>
  .changelog-page {
    display: flex;
    flex-direction: column;
    gap: calc(var(--padding) * 1.5);
  }

  .version-row {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: calc(var(--padding) / 2);
  }

  .version-label {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-muted);
  }

  .version-value {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--accent-text);
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: calc(var(--padding) * 3) 0;
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--input-border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .card {
    width: 100%;
    background: var(--surface);
    border: 2px solid var(--border-ink);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    border-radius: 12px;
    padding: calc(var(--padding) + 4px);
  }

  .empty-card {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: calc(var(--padding) * 2);
  }

  .empty-text {
    font-size: var(--text-base);
    font-weight: 500;
    color: var(--text-muted);
  }

  .entry-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: calc(var(--padding) / 2);
  }

  .entry-version {
    font-size: var(--text-md);
    font-weight: 600;
    color: var(--accent-text);
  }

  .entry-date {
    font-size: var(--text-sm);
    color: var(--text-dim);
  }

  .older-toggle {
    align-self: flex-start;
    padding: 10px 16px;
    background: var(--surface-hi);
    color: var(--accent-text);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    font-size: var(--text-base);
    font-weight: 600;
    cursor: pointer;
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  @media (hover: hover) {
    .older-toggle:hover {
      box-shadow: 4px 4px 0 var(--shadow-hard);
    }
  }

  .older-toggle:active {
    transform: translate(2px, 2px);
    box-shadow: none;
  }

  .markdown-content {
    font-size: var(--text-base);
    font-weight: 400;
    line-height: 1.7;
    color: var(--secondary);
  }

  .markdown-content :global(h2) {
    font-size: var(--text-lg);
    font-weight: 600;
    margin: 0 0 calc(var(--padding) / 2);
    letter-spacing: var(--track-snug);
  }

  .markdown-content :global(h3) {
    font-size: var(--text-md);
    font-weight: 600;
    margin: var(--padding) 0 calc(var(--padding) / 2);
  }

  .markdown-content :global(h4) {
    font-size: var(--text-base);
    font-weight: 600;
    margin: var(--padding) 0 calc(var(--padding) / 4);
    color: var(--text-muted);
  }

  .markdown-content :global(p) {
    margin: 0 0 4px;
  }

  .markdown-content :global(li) {
    margin: 0 0 4px;
    padding-left: calc(var(--padding) / 2);
    list-style: none;
  }

  .markdown-content :global(li::before) {
    content: "•";
    color: var(--accent-text);
    margin-right: 6px;
  }

  .markdown-content :global(strong) {
    font-weight: 600;
  }

  .markdown-content :global(pre) {
    background: var(--fill-1);
    border-radius: var(--radius-sm);
    padding: var(--space-3);
    overflow-x: auto;
    margin: var(--space-2) 0;
  }

  .markdown-content :global(pre code) {
    background: transparent;
    padding: 0;
  }

  .markdown-content :global(hr) {
    border: none;
    height: 1px;
    background: var(--border);
    margin: var(--space-4) 0;
  }

  .markdown-content :global(code) {
    font-size: 12px;
    padding: 1px 5px;
    background: var(--button-elevated);
    border-radius: 4px;
  }

  .markdown-content :global(a) {
    color: var(--accent-text);
    text-decoration: none;
  }

  @media (hover: hover) {
    .markdown-content :global(a:hover) {
      text-decoration: underline;
    }
  }

  .markdown-content :global(br) {
    display: block;
    content: "";
    margin-top: 4px;
  }
</style>
