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

  type Account = {
    slug: string;
    alias: string;
    captured_at_ms: number;
    cookie_count: number;
    last_used_at_ms: number | null;
  };

  let {
    accounts = [] as Account[],
    selectedSlug = $bindable<string | null>(null),
    onChange = undefined as ((slug: string) => void) | undefined,
  } = $props();

  let open = $state(false);

  let selectedLabel = $derived.by(() => {
    if (!accounts.length) return null;
    const slug = selectedSlug ?? accounts[0]?.slug;
    const acc = accounts.find(a => a.slug === slug);
    return acc?.alias ?? slug;
  });

  function pick(slug: string) {
    selectedSlug = slug;
    open = false;
    onChange?.(slug);
  }
</script>

{#if accounts.length > 1}
  <div class="cookie-picker">
    <button
      type="button"
      class="cookie-picker-trigger"
      onclick={() => { open = !open; }}
      aria-haspopup="listbox"
      aria-expanded={open}
    >
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <circle cx="12" cy="12" r="10" />
        <circle cx="9" cy="10" r="1.5" />
        <circle cx="15" cy="10" r="1.5" />
        <circle cx="12" cy="15" r="1.5" />
      </svg>
      <span class="cookie-picker-label">{$t('downloadbox.cookie_account')}:</span>
      <span class="cookie-picker-value">{selectedLabel}</span>
      <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <polyline points="6 9 12 15 18 9" />
      </svg>
    </button>

    {#if open}
      <ul class="cookie-picker-menu" role="listbox">
        {#each accounts as acc (acc.slug)}
          <li>
            <button
              type="button"
              class="cookie-picker-option"
              class:active={(selectedSlug ?? accounts[0]?.slug) === acc.slug}
              role="option"
              aria-selected={(selectedSlug ?? accounts[0]?.slug) === acc.slug}
              onclick={() => pick(acc.slug)}
            >
              <span class="cookie-picker-option-alias">{acc.alias}</span>
              <span class="cookie-picker-option-meta">
                {acc.cookie_count} {$t('downloadbox.cookie_account_cookies')}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .cookie-picker {
    position: relative;
    display: inline-flex;
  }

  .cookie-picker-trigger {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text);
    background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    border-radius: calc(var(--border-radius) - 2px);
    cursor: pointer;
    transition: box-shadow var(--duration-fast) var(--ease-out);
  }

  .cookie-picker-trigger:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }

  @media (hover: hover) {
    .cookie-picker-trigger:hover {
      background: var(--surface-hi);
      color: var(--text);
      box-shadow: 2px 2px 0 var(--shadow-hard);
    }
  }

  .cookie-picker-label {
    color: var(--text-dim);
  }

  .cookie-picker-value {
    color: var(--text);
    font-weight: 600;
  }

  .cookie-picker-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 30;
    min-width: 220px;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--popup-bg);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    box-shadow: 4px 4px 0 var(--shadow-hard);
  }

  .cookie-picker-option {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    width: 100%;
    padding: 8px 10px;
    font-size: var(--text-sm);
    color: var(--text);
    background: transparent;
    border: none;
    border-radius: calc(var(--border-radius) - 4px);
    text-align: left;
    cursor: pointer;
  }

  @media (hover: hover) {
    .cookie-picker-option:hover {
      background: var(--surface-hi);
    }
  }

  .cookie-picker-option.active {
    background: var(--accent);
    color: var(--on-accent);
  }

  .cookie-picker-option.active .cookie-picker-option-alias,
  .cookie-picker-option.active .cookie-picker-option-meta {
    color: var(--on-accent);
  }

  .cookie-picker-option-alias {
    color: var(--text);
    font-weight: 600;
  }

  .cookie-picker-option-meta {
    color: var(--text-dim);
    font-size: 12px;
  }
</style>
