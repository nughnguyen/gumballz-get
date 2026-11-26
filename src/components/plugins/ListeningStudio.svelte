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
  import { PlatformBridge } from "$lib/core/platform";
  import type { SubtitleSegment } from "$lib/plugins/types";

  let subtitlePath = $state("");
  let mediaPath = $state("");
  let outputDir = $state("");
  let segments = $state<SubtitleSegment[]>([]);
  let clips = $state<string[]>([]);
  let busy = $state(false);
  let error = $state("");

  async function chooseSubtitle() {
    const path = await PlatformBridge.selectPath({ extensions: ["srt", "vtt"] });
    if (typeof path === "string") subtitlePath = path;
  }
  async function chooseMedia() {
    const path = await PlatformBridge.selectPath({ extensions: ["mp3", "m4a", "wav", "mp4", "mkv", "webm"] });
    if (typeof path === "string") mediaPath = path;
  }
  async function chooseOutput() {
    const path = await PlatformBridge.selectPath({ directory: true });
    if (typeof path === "string") outputDir = path;
  }

  async function parse() {
    if (!subtitlePath) return;
    busy = true; error = "";
    try { segments = await PlatformBridge.invoke<SubtitleSegment[]>("listening_studio_parse_subtitles", { path: subtitlePath }); }
    catch (cause) { error = cause instanceof Error ? cause.message : "Could not read subtitles."; }
    finally { busy = false; }
  }

  async function split() {
    if (!subtitlePath || !mediaPath || !outputDir) return;
    busy = true; error = ""; clips = [];
    try { clips = await PlatformBridge.invoke<string[]>("listening_studio_split_audio", { audioPath: mediaPath, subtitlePath, outputDir }); }
    catch (cause) { error = cause instanceof Error ? cause.message : "Could not split subtitle clips."; }
    finally { busy = false; }
  }

  async function exportFile(format: "csv" | "markdown") {
    if (!subtitlePath) return;
    try {
      const content = await PlatformBridge.invoke<string>("listening_studio_export", { path: subtitlePath, format });
      PlatformBridge.downloadBlob(new Blob([content], { type: format === "csv" ? "text/csv" : "text/markdown" }), `listening-studio-worksheet.${format === "markdown" ? "md" : "csv"}`);
    } catch (cause) { error = cause instanceof Error ? cause.message : "Could not export worksheet."; }
  }
</script>

<section class="plugin-page">
  <p class="eyebrow">PLUGIN STUDIO · LEARNING</p><h1>Listening & Vocab Studio</h1>
  <p class="lede">Turn subtitle cues into shadowing clips and a fill-in-the-blank worksheet.</p>
  <div class="workflow">
    <article class="step"><span class="number">01</span><h2>Choose subtitles</h2><p>{subtitlePath || "Select an .srt or .vtt file"}</p><button onclick={() => void chooseSubtitle()}>Choose subtitle file</button></article>
    <article class="step"><span class="number">02</span><h2>Preview cues</h2><p>{segments.length ? `${segments.length} subtitle cues parsed` : "Inspect timestamps and text before splitting."}</p><button disabled={!subtitlePath || busy} onclick={() => void parse()}>{busy ? "Working…" : "Parse subtitles"}</button></article>
    <article class="step"><span class="number">03</span><h2>Make listening clips</h2><p>{mediaPath || "Select matching audio or video"}</p><button onclick={() => void chooseMedia()}>Choose media</button><p>{outputDir || "Choose a clip output folder"}</p><button onclick={() => void chooseOutput()}>Choose output folder</button><button class="primary" disabled={!segments.length || !mediaPath || !outputDir || busy} onclick={() => void split()}>Split sentence clips</button></article>
  </div>
  <div class="exports"><button disabled={!subtitlePath} onclick={() => void exportFile("markdown")}>Export Markdown cloze</button><button disabled={!subtitlePath} onclick={() => void exportFile("csv")}>Export CSV</button></div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if clips.length}<p class="success" role="status">Created {clips.length} audio clips in {outputDir}.</p>{/if}
  {#if segments.length}<ol class="cue-list">{#each segments.slice(0, 80) as cue, index}<li><time>{cue.start.toFixed(2)}–{cue.end.toFixed(2)}</time><span>{cue.text}</span></li>{/each}</ol>{/if}
</section>

<style>
  .plugin-page { width:min(100%,1000px); margin:0 auto; padding:32px; color:var(--text); }.eyebrow { font:700 11px 'JetBrains Mono',monospace; letter-spacing:.12em; color:var(--accent-text); }.plugin-page h1 { margin:8px 0; font-size:clamp(30px,5vw,52px); letter-spacing:-.04em; }.lede { max-width:650px; line-height:1.6; color:var(--text-dim); }.workflow { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:16px; margin-top:28px; }.step { display:flex; flex-direction:column; gap:10px; min-width:0; padding:18px; border:2px solid #101820; border-radius:12px; background:#cff6fc; color:#101820; box-shadow:4px 4px 0 #101820; }.step h2 { margin:0; font-size:19px; }.step p { overflow-wrap:anywhere; margin:0; font-size:12px; }.number { font:700 11px 'JetBrains Mono',monospace; }.step button,.exports button { padding:9px 12px; border:2px solid #101820; border-radius:7px; background:white; font:inherit; font-size:12px; font-weight:700; cursor:pointer; }.step .primary { background:#fbbf24; }.step button:disabled,.exports button:disabled { opacity:.85; cursor:not-allowed; }.exports { display:flex; gap:8px; margin:20px 0; }.error,.success { padding:12px; border:1px solid currentColor; }.error { color:var(--danger); }.success { color:var(--success); }.cue-list { display:grid; gap:5px; padding-left:30px; }.cue-list li { display:flex; gap:16px; padding:8px; border-bottom:1px solid var(--border); }.cue-list time { flex:0 0 110px; font:12px 'JetBrains Mono',monospace; color:var(--text-dim); }
  @media(max-width:800px) {.workflow{grid-template-columns:1fr;}.plugin-page{padding:20px;}}
</style>
