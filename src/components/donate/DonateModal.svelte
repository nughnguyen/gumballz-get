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
  import { onDestroy, onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { PlatformBridge } from "$lib/core/platform";
  import { DONATE_CONFIG, vietQrUrl } from "$lib/donate/donate.config";

  let { onClose }: { onClose: () => void } = $props();
  let amount = $state("");
  let note = $state(DONATE_CONFIG.memo);
  let qrSrc = $state(vietQrUrl("", DONATE_CONFIG.memo));
  let loading = $state(true);
  let offline = $state(typeof navigator !== "undefined" && !navigator.onLine);
  let copied = $state("");
  let qrFailed = $state(false);
  let dialogEl: HTMLDialogElement;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  function scheduleQr() {
    loading = true;
    qrFailed = false;
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      qrSrc = vietQrUrl(amount, note);
    }, 350);
  }

  function setAmount(value: string) {
    amount = value.replace(/\D/g, "").slice(0, 12);
    scheduleQr();
  }

  function onAmountInput(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const digits = input.value.replace(/\D/g, "").slice(0, 12);
    amount = digits;
    input.value = digits ? Number(digits).toLocaleString("vi-VN") : "";
    scheduleQr();
  }

  async function copy(value: string, id: string) {
    try {
      await PlatformBridge.copyText(value);
      copied = id;
      setTimeout(() => (copied = ""), 1600);
    } catch {
      copied = "";
    }
  }

  function updateNetwork() {
    offline = !navigator.onLine;
    if (!offline) scheduleQr();
  }

  onMount(() => {
    dialogEl.showModal();
    window.addEventListener("online", updateNetwork);
    window.addEventListener("offline", updateNetwork);
  });
  onDestroy(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
    window.removeEventListener("online", updateNetwork);
    window.removeEventListener("offline", updateNetwork);
  });
</script>

<dialog bind:this={dialogEl} class="donate-dialog" aria-labelledby="donate-title" oncancel={(event) => { event.preventDefault(); onClose(); }}>
  <header>
    <div><span class="eyebrow">GUMBALLZ GET · SUPPORT</span><h2 id="donate-title">{$t("donate.title")}</h2></div>
    <button class="close" type="button" onclick={onClose} aria-label={$t("donate.close")}>×</button>
  </header>

  <div class="content">
    <div class="qr-card">
      {#if offline || qrFailed}
        <div class="offline">{$t("donate.qr_offline")}</div>
      {:else}
        {#if loading}<div class="loading" role="status">{$t("donate.qr_loading")}</div>{/if}
        <img src={qrSrc} alt="VietQR bank transfer code" onload={() => (loading = false)} onerror={() => { loading = false; qrFailed = true; }} />
      {/if}
    </div>

    <label class="field">{$t("donate.amount")}
      <input inputmode="numeric" value={amount ? Number(amount).toLocaleString("vi-VN") : ""} placeholder="50.000" oninput={onAmountInput} />
    </label>
    <div class="presets" aria-label="Suggested amounts">
      <button type="button" onclick={() => setAmount("20000")}>{$t("donate.preset_20000")}</button>
      <button type="button" onclick={() => setAmount("50000")}>{$t("donate.preset_50000")}</button>
      <button type="button" onclick={() => setAmount("100000")}>{$t("donate.preset_100000")}</button>
      <button type="button" onclick={() => setAmount("200000")}>{$t("donate.preset_200000")}</button>
      <button type="button" onclick={() => setAmount("")}>{$t("donate.any_amount")}</button>
    </div>
    <label class="field">{$t("donate.memo")}
      <input bind:value={note} maxlength="80" oninput={scheduleQr} />
    </label>

    <section class="details" aria-label="Bank transfer details">
      <div class="detail"><span>{$t("donate.bank")}</span><strong>{DONATE_CONFIG.bank}</strong></div>
      <div class="detail"><span>{$t("donate.account")}</span><strong>{DONATE_CONFIG.account}</strong><button type="button" onclick={() => void copy(DONATE_CONFIG.account, "account")}>{$t(copied === "account" ? "donate.copied" : "donate.copy")}</button></div>
      <div class="detail"><span>{$t("donate.account_name")}</span><strong>{DONATE_CONFIG.accountName}</strong></div>
      <div class="detail"><span>{$t("donate.memo")}</span><strong>{note || DONATE_CONFIG.memo}</strong><button type="button" onclick={() => void copy(note || DONATE_CONFIG.memo, "memo")}>{$t(copied === "memo" ? "donate.copied" : "donate.copy")}</button></div>
    </section>
    <button class="zypage" type="button" onclick={() => void PlatformBridge.openExternal(DONATE_CONFIG.zypageUrl)}>{$t("donate.open_zypage")} ↗</button>
  </div>
</dialog>

<style>
  .donate-dialog { position:fixed; inset:50% auto auto 50%; transform:translate(-50%,-50%); width:min(94vw,560px); max-height:90vh; overflow:auto; padding:0; border:2px solid #101820; border-radius:14px; color:#101820; background:#fffdf6; box-shadow:8px 8px 0 #101820; font-family:inherit; }
  .donate-dialog::backdrop { background:rgb(10 18 24 / .68); backdrop-filter:blur(3px); }
  header { position:sticky; top:0; display:flex; justify-content:space-between; align-items:center; padding:18px 22px; border-bottom:2px solid #101820; background:#fbbf24; z-index:1; }
  h2 { margin:4px 0 0; font-size:24px; }.eyebrow { font:700 10px 'JetBrains Mono',monospace; letter-spacing:.13em; }.close { width:36px; height:36px; border:2px solid #101820; border-radius:8px; background:#fff; font-size:24px; cursor:pointer; }
  .detail button,.presets button,.field input { font-family:inherit; }
  .content { padding:20px; }.details { display:grid; gap:8px; margin-bottom:18px; }.detail { display:grid; grid-template-columns:1fr auto auto; align-items:center; gap:10px; padding:10px 12px; border:2px solid #101820; border-radius:8px; background:#f5f3ec; }.detail span { font-size:12px; color:#46515c; }.detail strong { font:700 12px 'JetBrains Mono',monospace; }.detail button,.presets button { padding:6px 9px; border:2px solid #101820; border-radius:6px; background:white; font:700 11px inherit; cursor:pointer; }
  .field { display:grid; gap:6px; margin-top:12px; font-size:12px; font-weight:700; }.field input { box-sizing:border-box; width:100%; padding:11px 12px; border:2px solid #101820; border-radius:7px; font:500 14px inherit; }.presets { display:flex; flex-wrap:wrap; gap:6px; margin:10px 0 6px; }.presets button { background:#e7faff; }.qr-card { display:grid; place-items:center; min-height:220px; margin:18px auto 12px; padding:10px; border:2px dashed #101820; border-radius:10px; text-align:center; }.qr-card img { display:block; width:min(100%,290px); height:auto; }.loading { font-size:13px; font-weight:700; }.offline { max-width:280px; padding:15px; color:#5c3d00; background:#fff1c2; border:2px solid #fbbf24; border-radius:8px; font-size:13px; }.zypage { width:100%; padding:12px; border:2px solid #101820; border-radius:8px; background:#fbbf24; font-weight:800; cursor:pointer; }
</style>
