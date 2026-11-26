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
  /** Filtros de conteúdo: anúncios, IA (3 níveis), tipo e largura mínima. */
  import { t } from "$lib/i18n";
  import type { Filters } from "$lib/tools/pinterest";

  let { filters = $bindable(), compact = false }: { filters: Filters; compact?: boolean } = $props();
</script>

<div class="group-row">
  <div class="group-row-content"><div class="group-row-title">{$t("tools.pinterest.hide_promoted")}</div></div>
  <div class="group-row-trailing"><input class="checkbox" type="checkbox" bind:checked={filters.skip_promoted} /></div>
</div>
<div class="group-row">
  <div class="group-row-content"><div class="group-row-title">{$t("tools.pinterest.hide_ai")}</div></div>
  <div class="group-row-trailing">
    <select class="input" bind:value={filters.ai_level}>
      <option value={0}>{$t("tools.pinterest.ai_off")}</option>
      <option value={3}>{$t("tools.pinterest.ai_labeled")}</option>
      <option value={2}>{$t("tools.pinterest.ai_tools")}</option>
      <option value={1}>{$t("tools.pinterest.ai_any")}</option>
    </select>
  </div>
</div>
{#if !compact}
  <div class="group-row">
    <div class="group-row-content"><div class="group-row-title">{$t("tools.pinterest.only")}</div></div>
    <div class="group-row-trailing">
      <select class="input" bind:value={filters.only_kind}>
        <option value="">{$t("tools.pinterest.only_all")}</option>
        <option value="image">{$t("tools.pinterest.only_image")}</option>
        <option value="video">{$t("tools.pinterest.only_video")}</option>
        <option value="gif">{$t("tools.pinterest.only_gif")}</option>
      </select>
    </div>
  </div>
  <div class="group-row">
    <div class="group-row-content"><div class="group-row-title">{$t("tools.pinterest.min_width")}</div></div>
    <div class="group-row-trailing btn-row"><input class="input" type="number" min="0" step="100" bind:value={filters.min_width} style:width="7em" /> px</div>
  </div>
{/if}
