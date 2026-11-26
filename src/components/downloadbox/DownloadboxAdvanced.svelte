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
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { t } from "$lib/i18n";
  import { getSettings } from "$lib/stores/settings-store.svelte";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import { validateYtdlpArgs } from "$lib/utils/ytdlp-args";

  type Template = {
    id: string;
    name: string;
    args: string[];
    created_at_ms: number;
    updated_at_ms: number;
  };

  let url = $state("");
  let argsText = $state("");
  let templates = $state<Template[]>([]);
  let saveName = $state("");
  let savingTpl = $state(false);
  let submitting = $state(false);
  let selectedTemplateId = $state<string | null>(null);

  onMount(() => {
    void refreshTemplates();
  });

  async function refreshTemplates() {
    try {
      templates = await invoke<Template[]>("yt_templates_list");
    } catch (e) {
      console.warn("[advanced-downloadbox] list failed:", e);
    }
  }

  function parseArgs(text: string): string[] {
    const result: string[] = [];
    let current = "";
    let quote: '"' | "'" | null = null;
    for (let i = 0; i < text.length; i++) {
      const ch = text[i];
      if (quote) {
        if (ch === quote) {
          quote = null;
        } else if (ch === "\\" && i + 1 < text.length) {
          current += text[++i];
        } else {
          current += ch;
        }
        continue;
      }
      if (ch === '"' || ch === "'") {
        quote = ch as '"' | "'";
        continue;
      }
      if (/\s/.test(ch)) {
        if (current.length > 0) {
          result.push(current);
          current = "";
        }
        continue;
      }
      current += ch;
    }
    if (current.length > 0) result.push(current);
    return result;
  }

  function applyTemplate(tpl: Template) {
    selectedTemplateId = tpl.id;
    saveName = tpl.name;
    argsText = tpl.args
      .map(a => (/\s/.test(a) ? `"${a.replace(/"/g, '\\"')}"` : a))
      .join(" ");
  }

  async function saveTemplate() {
    if (!saveName.trim()) {
      showToast("error", $t("downloadbox.adv.template_name_required"));
      return;
    }
    savingTpl = true;
    try {
      const args = parseArgs(argsText);
      const tpl = await invoke<Template>("yt_templates_save", {
        request: {
          id: selectedTemplateId,
          name: saveName.trim(),
          args,
        },
      });
      selectedTemplateId = tpl.id;
      await refreshTemplates();
      showToast("success", $t("downloadbox.adv.template_saved"));
    } catch (e) {
      showToast("error", String(e));
    } finally {
      savingTpl = false;
    }
  }

  async function deleteTemplate(tpl: Template) {
    try {
      await invoke("yt_templates_delete", { request: { id: tpl.id } });
      if (selectedTemplateId === tpl.id) {
        selectedTemplateId = null;
        saveName = "";
      }
      await refreshTemplates();
    } catch (e) {
      showToast("error", String(e));
    }
  }

  function newTemplate() {
    selectedTemplateId = null;
    saveName = "";
    argsText = "";
  }

  async function runDownload() {
    const trimmedUrl = url.trim();
    if (!trimmedUrl) {
      showToast("error", $t("downloadbox.adv.url_required"));
      return;
    }
    const settings = getSettings();
    let outputDir = settings?.download.default_output_dir ?? "";
    if (!outputDir || settings?.download.always_ask_path) {
      const picked = await open({
        directory: true,
        title: $t("settings.download.default_output_dir"),
      });
      if (!picked) return;
      outputDir = picked as string;
    }
    const parsedArgs = parseArgs(argsText);
    const check = validateYtdlpArgs(parsedArgs);
    if (!check.valid) {
      showToast("error", `Security error: disallowed argument "${check.disallowed}"`);
      return;
    }

    submitting = true;
    try {
      await invoke("download_with_custom_args", {
        url: trimmedUrl,
        outputDir,
        customArgs: parsedArgs,
        cookieSlug: null,
      });
      showToast("success", $t("downloadbox.adv.queued"));
      url = "";
    } catch (e) {
      showToast("error", String(e));
    } finally {
      submitting = false;
    }
  }
</script>

