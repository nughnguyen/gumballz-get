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
  import { PlatformBridge } from "$lib/core/platform";

  interface Progress { completed: number; total: number; file: string; success: boolean }
  interface Result { input: string; output: string | null; error: string | null }
  let inputs = $state<string[]>([]);
  let outputDir = $state("");
  let format = $state("webp");
  let progress = $state<Progress | null>(null);
  let results = $state<Result[]>([]);
  let busy = $state(false);
  let error = $state("");
  let unlisten: (() => void) | undefined;

  onMount(() => { void PlatformBridge.listen<Progress>("asset-forge-progress", (event) => (progress = event)).then((stop) => (unlisten = stop)); });
  onDestroy(() => unlisten?.());

  async function chooseFiles(directory: boolean) {
    const paths = await PlatformBridge.selectPath({ directory, multiple: !directory });
    inputs = Array.isArray(paths) ? paths : typeof paths === "string" ? [paths] : inputs;
  }
  async function chooseOutput() {
    const path = await PlatformBridge.selectPath({ directory: true });
    if (typeof path === "string") outputDir = path;
  }
  async function convert() {
    if (!inputs.length || !outputDir) return;
    busy = true; error = ""; results = []; progress = { completed: 0, total: 0, file: "", success: true };
    try { results = await PlatformBridge.invoke<Result[]>("asset_forge_convert", { inputs, outputDir, format }); }
    catch (cause) { error = cause instanceof Error ? cause.message : "Batch conversion failed."; }
    finally { busy = false; }
  }
</script>

<section class="plugin-page">
  <p class="eyebrow">PLUGIN STUDIO · MEDIA</p><h1>Batch Media & Asset Forge</h1>
  <p class="lede">Convert a folder or a selection of up to 256 media files with parallel FFmpeg workers.</p>
  <div class="panel">
    <div class="actions"><button onclick={() => void chooseFiles(true)}>Choose folder</button><button onclick={() => void chooseFiles(false)}>Choose files</button></div>
    <p class="path">{inputs.length ? `${inputs.length} source selection(s): ${inputs.join(", ")}` : "No source selected"}</p>
    <label>Output format <select bind:value={format}><option value="webp">WebP image</option><option value="ogg">Ogg Vorbis audio</option><option value="mp3">MP3 audio</option><option value="flac">FLAC audio</option></select></label>
    <div class="actions"><button onclick={() => void chooseOutput()}>{outputDir || "Choose output folder"}</button><button class="primary" disabled={!inputs.length || !outputDir || busy} onclick={() => void convert()}>{busy ? "Converting…" : "Convert batch"}</button></div>
  </div>
  {#if progress?.total}<p class="progress" role="status">{progress.completed}/{progress.total} · {progress.file}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if results.length}<div class="results"><h2>Conversion results</h2>{#each results as result}<p class:failed={result.error}>{result.output || result.input} {result.error ? `— ${result.error}` : "— done"}</p>{/each}</div>{/if}
</section>

<style>
  .plugin-page { width:min(100%,900px); margin:0 auto; padding:32px; color:var(--text); }.eyebrow { font:700 11px 'JetBrains Mono',monospace; letter-spacing:.12em; color:var(--accent-text); }.plugin-page h1 { margin:8px 0; font-size:clamp(30px,5vw,52px); letter-spacing:-.04em; }.lede { max-width:650px; line-height:1.6; color:var(--text-dim); }.panel { display:grid; gap:16px; margin-top:28px; padding:22px; border:2px solid #101820; border-radius:12px; background:#d7f3d1; box-shadow:4px 4px 0 #101820; color:#101820; }.actions { display:flex; flex-wrap:wrap; gap:8px; }.panel button,.panel select { padding:10px 13px; border:2px solid #101820; border-radius:7px; background:white; font:inherit; font-size:13px; font-weight:700; cursor:pointer; }.panel .primary { background:#fbbf24; }.panel button:disabled { opacity:.85; cursor:not-allowed; }.path { overflow-wrap:anywhere; font-size:12px; }.panel label { display:grid; gap:6px; font-size:13px; font-weight:700; }.panel select { width:max-content; }.progress,.error { padding:12px; border:1px solid #101820; }.error,.failed { color:var(--danger); }.results { margin-top:24px; }.results p { padding:8px; border-bottom:1px solid var(--border); overflow-wrap:anywhere; }
  @media(max-width:700px) {.plugin-page{padding:20px;}}
</style>
