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
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import DialogContainer from "./DialogContainer.svelte";
  import { t } from "$lib/i18n";

  let isOpen = $state(false);
  let activeCount = $state(0);

  onMount(() => {
    let unlisten: (() => void) | undefined;
    listen<number>("exit-confirm-required", (event) => {
      activeCount = typeof event.payload === "number" ? event.payload : 0;
      isOpen = true;
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  });

  function cancel() {
    isOpen = false;
  }

  async function confirm() {
    isOpen = false;
    await invoke("force_exit_app");
  }
</script>

<DialogContainer bind:isOpen titleId="confirm-close-title" onClose={cancel}>
  <h3 id="confirm-close-title" class="dialog-title">
    {$t("confirm_close.title")}
  </h3>
  <p class="dialog-body">
    {$t("confirm_close.message", { values: { count: activeCount } })}
  </p>
  <div class="dialog-actions">
    <button type="button" class="btn btn-secondary" onclick={cancel}>
      {$t("confirm_close.cancel")}
    </button>
    <button type="button" class="btn btn-destructive" onclick={confirm}>
      {$t("confirm_close.confirm")}
    </button>
  </div>
</DialogContainer>

<style>
  .dialog-title {
    margin: 0;
    padding: calc(var(--padding) * 1.25) calc(var(--padding) * 1.5) calc(var(--padding) * 0.75);
    font-size: 15px;
    font-weight: 600;
    color: var(--secondary);
  }

  .dialog-body {
    margin: 0;
    padding: 0 calc(var(--padding) * 1.5) calc(var(--padding) * 1.25);
    font-size: 13px;
    line-height: 1.5;
    color: var(--secondary);
    opacity: 0.85;
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: calc(var(--padding) * 0.5);
    padding: calc(var(--padding) * 0.75) calc(var(--padding) * 1.5) calc(var(--padding) * 1.25);
    border-top: none;
  }






</style>
