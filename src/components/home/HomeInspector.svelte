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
  import type { Snippet } from "svelte";

  let {
    open = false,
    title,
    children,
  }: {
    open?: boolean;
    title: string;
    children: Snippet;
  } = $props();
</script>

{#if open}
  <section class="home-panel" aria-label={title}>
    <div class="home-panel-body">
      {@render children()}
    </div>
  </section>
{/if}

<style>
  .home-panel {
    width: 100%;
    background: var(--surface);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-lg);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    padding: var(--space-4);
    animation: panel-in var(--duration-slow) var(--ease-spring);
    transform-origin: top center;
  }

  .home-panel-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  @keyframes panel-in {
    from { opacity: 0; transform: translateY(-6px) scale(0.99); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  @media (prefers-reduced-motion: reduce) {
    .home-panel {
      animation: none;
    }
  }
</style>
