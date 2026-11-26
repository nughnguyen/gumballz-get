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
  import { onDestroy } from "svelte";
  import { t } from "$lib/i18n";
  import { getSettings, updateSettings, toggleBool, changeNumber } from "./settings-helpers";

  let settings = $derived(getSettings());

  let proxyHost = $state("");
  let proxyUsername = $state("");
  let proxyPassword = $state("");

  type ProxyField = "host" | "username" | "password";
  const proxyTimers: Partial<Record<ProxyField, ReturnType<typeof setTimeout>>> = {};
  const pendingProxy: Partial<Record<ProxyField, string>> = {};

  $effect(() => {
    if (settings) {
      if (!("host" in pendingProxy)) proxyHost = settings.proxy?.host ?? "";
      if (!("username" in pendingProxy)) proxyUsername = settings.proxy?.username ?? "";
      if (!("password" in pendingProxy)) proxyPassword = settings.proxy?.password ?? "";
    }
  });

  function proxyPatch(field: ProxyField, value: string) {
    if (field === "host") return { proxy: { host: value } };
    if (field === "username") return { proxy: { username: value } };
    return { proxy: { password: value } };
  }

  function flushProxyField(field: ProxyField) {
    const timer = proxyTimers[field];
    if (timer) {
      clearTimeout(timer);
      delete proxyTimers[field];
    }
    const value = pendingProxy[field];
    if (value === undefined) return;
    delete pendingProxy[field];
    updateSettings(proxyPatch(field, value)).catch(() => {
      pendingProxy[field] = value;
    });
  }

  function queueProxyField(field: ProxyField, value: string) {
    pendingProxy[field] = value;
    const timer = proxyTimers[field];
    if (timer) clearTimeout(timer);
    proxyTimers[field] = setTimeout(() => flushProxyField(field), 800);
  }

  onDestroy(() => {
    flushProxyField("host");
    flushProxyField("username");
    flushProxyField("password");
  });

  async function changeProxyType(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    await updateSettings({ proxy: { proxy_type: value } });
  }

  function handleProxyHost(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    proxyHost = value;
    queueProxyField("host", value);
  }

  function handleProxyUsername(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    proxyUsername = value;
    queueProxyField("username", value);
  }

  function handleProxyPassword(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    proxyPassword = value;
    queueProxyField("password", value);
  }
</script>

{#if settings}
<section class="section">
  <h5 class="section-title">{$t('settings.proxy.title')}</h5>
  <div class="card">
    <div class="setting-row">
      <div class="setting-col">
        <span class="setting-label">{$t('settings.proxy.enabled')}</span>
      </div>
      <button
        class="toggle"
        class:on={settings.proxy?.enabled}
        onclick={() => toggleBool("proxy", "enabled", settings.proxy?.enabled ?? false)}
        role="switch"
        aria-checked={settings.proxy?.enabled ?? false}
        aria-label={$t('settings.proxy.enabled') as string}
      >
        <span class="toggle-knob"></span>
      </button>
    </div>
    {#if settings.proxy?.enabled}
      <div class="divider"></div>
      <div class="setting-row">
        <span class="setting-label">{$t('settings.proxy.type')}</span>
        <select class="select" value={settings.proxy?.proxy_type ?? 'http'} onchange={changeProxyType}>
          <option value="http">HTTP</option>
          <option value="https">HTTPS</option>
          <option value="socks5">SOCKS5</option>
        </select>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <span class="setting-label">{$t('settings.proxy.host')}</span>
        <input type="text" class="input-text" value={proxyHost} oninput={handleProxyHost} onchange={() => flushProxyField("host")} placeholder="127.0.0.1" spellcheck="false" />
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <span class="setting-label">{$t('settings.proxy.port')}</span>
        <input type="number" class="input-number" min="1" max="65535" value={settings.proxy?.port ?? 8080} onchange={(e) => changeNumber("proxy", "port", e)} />
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <span class="setting-label">{$t('settings.proxy.username')}</span>
        <input type="text" class="input-text" value={proxyUsername} oninput={handleProxyUsername} onchange={() => flushProxyField("username")} placeholder="" spellcheck="false" />
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <span class="setting-label">{$t('settings.proxy.password')}</span>
        <input type="password" class="input-text" value={proxyPassword} oninput={handleProxyPassword} onchange={() => flushProxyField("password")} placeholder="" />
      </div>
    {/if}
  </div>
</section>
{/if}
