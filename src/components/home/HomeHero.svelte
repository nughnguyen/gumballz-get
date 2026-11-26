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
  import BrandMark from "$lib/ui/BrandMark.svelte";
  import Confetti from "$components/celebrate/Confetti.svelte";

  let {
    emotion: _emotion,
    compact = false,
    stage = false,
    bubbleText,
    celebrate = false,
  }: {
    emotion: string;
    compact?: boolean;
    stage?: boolean;
    bubbleText?: string;
    celebrate?: boolean;
  } = $props();
</script>

<div class="home-hero" class:compact class:stage>
  <BrandMark size={stage ? 84 : 64} />
  {#if bubbleText && !stage}<span class="hero-message">{bubbleText}</span>{/if}
  <Confetti active={celebrate} />
</div>

<style>
  .home-hero {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-start;
    width: 100%;
    max-height: none;
    overflow: visible;
  }

  .home-hero.compact {
    max-height: none;
  }

  .home-hero.stage {
    width: auto;
  }

  .hero-message {
    margin-top: 12px;
    padding: 8px 12px;
    border: 2px solid var(--border-ink, #0f172a);
    border-radius: 8px;
    background: var(--surface-hi);
    box-shadow: 3px 3px 0 var(--shadow-hard, #000);
    color: var(--text);
    font-size: 13px;
    font-weight: 600;
  }
</style>