<div class="advanced">
  <div class="row">
    <label class="field-label" for="adv-url">{$t("downloadbox.adv.url_label")}</label>
    <input
      id="adv-url"
      class="text-input"
      type="text"
      placeholder={$t("downloadbox.adv.url_placeholder")}
      bind:value={url}
      spellcheck="false"
    />
  </div>

  <div class="row">
    <label class="field-label" for="adv-args">{$t("downloadbox.adv.args_label")}</label>
    <textarea
      id="adv-args"
      class="args-input"
      placeholder={$t("downloadbox.adv.args_placeholder")}
      bind:value={argsText}
      spellcheck="false"
      rows="4"
    ></textarea>
    <span class="hint">{$t("downloadbox.adv.args_hint")}</span>
  </div>

  <div class="actions">
    <button
      type="button"
      class="primary-btn"
      onclick={runDownload}
      disabled={submitting || !url.trim()}
    >
      {submitting ? $t("downloadbox.adv.queuing") : $t("downloadbox.adv.run")}
    </button>
  </div>

  <div class="templates">
    <div class="templates-header">
      <h4>{$t("downloadbox.adv.templates_title")}</h4>
      <button type="button" class="ghost-btn small" onclick={newTemplate}>
        {$t("downloadbox.adv.template_new")}
      </button>
    </div>

    <div class="save-row">
      <input
        class="text-input small"
        type="text"
        placeholder={$t("downloadbox.adv.template_name_placeholder")}
        bind:value={saveName}
      />
      <button type="button" class="ghost-btn small" onclick={saveTemplate} disabled={savingTpl}>
        {selectedTemplateId
          ? $t("downloadbox.adv.template_update")
          : $t("downloadbox.adv.template_save")}
      </button>
    </div>

    {#if templates.length > 0}
      <ul class="tpl-list">
        {#each templates as tpl (tpl.id)}
          <li class="tpl-item" class:active={selectedTemplateId === tpl.id}>
            <button type="button" class="tpl-pick" onclick={() => applyTemplate(tpl)}>
              <span class="tpl-name">{tpl.name}</span>
              <span class="tpl-count">{tpl.args.length} {$t("downloadbox.adv.template_flags")}</span>
            </button>
            <button
              type="button"
              class="tpl-delete"
              onclick={() => deleteTemplate(tpl)}
              aria-label={$t("downloadbox.adv.template_delete")}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
              </svg>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty-hint">{$t("downloadbox.adv.templates_empty")}</p>
    {/if}
  </div>
</div>

<style>
  .advanced {
    display: flex;
    flex-direction: column;
    gap: 14px;
    width: 100%;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field-label {
    font-size: var(--text-sm);
    color: var(--text-dim);
    font-weight: 600;
  }
  .text-input,
  .args-input {
    background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    padding: 10px 12px;
    font-size: 14px;
    font-family: ui-monospace, "Cascadia Code", monospace;
    color: var(--text);
    width: 100%;
    box-sizing: border-box;
  }
  .text-input::placeholder,
  .args-input::placeholder {
    color: var(--text-dim);
  }
  .text-input.small {
    font-size: 13px;
    padding: 6px 10px;
  }
  .args-input {
    resize: vertical;
    min-height: 80px;
  }
  .text-input:focus,
  .args-input:focus {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }
  .hint {
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 2px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
  .primary-btn {
    background: var(--cta);
    color: var(--on-cta);
    border: 2px solid var(--border-ink);
    border-radius: var(--border-radius);
    padding: 10px 18px;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 4px 4px 0 var(--shadow-hard);
  }
  .primary-btn:active:not(:disabled) {
    transform: translate(2px, 2px);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }
  .primary-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .ghost-btn {
    background: var(--surface-hi);
    color: var(--text);
    border: 2px solid var(--border-ink);
    border-radius: calc(var(--border-radius) - 2px);
    padding: 6px 12px;
    font-size: var(--text-sm);
    font-weight: 600;
    cursor: pointer;
  }
  .ghost-btn.small {
    font-size: 12px;
    padding: 5px 10px;
  }
  .ghost-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .templates {
    border-top: 2px solid var(--border-ink);
    padding-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .templates-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .templates-header h4 {
    margin: 0;
    font-size: 13px;
    color: var(--text);
    font-weight: 600;
  }
  .save-row {
    display: flex;
    gap: 8px;
  }
  .save-row .text-input {
    flex: 1;
  }
  .tpl-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tpl-item {
    display: flex;
    gap: 4px;
    background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    border-radius: calc(var(--border-radius) - 2px);
    overflow: hidden;
  }
  .tpl-item.active {
    background: var(--accent);
    border: 2px solid var(--border-ink);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }
  .tpl-item.active .tpl-name,
  .tpl-item.active .tpl-count {
    color: var(--on-accent);
  }
  .tpl-pick {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 8px 10px;
    background: transparent;
    border: none;
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }
  .tpl-pick:hover {
    background: var(--surface);
  }
  .tpl-name {
    font-weight: 600;
    font-size: 13px;
    color: var(--text);
  }
  .tpl-count {
    font-size: 11px;
    color: var(--text-dim);
  }
  .tpl-delete {
    background: transparent;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    padding: 0 10px;
  }
  .tpl-delete:hover {
    color: var(--danger);
  }
  .empty-hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-dim);
  }
</style>
