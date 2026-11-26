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
  import DonateModal from "./DonateModal.svelte";

  let { compact = false, inline = false }: { compact?: boolean; inline?: boolean } = $props();
  let open = $state(false);
</script>

<button type="button" class="donate-trigger" class:compact class:inline aria-label={$t("donate.support")} onclick={() => (open = true)}>
  <span aria-hidden="true">♥</span>
  <span>{$t("donate.support")}</span>
</button>
{#if open}<DonateModal onClose={() => (open = false)} />{/if}

<style>
  .donate-trigger { display:flex; align-items:center; justify-content:center; gap:8px; width:100%; min-height:36px; margin-block:0 8px; padding:6px 10px; border:2px solid #101820; border-radius:9px; background:#fbbf24; color:#101820; font:inherit; font-size:12px; font-weight:700; cursor:pointer; box-shadow:3px 3px 0 #101820; }
  .donate-trigger:active { transform:translate(2px,2px); box-shadow:1px 1px 0 #101820; }
  .donate-trigger span:first-child { font-size:14px; }
  .compact span:last-child { display:none; }
  .inline { width:auto; margin:0; flex-shrink:0; }
</style>
