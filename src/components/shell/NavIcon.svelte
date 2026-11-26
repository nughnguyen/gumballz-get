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
  /**
   * GumballZ Get navigation icon: a gradient tile with a white glyph.
   * Every nav item maps to one glyph + gradient (same colors as the
   * old tile artwork), so the sidebar keeps its look without any
   * image files.
   */
  let {
    icon,
    iconSvg,
    size = 26,
    active = false,
  }: { icon: string; iconSvg?: string; size?: number; active?: boolean } = $props();

  const TILES: Record<string, { glyph: string; from: string; to: string }> = {
    home: { glyph: "house", from: "#22D3EE", to: "#0891B2" },
    downloads: { glyph: "download-simple", from: "#A3E635", to: "#4D7C0F" },
    tools: { glyph: "wrench", from: "#94A3B8", to: "#475569" },
    "social-media": { glyph: "chats-circle", from: "#A78BFA", to: "#7C3AED" },
    marketplace: { glyph: "storefront", from: "#FBBF24", to: "#D97706" },
    settings: { glyph: "gear-six", from: "#64748B", to: "#334155" },
    about: { glyph: "info", from: "#38BDF8", to: "#0369A1" },
    convert: { glyph: "arrows-clockwise", from: "#2DD4BF", to: "#0F766E" },
    courses: { glyph: "books", from: "#FB923C", to: "#C2410C" },
    telegram: { glyph: "paper-plane-tilt", from: "#38BDF8", to: "#1D4ED8" },
    library: { glyph: "book-open", from: "#FB923C", to: "#C2410C" },
    chat: { glyph: "chats-circle", from: "#4CD964", to: "#2AA845" },
    misc: { glyph: "wrench", from: "#9B9BA3", to: "#63636B" },
    plugin: { glyph: "puzzle-piece", from: "#8E8E93", to: "#5C5C60" },
  };

  let tile = $derived(TILES[icon] ?? TILES.plugin);
  let glyphSize = $derived(Math.round(size * 0.64));
</script>

{#if iconSvg}
  <span
    class="nav-icon nav-tile"
    class:nav-icon-active={active}
    style:--tile-from={tile.from}
    style:--tile-to={tile.to}
    style:--tile-size="{size}px"
    aria-hidden="true"
  >
    <svg viewBox="0 0 24 24" width={glyphSize} height={glyphSize} fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
      {#each iconSvg.split(" M").map((d, i) => (i === 0 ? d : "M" + d)) as pathD}
        <path d={pathD} />
      {/each}
    </svg>
  </span>
{:else}
  <span
    class="nav-icon nav-tile"
    class:nav-icon-active={active}
    style:--tile-from={tile.from}
    style:--tile-to={tile.to}
    style:--tile-size="{size}px"
    aria-hidden="true"
  >
    <span class="nav-glyph" style:--glyph="url(/icons/{tile.glyph}.svg)" style:width="{glyphSize}px" style:height="{glyphSize}px"></span>
  </span>
{/if}

<style>
  .nav-tile {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--tile-size);
    height: var(--tile-size);
    flex-shrink: 0;
    border: 2px solid var(--border-ink);
    border-radius: calc(var(--tile-size) * 0.28);
    background: linear-gradient(180deg, var(--tile-from), var(--tile-to));
    box-shadow:
      inset 0 0 0 0.5px rgba(255, 255, 255, 0.25),
      inset 0 -1px 1px rgba(0, 0, 0, 0.12),
      0 0.5px 1px rgba(0, 0, 0, 0.25);
    color: #fff;
    transition: transform 0.15s var(--ease-out, ease-out);
  }

  .nav-glyph {
    display: block;
    background: #fff;
    -webkit-mask: var(--glyph) center / contain no-repeat;
    mask: var(--glyph) center / contain no-repeat;
    filter: drop-shadow(0 1px 1px rgba(0, 0, 0, 0.45));
  }

  /* Custom plugin glyphs are stroked SVG, not a mask — same dark halo so
     the white stroke stays legible on the lightest tile gradients
     (lime/amber tops are ~1.5:1 for bare white). */
  .nav-tile svg {
    filter: drop-shadow(0 1px 1px rgba(0, 0, 0, 0.45));
  }

  .nav-icon-active.nav-tile {
    transform: scale(1.05);
  }

  :global(.mac-nav-item:hover) .nav-tile:not(.nav-icon-active) {
    transform: scale(1.05);
  }

  @media (prefers-reduced-motion: reduce) {
    .nav-tile {
      transition: none;
    }
    :global(.mac-nav-item:hover) .nav-tile:not(.nav-icon-active) {
      transform: none;
    }
  }
</style>
