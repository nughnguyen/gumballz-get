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
  /** Manter o computador acordado (estudo 29, PowerToys Awake). */
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import { errText } from "$lib/tools/rt";

  let active = $state(false);
  onMount(async () => { active = await invoke<boolean>("tool_awake_get"); });
  async function toggle() {
    try { active = await invoke<boolean>("tool_awake_set", { active: !active }); }
    catch (e) { showToast("error", errText(e)); }
  }
</script>

<div class="tool">
  <section>
    <div class="group">
      <div class="group-row">
        <div class="group-row-content">
          <div class="group-row-title">{active ? $t("tools.awake.on") : $t("tools.awake.off")}</div>
          <div class="group-row-sub">{$t("tools.awake.intro")}</div>
        </div>
        <div class="group-row-trailing"><button class="btn {active ? 'btn-secondary' : 'btn-primary'}" type="button" onclick={toggle}>{active ? $t("tools.awake.stop") : $t("tools.awake.start")}</button></div>
      </div>
      <div class="group-row"><div class="group-row-sub">{$t("tools.awake.note")}</div></div>
    </div>
  </section>
</div>

<style>
  .tool { display: flex; flex-direction: column; gap: var(--space-5); }
</style>
