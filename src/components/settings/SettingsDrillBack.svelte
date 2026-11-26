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

  let { title, hint = "", onBack }: {
    title: string;
    hint?: string;
    onBack: () => void;
  } = $props();
</script>

<div class="settings-drill-back">
  <button type="button" class="settings-drill-back-btn" onclick={onBack}>
    <span class="settings-drill-back-chevron" aria-hidden="true">‹</span>
    {$t('settings.drill_back')}
  </button>
  <div class="settings-drill-back-head">
    <h5 class="section-title">{title}</h5>
    {#if hint}
      <p class="settings-section-hint">{hint}</p>
    {/if}
  </div>
</div>
