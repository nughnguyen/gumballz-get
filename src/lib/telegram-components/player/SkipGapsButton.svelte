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
  import type { SkipGaps } from "$lib/player-types";

  type Props = {
    skipGaps: SkipGaps | null;
    currentTimeMs: number;
    bingeAutoSkip?: boolean;
    onSkip: (toMs: number, kind: "intro" | "outro") => void;
  };

  let { skipGaps, currentTimeMs, bingeAutoSkip = false, onSkip }: Props = $props();

  const isInIntro = $derived.by(() => {
    if (!skipGaps?.intro_from_ms || !skipGaps?.intro_to_ms) return false;
    return currentTimeMs >= skipGaps.intro_from_ms && currentTimeMs < skipGaps.intro_to_ms;
  });

  const isInOutro = $derived.by(() => {
    if (!skipGaps?.outro_from_ms) return false;
    return currentTimeMs >= skipGaps.outro_from_ms;
  });

  let autoSkippedIntroFor = $state<number | null>(null);
  let autoSkippedOutroFor = $state<number | null>(null);

  $effect(() => {
    if (!bingeAutoSkip || !skipGaps) return;
    if (isInIntro && skipGaps.intro_to_ms != null) {
      const key = skipGaps.intro_from_ms ?? -1;
      if (autoSkippedIntroFor !== key) {
        autoSkippedIntroFor = key;
        onSkip(skipGaps.intro_to_ms, "intro");
      }
    }
  });

  $effect(() => {
    if (!skipGaps) {
      autoSkippedIntroFor = null;
      autoSkippedOutroFor = null;
    }
  });

  function skipIntro() {
    if (skipGaps?.intro_to_ms != null) onSkip(skipGaps.intro_to_ms, "intro");
  }

  function skipOutro() {
    if (skipGaps?.outro_from_ms != null) {
      onSkip(skipGaps.outro_from_ms + 60_000, "outro");
    }
  }
</script>

{#if isInIntro}
  <button type="button" class="skip-btn" onclick={skipIntro}>
    <span>Pular intro</span>
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <polyline points="13 17 18 12 13 7" />
      <polyline points="6 17 11 12 6 7" />
    </svg>
  </button>
{:else if isInOutro}
  <button type="button" class="skip-btn" onclick={skipOutro}>
    <span>Pular créditos</span>
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <polyline points="13 17 18 12 13 7" />
      <polyline points="6 17 11 12 6 7" />
    </svg>
  </button>
{/if}

<style>
  .skip-btn {
    position: absolute;
    bottom: 64px;
    right: 16px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    background: color-mix(in oklab, black 75%, transparent);
    color: white;
    border: 1px solid color-mix(in oklab, white 20%, transparent);
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    z-index: 5;
    backdrop-filter: blur(6px);
    box-shadow: 0 4px 12px color-mix(in oklab, black 30%, transparent);
    animation: slide-in 200ms ease-out;
  }

  .skip-btn:hover {
    background: color-mix(in oklab, var(--accent) 60%, black);
    border-color: var(--accent);
  }

  @keyframes slide-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .skip-btn {
      animation: none;
    }
  }
</style>
