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

  let { count, onDownload } = $props();
</script>

<div class="feedback feedback-enter" data-supported="true">
  <span class="feedback-text">
    {$t('downloadbox.batch_detected', { count })}
  </span>
</div>

<button class="button action-btn" onclick={onDownload}>
  {$t('downloadbox.batch_download_all')}
</button>

<style>
  .feedback {
    display: flex;
    align-items: center;
    gap: calc(var(--padding) / 2);
  }

  .feedback[data-supported="true"] {
    color: var(--text);
  }

  .feedback-text {
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: var(--padding) calc(var(--padding) * 2);
    font-size: var(--text-base);
    font-weight: 600;
    background: var(--accent);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    color: var(--on-accent);
    cursor: pointer;
    box-shadow: 4px 4px 0 var(--shadow-hard);
    transition: background var(--duration-fast) var(--ease-out), transform var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out);
  }

  @media (hover: hover) {
    .action-btn:hover {
      background: var(--cta-hover);
    }
  }

  .action-btn:active {
    background: var(--cta-press);
    transform: translate(2px, 2px);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  .action-btn:disabled {
    cursor: default;
  }
</style>
