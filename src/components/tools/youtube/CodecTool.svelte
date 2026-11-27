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
  /** Force H.264 (study 39, enhanced-h264ify): the option lives in the browser extension; here is the guide and script for Tampermonkey users. */
  import { t } from "$lib/i18n";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import { openUrl } from "$lib/tools/rt";

  const USERSCRIPT = `// ==UserScript==
// @name         GumballZ Get · Force H.264 on YouTube
// @namespace    com.gumballz.get
// @version      1.0
// @match        *://*.youtube.com/*
// @match        *://*.youtube-nocookie.com/*
// @run-at       document-start
// @grant        none
// ==/UserScript==
(() => {
  const blocked = /vp9|vp09|vp8|vp08|av01/i;
  const wrap = (obj, name, miss) => {
    const orig = obj?.[name];
    if (typeof orig !== "function") return;
    obj[name] = function (type, ...rest) {
      return typeof type === "string" && blocked.test(type) ? miss : orig.call(this, type, ...rest);
    };
  };
  wrap(window.MediaSource, "isTypeSupported", false);
  wrap(window.HTMLMediaElement?.prototype, "canPlayType", "");
})();`;

  async function copy() { await navigator.clipboard.writeText(USERSCRIPT); showToast("success", $t("tools.common.copied") as string); }
</script>

<div class="tool">
  <section>
    <div class="group">
      <div class="group-row"><div class="group-row-content"><div class="group-row-title">{$t("tools.codec.what")}</div><div class="group-row-sub">{$t("tools.codec.what_hint")}</div></div></div>
      <div class="group-row"><div class="group-row-content"><div class="group-row-title">{$t("tools.codec.how")}</div><ol class="steps"><li>{$t("tools.codec.step1")}</li><li>{$t("tools.codec.step2")}</li><li>{$t("tools.codec.step3")}</li></ol></div></div>
      <div class="group-row">
        <div class="group-row-content"><div class="group-row-title">{$t("tools.codec.extension")}</div><div class="group-row-sub">{$t("tools.codec.extension_hint")}</div></div>
        <div class="group-row-trailing btn-row"><a class="btn btn-secondary btn-sm" href="/settings">{$t("tools.codec.open_settings")}</a><button class="btn btn-ghost btn-sm" type="button" onclick={() => openUrl("https://github.com/nughnguyen/gumballz-get")}>GitHub</button></div>
      </div>
    </div>
  </section>
  <section>
    <div class="group">
      <div class="group-row">
        <div class="group-row-content"><div class="group-row-title">{$t("tools.codec.userscript")}</div><div class="group-row-sub">{$t("tools.codec.userscript_hint")}</div></div>
        <div class="group-row-trailing"><button class="btn btn-secondary btn-sm" type="button" onclick={copy}>{$t("tools.common.copy")}</button></div>
      </div>
      <div class="group-row"><pre class="code">{USERSCRIPT}</pre></div>
    </div>
  </section>
</div>

<style>
  .tool { display: flex; flex-direction: column; gap: var(--space-5); }
  .steps { margin: var(--space-1) 0 0; padding-left: 1.2em; font-size: var(--text-sm); color: var(--text-muted); display: flex; flex-direction: column; gap: 4px; }
  .code { margin: 0; width: 100%; white-space: pre-wrap; font-family: var(--font-mono); font-size: var(--text-xs); max-height: 260px; overflow: auto; color: var(--text-muted); }
</style>
