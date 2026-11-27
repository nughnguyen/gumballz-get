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
   * Icon tile for the Tools section: rounded square with a gradient and a
   * white glyph as a mask. Accepts `brand:<name>` (static/brands, Simple
   * Icons) or `glyph:<name>` (static/icons, Phosphor Fill), the same scheme
   * as the sidebar's NavIcon, only at Launchpad size.
   */
  let {
    icon,
    from,
    to,
    via,
    size = 96,
    muted = false,
  }: { icon: string; from: string; to: string; via?: string; size?: number; muted?: boolean } = $props();

  let kind = $derived(icon.startsWith("brand:") ? "brand" : icon.startsWith("app:") ? "app" : "glyph");
  let name = $derived(icon.slice(icon.indexOf(":") + 1));
  let url = $derived(kind === "brand" ? `/brands/${name}.svg` : kind === "app" ? `/apps/${name}.png` : `/icons/${name}.svg`);
  let glyphSize = $derived(Math.round(size * (kind === "brand" ? 0.52 : 0.58)));
  let gradient = $derived(
    via ? `linear-gradient(135deg, ${from} 0%, ${via} 55%, ${to} 100%)` : `linear-gradient(180deg, ${from}, ${to})`,
  );
</script>

{#if kind === "app"}
  <img class="tool-app" class:muted src={url} alt="" width={size} height={size} draggable="false" aria-hidden="true" />
{:else}
  <span
    class="tool-icon"
    class:muted
    style:width="{size}px"
    style:height="{size}px"
    style:border-radius="{Math.round(size * 0.225)}px"
    style:background={gradient}
    aria-hidden="true"
  >
    <img
      class="tool-glyph-img"
      src={url}
      alt=""
      width={glyphSize}
      height={glyphSize}
      draggable="false"
      loading="lazy"
      decoding="async"
    />
  </span>
{/if}

<style>
  .tool-icon {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: #fff;
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.25),
      0 2px 8px -2px rgba(0, 0, 0, 0.25);
    transition: transform var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out);
  }

  /* top highlight, like macOS app icons */
  .tool-icon::before {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.18) 0%, rgba(255, 255, 255, 0) 55%);
    pointer-events: none;
  }

  .tool-app {
    display: block;
    flex-shrink: 0;
    object-fit: contain;
    filter: drop-shadow(0 6px 14px rgba(0, 0, 0, 0.3));
    transition: transform var(--duration-fast) var(--ease-out);
  }

  .tool-app.muted {
    filter: saturate(0.55) drop-shadow(0 6px 14px rgba(0, 0, 0, 0.3));
    opacity: 0.72;
  }

  .tool-icon.muted {
    filter: saturate(0.55);
    opacity: 0.72;
  }

  .tool-glyph-img {
    display: block;
    object-fit: contain;
    filter: brightness(0) invert(1);
    pointer-events: none;
    -webkit-user-drag: none;
  }
</style>
