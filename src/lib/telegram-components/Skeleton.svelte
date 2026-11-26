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
  let {
    lines = [80, 50],
    height,
    gap = 6,
    block = false,
  } = $props<{
    lines?: number[];
    height?: number;
    gap?: number;
    block?: boolean;
  }>();
</script>

{#if block}
  <div
    class="sk-block"
    style:height={height ? `${height}px` : undefined}
  ></div>
{:else}
  <div class="sk-stack" style:gap={`${gap}px`}>
    {#each lines as w, i (i)}
      <div class="sk-line" style:width={`${w}%`}></div>
    {/each}
  </div>
{/if}

<style>
  .sk-stack {
    display: flex;
    flex-direction: column;
  }
  .sk-line,
  .sk-block {
    background: linear-gradient(
      90deg,
      color-mix(in oklab, var(--content-border) 60%, transparent),
      color-mix(in oklab, var(--content-border) 30%, transparent),
      color-mix(in oklab, var(--content-border) 60%, transparent)
    );
    background-size: 200% 100%;
    animation: sk-shimmer 1.6s ease-in-out infinite;
    border-radius: 6px;
  }
  .sk-line {
    height: 10px;
  }
  .sk-block {
    width: 100%;
    height: 100%;
    border-radius: 8px;
  }
  @keyframes sk-shimmer {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .sk-line,
    .sk-block {
      animation: none;
    }
  }
</style>
