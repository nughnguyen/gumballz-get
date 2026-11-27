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
  /** Cookie line (cookies.txt file or string), remembered across screens. */
  import { t } from "$lib/i18n";
  import { pickFile } from "$lib/tools/rt";
  import { saveCookies } from "$lib/tools/pinterest";

  let { value = $bindable("") }: { value: string } = $props();

  $effect(() => {
    saveCookies(value);
  });
</script>

<div class="group-row">
  <div class="group-row-content">
    <div class="group-row-title">{$t("tools.pinterest.cookies")}</div>
    <div class="group-row-sub">{$t("tools.pinterest.cookies_hint")}</div>
    <input class="input" type="text" bind:value placeholder={$t("tools.pinterest.cookies_placeholder")} spellcheck="false" autocomplete="off" />
  </div>
  <div class="group-row-trailing btn-row">
    {#if value}<button class="btn btn-ghost btn-sm" type="button" onclick={() => (value = "")}>×</button>{/if}
    <button class="btn btn-secondary btn-sm" type="button" onclick={async () => { const f = await pickFile([{ name: "cookies.txt", extensions: ["txt"] }]); if (f) value = f; }}>{$t("tools.common.choose")}</button>
  </div>
</div>
