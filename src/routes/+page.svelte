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
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import BilibiliPreviewExtras from "$components/downloadbox/BilibiliPreviewExtras.svelte";
  import DownloadModeSelector from "$components/downloadbox/DownloadModeSelector.svelte";
  import QualityPicker from "$components/downloadbox/QualityPicker.svelte";
  import FormatSelector from "$components/downloadbox/FormatSelector.svelte";
  import CookieAccountPicker from "$components/downloadbox/CookieAccountPicker.svelte";
  import OutputLocationPicker from "$components/downloadbox/OutputLocationPicker.svelte";
  import DownloadboxAdvanced from "$components/downloadbox/DownloadboxAdvanced.svelte";
  import MediaPreview from "$components/downloadbox/MediaPreview.svelte";
  import PhotoList from "$components/downloadbox/PhotoList.svelte";
  import VideoWatermarkChoice from "$components/downloadbox/VideoWatermarkChoice.svelte";
  import BatchDownload from "$components/downloadbox/BatchDownload.svelte";
  import SearchResults from "$components/downloadbox/SearchResults.svelte";
  import P2pSendDialog from "$components/p2p/P2pSendDialog.svelte";
  import P2pReceiveDialog from "$components/p2p/P2pReceiveDialog.svelte";
  import HomeHero from "$components/home/HomeHero.svelte";
  import HomeUrlBar from "$components/home/HomeUrlBar.svelte";
  import HomeInspector from "$components/home/HomeInspector.svelte";
  import VideoOrListChoice from "$components/home/VideoOrListChoice.svelte";
  import { videoOrListLink } from "$lib/home/video-or-list";
  import {
    type DownloadboxState,
    type PlatformInfo,
    type SearchResult,
    type HomeInputMode,
    type MoreAction,
    showInspectorForState,
    showDownloadboxForState,
    isUrl,
    extractUrls,
  } from "$lib/home/downloadbox-controller";
  import { pluralKey } from "$lib/home/plural";
  import { getDownloads, formatBytes } from "$lib/stores/download-store.svelte";
  import { getDownloadStats } from "$lib/stores/download-stats.svelte";
  import { getSettings, updateSettings } from "$lib/stores/settings-store.svelte";
  import { showToast } from "$lib/stores/toast-store.svelte";
  import { onClipboardUrl } from "$lib/stores/clipboard-monitor";
  import { getMediaPreview, clearMediaPreview } from "$lib/stores/media-preview-store.svelte";
  import { clearPendingExternalPrefill, getPendingExternalPrefill, type ExternalUrlEvent } from "$lib/stores/external-url-store.svelte";
  import { getDownloadboxDraftUrl, setDownloadboxDraftUrl } from "$lib/stores/downloadbox-draft-store.svelte";
  import { t } from "$lib/i18n";
  import { translateBackendError } from "$lib/error-translate";
  import { platformDisplayName } from "$lib/platform-display-names";
  import { documentFormatsFor } from "$lib/document-formats";


  type DownloadStarted = {
    id: number;
    title: string;
  };

  type FormatInfo = {
    format_id: string;
    ext: string;
    resolution: string | null;
    width: number | null;
    height: number | null;
    fps: number | null;
    vcodec: string | null;
    acodec: string | null;
    filesize: number | null;
    tbr: number | null;
    has_video: boolean;
    has_audio: boolean;
    format_note: string | null;
  };

  // Platforms whose content is downloaded via the Courses page (courses
  // plugin + logged-in account), not from a pasted URL.
  const COURSE_PLATFORMS = new Set(["hotmart", "udemy"]);

  let url = $state(getDownloadboxDraftUrl());
  let urlInput = $state<HTMLInputElement | null>(null);
  let homeInputMode = $state<HomeInputMode>("url");
  let downloadboxState = $state<DownloadboxState>({ kind: "idle" });
  let debounceTimer = $state<ReturnType<typeof setTimeout> | null>(null);
  let downloadMode = $state<"auto" | "audio" | "mute">("auto");
  let selectedQuality = $state("best");
  let selectedOutputDir = $state("");
  let clipStart = $state("");
  let clipEnd = $state("");
  let scheduleAt = $state("");
  let scheduleStop = $state("");
  let playlistEntries = $state<{ index: number; title: string; url: string }[]>([]);
  let selectedPlaylistItems = $state<Set<number>>(new Set());
  let playlistLoading = $state(false);
  let listChoice = $state<"clip" | "playlist" | "mix">("clip");
  let videoOrList = $derived(videoOrListLink(url.trim()));
  let torrentEntries = $state<{ index: number; path: string; size_bytes: number }[]>([]);
  let selectedTorrentFiles = $state<Set<number>>(new Set());
  let torrentLoading = $state(false);
  let selectedFormatId = $state<string | null>(null);

  // Document links (Scribd, Google Docs...) offer only document formats —
  // the format picker sits above the file name for every platform, and
  // PDF is pre-selected as soon as a document platform is detected.
  let docFormats = $derived(
    downloadboxState.kind === "detected"
      ? documentFormatsFor(downloadboxState.info.platform)
      : null,
  );
  let formats = $state<FormatInfo[]>([]);
  let loadingFormats = $state(false);
  let formatError = $state<string | null>(null);
  let formatFetchGeneration = $state(0);
  let referer = $state("");

  // Derived quality data from real yt-dlp format info.
  // These update after the user loads formats via FormatSelector.
  let availableHeights = $derived(
    formats.length > 0
      ? [...new Set(
          formats
            .filter(f => f.has_video && typeof f.height === "number" && f.height > 0)
            .map(f => f.height as number)
        )].sort((a, b) => b - a)
      : null
  );
  let hasAudioOnly = $derived(
    formats.some(f => f.has_audio && !f.has_video)
  );

  type CookieAccount = {
    slug: string;
    alias: string;
    captured_at_ms: number;
    cookie_count: number;
    last_used_at_ms: number | null;
  };
  let cookieAccounts = $state<CookieAccount[]>([]);
  let selectedCookieSlug = $state<string | null>(null);
  let cookieHint = $state<"stale" | "expired" | null>(null);
  let advancedMode = $state(false);

  let mediaPreview = $derived(getMediaPreview());
  let dlStats = $derived(getDownloadStats());
  let coursesPluginInstalled = $state<boolean | null>(null);

  onMount(() => {
    invoke<{ id: string; enabled: boolean }[]>("list_plugins")
      .then((plugins) => {
        coursesPluginInstalled = plugins.some((p) => p.id === "courses" && p.enabled);
      })
      .catch(() => {});
  });
  let pendingExternalPrefill = $derived(getPendingExternalPrefill());
  let previewImageLoading = $state(true);
  let showP2pSendDialog = $state(false);
  let p2pReceiveCode = $state<string | null>(null);
  let p2pReceiveUrl = $state("");
  let externalNotice = $state<ExternalUrlEvent | null>(null);
  let lastExternalPrefillId = $state<number | null>(null);
  let pendingAutoDownload = $state(false);

  onMount(() => {
    onClipboardUrl((detectedUrl) => {
      if (downloadboxState.kind === "preparing") return;
      url = detectedUrl;
      const settings = getSettings();
      const autoDownload = !!(settings?.download.auto_download_on_paste && settings?.download.clipboard_detection);
      pendingAutoDownload = autoDownload;
      handleInput();
      showToast("info", $t(autoDownload ? "toast.auto_download_started" : "toast.clipboard_url_detected"));
    });
    if (url.trim()) {
      queueMicrotask(() => handleInput());
    }
    return () => {
      onClipboardUrl(null);
    };
  });

  $effect(() => {
    setDownloadboxDraftUrl(url);
  });

  const AUTO_DOWNLOAD_DELAY_MS = 2000;

  $effect(() => {
    if (!pendingAutoDownload) return;
    if (downloadboxState.kind === "detected") {
      const info = downloadboxState.info;
      if (COURSE_PLATFORMS.has(info.platform) || info.platform === "p2p") {
        pendingAutoDownload = false;
        return;
      }
      pendingAutoDownload = false;
      const snapshotUrl = url;
      setTimeout(() => {
        if (url === snapshotUrl && downloadboxState.kind === "detected") {
          handleAction();
        }
      }, AUTO_DOWNLOAD_DELAY_MS);
    } else if (
      downloadboxState.kind === "unsupported" ||
      downloadboxState.kind === "error" ||
      downloadboxState.kind === "batch" ||
      downloadboxState.kind === "search-results" ||
      downloadboxState.kind === "search-empty" ||
      downloadboxState.kind === "idle"
    ) {
      pendingAutoDownload = false;
    }
  });

  const STALL_THRESHOLD = 30_000;
  let downloads = $derived(getDownloads());
  let stallTick = $state(0);
  let lastCompletionAt = $state(0);
  let firstCompletionOfSession = $state(false);
  let completionSeenIds = new Set<string | number>();
  let addedNoticeActive = $state(false);
  let addedNoticeTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    for (const [id, item] of downloads.entries()) {
      if (item.status === "complete" && !completionSeenIds.has(id)) {
        completionSeenIds.add(id);
        lastCompletionAt = Date.now();
        if (!firstCompletionOfSession) {
          firstCompletionOfSession = true;
        }
        break;
      }
    }
  });

  $effect(() => {
    const interval = setInterval(() => { stallTick++; }, 5000);
    return () => clearInterval(interval);
  });


  $effect(() => {
    if (mediaPreview) {
      previewImageLoading = true;
    }
  });

  $effect(() => {
    const incoming = pendingExternalPrefill;
    if (!incoming || incoming.id === lastExternalPrefillId) {
      return;
    }

    lastExternalPrefillId = incoming.id;
    clearPendingExternalPrefill(incoming.id);
    externalNotice = incoming;

    if (incoming.action !== "prefill") {
      return;
    }

    url = incoming.url;
    if (getSettings()?.download.auto_download_on_paste) {
      pendingAutoDownload = true;
    }
    handleInput();
  });

  let mascotEmotion = $derived.by((): "idle" | "downloading" | "error" | "stalled" | "queue" | "complete" | "amazed" => {
    void stallTick;

    if (addedNoticeActive) {
      return "complete";
    }

    if (lastCompletionAt > 0 && Date.now() - lastCompletionAt < 5000) {
      return firstCompletionOfSession && completionSeenIds.size === 1 ? "amazed" : "complete";
    }

    if (downloadboxState.kind === "preparing") return "downloading";
    if (downloadboxState.kind === "error") return "error";

    let hasActiveDownloading = false;
    let hasActiveStalled = false;
    let hasItems = false;
    for (const item of downloads.values()) {
      hasItems = true;
      if (item.status === "downloading") {
        hasActiveDownloading = true;
        const elapsed = Date.now() - item.lastUpdateAt;
        if (elapsed > STALL_THRESHOLD) {
          hasActiveStalled = true;
        }
      }
    }

    if (hasActiveStalled) return "stalled";
    if (hasActiveDownloading) return "downloading";
    if (hasItems) return "queue";
    return "idle";
  });

  let mascotCompact = $derived(advancedMode);
  let isStage = $derived(!advancedMode);

  let showDownloadbox = $derived(showDownloadboxForState(downloadboxState));
  let showInspector = $derived(showInspectorForState(downloadboxState));

  function isValidTimeBound(v: string): boolean {
    return /^(\d+:)?\d{1,2}:\d{1,2}(\.\d+)?$|^\d+(\.\d+)?$/.test(v.trim());
  }

  function buildTimeRange(): string | null {
    const s = clipStart.trim();
    const e = clipEnd.trim();
    if (!s && !e) return null;
    if (s && !isValidTimeBound(s)) return null;
    if (e && !isValidTimeBound(e)) return null;
    return `${s || "0"}-${e || "inf"}`;
  }

  function toEpochMs(v: string): number | null {
    if (!v) return null;
    const ms = new Date(v.includes("T") ? v : `${v}T00:00`).getTime();
    if (Number.isNaN(ms)) return null;
    return ms;
  }

  function schedulePart(v: string, part: "date" | "time"): string {
    if (!v) return "";
    const [date, time = ""] = v.split("T");
    return part === "date" ? date : time;
  }

  function withSchedulePart(current: string, part: "date" | "time", value: string): string {
    const pad = (n: number) => String(n).padStart(2, "0");
    let [date, time = ""] = current ? current.split("T") : ["", ""];
    if (part === "date") date = value;
    else time = value;
    if (!date && !time) return "";
    if (!date) {
      const now = new Date();
      date = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
    }
    if (!time) time = "00:00";
    return `${date}T${time}`;
  }

  function setSchedulePreset(kind: "1h" | "tonight" | "1d") {
    const d = new Date();
    if (kind === "1h") d.setHours(d.getHours() + 1);
    else if (kind === "1d") d.setDate(d.getDate() + 1);
    else {
      d.setHours(22, 0, 0, 0);
      if (d.getTime() < Date.now()) d.setDate(d.getDate() + 1);
    }
    const pad = (n: number) => String(n).padStart(2, "0");
    scheduleAt = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  function handleInput() {
    if (debounceTimer) clearTimeout(debounceTimer);
    if (addedNoticeTimer) {
      clearTimeout(addedNoticeTimer);
      addedNoticeTimer = null;
    }
    addedNoticeActive = false;
    clearMediaPreview();
    clipStart = "";
    clipEnd = "";
    scheduleAt = "";
    scheduleStop = "";
    cookieHint = null;
    playlistEntries = [];
    selectedPlaylistItems = new Set();
    playlistLoading = false;
    torrentEntries = [];
    selectedTorrentFiles = new Set();
    torrentLoading = false;
    const currentSettings = getSettings();
    const saved = currentSettings?.last_download_options;
    const savedMode = saved?.mode;
    const settingsQuality = currentSettings?.download.video_quality;
    if (settingsQuality === "audio") {
      selectedQuality = "best";
      downloadMode = "audio";
    } else {
      selectedQuality = settingsQuality && typeof settingsQuality === "string"
        ? settingsQuality
        : "best";
      downloadMode = savedMode === "audio" || savedMode === "mute" ? savedMode : "auto";
    }
    selectedFormatId = null;
    formats = [];
    loadingFormats = false;
    formatError = null;
    formatFetchGeneration++;
    referer = "";

    const trimmed = url.trim();
    if (!trimmed) {
      downloadboxState = { kind: "idle" };
      return;
    }

    const urls = extractUrls(trimmed);

    if (urls.length > 1) {
      downloadboxState = { kind: "batch", urls };
      return;
    }

    if (urls.length === 1) {
      const targetUrl = urls[0];
      if (url !== targetUrl) {
        url = targetUrl;
      }
      // A new link starts in clip mode; the user picks playlist/mix
      // again for this link instead of inheriting the old choice.
      listChoice = "clip";
      downloadboxState = { kind: "detecting" };
      if (getSettings()?.download.auto_download_on_paste) {
        pendingAutoDownload = true;
      }
      debounceTimer = setTimeout(() => {
        detectPlatform(targetUrl);
      }, 120);
      return;
    }

    if (trimmed.length >= 2) {
      downloadboxState = { kind: "searching" };
      debounceTimer = setTimeout(() => {
        performSearch(trimmed);
      }, 600);
    } else {
      downloadboxState = { kind: "idle" };
    }
  }

  async function detectPlatform(value: string) {
    try {
      const result = await invoke<PlatformInfo>("detect_platform", { url: value });
      if (url.trim() !== value) return;
      if (result.supported) {
        loadCookieAccounts(value);
        if (result.content_type === "playlist" && !videoOrListLink(value)) {
          loadPlaylistEntries(value);
        }
        if (isTorrentUrl(value)) {
          loadTorrentContents(value);
        }
        const shouldPrefetch =
          !COURSE_PLATFORMS.has(result.platform) &&
          result.platform !== "direct_file" &&
          result.platform !== "p2p" &&
          !isTorrentUrl(value);
        if (shouldPrefetch) {
          invoke("prefetch_media_info", { url: value }).catch(() => {});
          const startedAt = Date.now();
          while (
            url.trim() === value &&
            getMediaPreview()?.url !== value &&
            Date.now() - startedAt < 2200
          ) {
            await new Promise((r) => setTimeout(r, 50));
          }
        }
        if (url.trim() !== value) return;
        downloadboxState = { kind: "detected", info: result };
        const docs = documentFormatsFor(result.platform);
        selectedFormatId = docs ? docs[0] : null;
      } else {
        downloadboxState = { kind: "unsupported" };
      }
    } catch {
      if (url.trim() === value) {
        downloadboxState = { kind: "unsupported" };
      }
    }
  }

  async function loadPlaylistEntries(targetUrl: string) {
    playlistEntries = [];
    selectedPlaylistItems = new Set();
    playlistLoading = true;
    try {
      const entries = await invoke<{ index: number; title: string; url: string }[]>(
        "playlist_entries",
        { url: targetUrl }
      );
      playlistEntries = entries;
      selectedPlaylistItems = new Set(entries.map((e) => e.index));
    } catch {
      playlistEntries = [];
    } finally {
      playlistLoading = false;
    }
  }

  function togglePlaylistItem(idx: number) {
    const next = new Set(selectedPlaylistItems);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    selectedPlaylistItems = next;
  }

  function selectAllPlaylist() {
    selectedPlaylistItems = new Set(playlistEntries.map((e) => e.index));
  }

  function selectNonePlaylist() {
    selectedPlaylistItems = new Set();
  }

  function chooseYoutubeMode(next: "clip" | "playlist" | "mix") {
    listChoice = next;
    if (next !== "clip" && playlistEntries.length === 0 && !playlistLoading) {
      loadPlaylistEntries(url.trim());
    }
  }

  function wantsList(info: PlatformInfo): boolean {
    return info.content_type === "playlist" && (!videoOrList || listChoice !== "clip");
  }

  function isTorrentUrl(value: string): boolean {
    const v = value.trim().toLowerCase();
    return v.startsWith("magnet:") || v.endsWith(".torrent");
  }

  async function loadTorrentContents(targetUrl: string) {
    torrentEntries = [];
    selectedTorrentFiles = new Set();
    torrentLoading = true;
    try {
      const entries = await invoke<{ index: number; path: string; size_bytes: number }[]>(
        "torrent_contents",
        { url: targetUrl }
      );
      torrentEntries = entries;
      selectedTorrentFiles = new Set(entries.map((e) => e.index));
    } catch {
      torrentEntries = [];
    } finally {
      torrentLoading = false;
    }
  }

  function toggleTorrentFile(idx: number) {
    const next = new Set(selectedTorrentFiles);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    selectedTorrentFiles = next;
  }

  function selectAllTorrent() {
    selectedTorrentFiles = new Set(torrentEntries.map((e) => e.index));
  }

  function selectNoneTorrent() {
    selectedTorrentFiles = new Set();
  }

  async function loadCookieAccounts(targetUrl: string) {
    try {
      const result = await invoke<{ domain: string; accounts: CookieAccount[] }>(
        "cookies_accounts_for_url",
        { url: targetUrl }
      );
      const accounts = result.accounts.slice().sort((a, b) => {
        const lhs = a.last_used_at_ms ?? a.captured_at_ms;
        const rhs = b.last_used_at_ms ?? b.captured_at_ms;
        return rhs - lhs;
      });
      cookieAccounts = accounts;
      selectedCookieSlug = accounts[0]?.slug ?? null;
      cookieHint = null;
      if (accounts.length > 0) {
        try {
          const h = await invoke<{ items: { domain: string; slug: string; status: string }[] }>(
            "cookies_health"
          );
          const slug = selectedCookieSlug ?? "_default";
          const item =
            h.items.find((it) => it.domain === result.domain && it.slug === slug) ??
            h.items.find((it) => it.domain === result.domain);
          if (item && item.status !== "fresh") {
            cookieHint = item.status === "expired" ? "expired" : "stale";
          }
        } catch {}
      }
    } catch {
      cookieAccounts = [];
      selectedCookieSlug = null;
      cookieHint = null;
    }
  }

  async function performSearch(query: string) {
    try {
      const results = await invoke<SearchResult[]>("search_videos", {
        query,
        platform: "youtube",
        maxResults: 6,
      });
      if (url.trim() !== query) return;
      if (results.length > 0) {
        downloadboxState = { kind: "search-results", results };
      } else {
        downloadboxState = { kind: "search-empty" };
      }
    } catch {
      if (url.trim() === query) {
        downloadboxState = { kind: "search-empty" };
      }
    }
  }

  function selectSearchResult(result: SearchResult) {
    url = result.url;
    downloadboxState = { kind: "detecting" };
    detectPlatform(result.url);
  }

  function getContentTypeLabel(contentType: string | null): string {
    if (!contentType) return $t("downloadbox.content_type.unknown");
    const key = `downloadbox.content_type.${contentType}`;
    const result = $t(key);
    if (result === key) return $t("downloadbox.content_type.unknown");
    return result;
  }


  async function loadFormats() {
    if (loadingFormats) return;
    if (formats.length > 0) {
      formats = [];
      selectedFormatId = null;
      formatError = null;
      return;
    }
    const targetUrl = url.trim();
    if (!targetUrl) {
      formatError = $t("downloadbox.formats_error");
      return;
    }
    loadingFormats = true;
    formatError = null;
    const gen = ++formatFetchGeneration;
    try {
      const result = await invoke<FormatInfo[]>("get_media_formats", { url: targetUrl });
      if (gen !== formatFetchGeneration) return;
      formats = result;
      if (result.length === 0) {
        formatError = $t("downloadbox.no_formats");
      }
    } catch (e: any) {
      if (gen !== formatFetchGeneration) return;
      formats = [];
      const msg = typeof e === "string" ? e : e.message ?? "";
      formatError = msg ? translateBackendError(msg, $t) : $t("downloadbox.formats_error");
    } finally {
      loadingFormats = false;
    }
  }

  function selectFormat(formatId: string) {
    selectedFormatId = formatId;
  }

  /** Photo list / slideshow / audio option: download just that item
   * (label), or the whole photo post when label is null. */
  function downloadPhotoOption(label: string | null) {
    selectedFormatId = label;
    void handleAction();
  }

  function clearFormatSelection() {
    selectedFormatId = null;
  }

  function presetBest() {
    selectedFormatId = null;
    downloadMode = "auto";
    selectedQuality = "best";
  }

  function presetMusic() {
    selectedFormatId = null;
    downloadMode = "audio";
  }

  function persistLastDownloadOptions() {
    const saved = getSettings()?.last_download_options;
    const nextMode = downloadMode;
    if (saved?.mode === nextMode) return;
    updateSettings({
      last_download_options: {
        mode: nextMode,
        quality: saved?.quality ?? "best",
      },
    }).catch(() => {});
  }

  async function handleAction() {
    if (downloadboxState.kind !== "detected") return;
    const info = downloadboxState.info;

    if (COURSE_PLATFORMS.has(info.platform)) {
      goto(`/courses/${encodeURIComponent(info.platform)}`);
      return;
    }

    if (info.platform === "p2p") {
      const trimmed = url.trim();
      const code = trimmed.replace(/^p2p:/, "");
      p2pReceiveUrl = trimmed;
      p2pReceiveCode = code;
      return;
    }

    const isPlaylist = wantsList(info) && playlistEntries.length > 0;
    if (isPlaylist && selectedPlaylistItems.size === 0) {
      showToast("error", $t("downloadbox.playlist_none_selected") as string);
      return;
    }

    const isTorrent = torrentEntries.length > 0;
    if (isTorrent && selectedTorrentFiles.size === 0) {
      showToast("error", $t("downloadbox.torrent_none_selected") as string);
      return;
    }

    const settings = getSettings();
    const hasExplicitOutputDir = !!selectedOutputDir;
    let outputDir = selectedOutputDir || settings?.download.default_output_dir || "";

    if (
      (!hasExplicitOutputDir &&
        settings?.download.always_ask_path &&
        !settings?.download.auto_download_on_paste) ||
      !outputDir
    ) {
      const selected = await open({
        directory: true,
        title: $t("settings.download.default_output_dir"),
      });
      if (!selected) return;
      outputDir = selected as string;
    }

    const currentUrl = url.trim();
    const platform = info.platform;

    // B40: the user's rule decides before we ask again. It only
    // fills in what he didn't explicitly choose this session — a rule
    // can't override the choice made just now, in front of him.
    let ruleQuality = selectedQuality;
    try {
      const hit = await invoke<{ name: string; then: { output_dir?: string | null; quality?: string | null } } | null>(
        "preview_rule_match",
        { url: currentUrl, platform },
      );
      if (hit) {
        if (hit.then.output_dir) outputDir = hit.then.output_dir;
        if (hit.then.quality && !selectedQuality) ruleQuality = hit.then.quality;
        showToast("info", $t("downloadbox.rule_applied", { name: hit.name }) as string);
      }
    } catch {
      // A rule is a convenience: if it fails, the download goes on with the manual choices.
    }

    // Spotify picks its artifact by `quality` ("30s preview" / "cover art"),
    // not by `format_id` like the other native downloaders — pass the picked
    // format through so choosing "cover art" really downloads the cover
    // instead of silently falling back to the preview.
    const qualityForDownload =
      platform === "spotify" && selectedFormatId ? selectedFormatId : ruleQuality;

    // B39: compare with what this URL was last time, before overwriting.
    // After the download it's too late: the old file is already gone.
    const snapshot = {
      duration_secs: mediaPreview?.duration_seconds ?? null,
      chapters: [],
      sha256: null,
      title: mediaPreview?.title ?? null,
    };
    try {
      const mudou = await invoke<string | null>("check_media_changed", {
        url: currentUrl,
        current: snapshot,
      });
      if (mudou) {
        showToast("info", $t("downloadbox.media_changed", { summary: mudou }) as string);
      }
    } catch {
      // The warning is a courtesy: if it fails, the download goes on as it always has.
    }
    void invoke("record_media_snapshot", { url: currentUrl, snapshot }).catch(() => {});

    downloadboxState = { kind: "preparing", platform };
    url = "";

    try {
      await invoke<DownloadStarted>("download_from_url", {
        url: currentUrl,
        outputDir,
        downloadMode: downloadMode === "auto" ? null : downloadMode,
        quality: qualityForDownload,
        formatId: selectedFormatId,
        referer: referer.trim() || null,
        cookieSlug: selectedCookieSlug,
        timeRange: buildTimeRange(),
        playlistItems: isPlaylist ? [...selectedPlaylistItems] : null,
        torrentFiles: isTorrent ? [...selectedTorrentFiles] : null,
        scheduledAt: toEpochMs(scheduleAt),
        stopAt: toEpochMs(scheduleStop),
      });
      persistLastDownloadOptions();
      clearMediaPreview();
      downloadboxState = { kind: "idle" };
      addedNoticeActive = true;
      if (addedNoticeTimer) clearTimeout(addedNoticeTimer);
      addedNoticeTimer = setTimeout(() => {
        addedNoticeActive = false;
      }, 3000);
    } catch (e: any) {
      const msg = typeof e === "string" ? e : e.message ?? $t("downloadbox.error");
      downloadboxState = {
        kind: "error",
        message: msg,
        originalUrl: currentUrl,
        platform,
      };
    }
  }

  type PreflightReport = {
    total: number;
    ready: number;
    verdict: "go" | "go_with_skips" | "stop";
    problems: { url: string; problem: string | null }[];
  };

  async function handleBatchDownload() {
    if (downloadboxState.kind !== "batch") return;
    const batchUrls = downloadboxState.urls;

    const settings = getSettings();
    const hasExplicitOutputDir = !!selectedOutputDir;
    let outputDir = selectedOutputDir || settings?.download.default_output_dir || "";

    if (
      (!hasExplicitOutputDir &&
        settings?.download.always_ask_path &&
        !settings?.download.auto_download_on_paste) ||
      !outputDir
    ) {
      const selected = await open({
        directory: true,
        title: $t("settings.download.default_output_dir"),
      });
      if (!selected) return;
      outputDir = selected as string;
    }

    // B34: check before queueing. Without this, an unsupported or
    // duplicate URL enters the queue and only fails later, one by one — the user
    // finds out item by item what could have been known all at once.
    let paraBaixar = batchUrls;
    try {
      const report = await invoke<PreflightReport>("preflight_batch", {
        urls: batchUrls,
        outputDir,
      });
      if (report.verdict === "stop") {
        showToast("error", $t("downloadbox.preflight_stop", { total: report.total }) as string);
        return;
      }
      if (report.problems.length > 0) {
        const ruins = new Set(report.problems.map(p => p.url));
        paraBaixar = batchUrls.filter(u => !ruins.has(u));
        showToast("info", $t("downloadbox.preflight_skips", {
          skipped: report.problems.length,
          total: report.total,
        }) as string);
      }
    } catch {
      // The check is a courtesy, not a gate: if it fails, the batch
      // goes on as it did before.
    }

    downloadboxState = { kind: "idle" };
    url = "";

    const results = await Promise.allSettled(
      paraBaixar.map(u => invoke<DownloadStarted>("download_from_url", {
        url: u,
        outputDir,
        downloadMode: downloadMode === "auto" ? null : downloadMode,
        quality: selectedQuality,
        formatId: null,
        referer: null,
        cookieSlug: null,
      }))
    );

    const queued = results.filter(r => r.status === "fulfilled").length;
    if (queued > 0) {
      showToast("info", $t("downloadbox.batch_queued", { count: queued }));
      persistLastDownloadOptions();
    }
  }

  function handleRetry() {
    if (downloadboxState.kind !== "error") return;
    url = downloadboxState.originalUrl;
    downloadboxState = { kind: "detecting" };
    detectPlatform(url.trim());
  }

  async function handleP2pAccept() {
    const currentUrl = p2pReceiveUrl;
    p2pReceiveCode = null;
    p2pReceiveUrl = "";

    const settings = getSettings();
    const hasExplicitOutputDir = !!selectedOutputDir;
    let outputDir = selectedOutputDir || settings?.download.default_output_dir || "";

    if (
      (!hasExplicitOutputDir &&
        settings?.download.always_ask_path &&
        !settings?.download.auto_download_on_paste) ||
      !outputDir
    ) {
      const selected = await open({
        directory: true,
        title: $t("settings.download.default_output_dir"),
      });
      if (!selected) return;
      outputDir = selected as string;
    }

    downloadboxState = { kind: "preparing", platform: "p2p" };
    url = "";

    try {
      await invoke("download_from_url", {
        url: currentUrl,
        outputDir,
        downloadMode: null,
        quality: "best",
        formatId: null,
        referer: null,
        cookieSlug: null,
      });
      downloadboxState = { kind: "idle" };
    } catch (e: any) {
      const msg = typeof e === "string" ? e : e.message ?? $t("downloadbox.error");
      downloadboxState = { kind: "error", message: msg, originalUrl: currentUrl, platform: "p2p" };
    }
  }

  function handleP2pReject() {
    p2pReceiveCode = null;
    p2pReceiveUrl = "";
  }

  async function openTorrentFile() {
    const selected = await open({
      title: "Select .torrent file",
      filters: [{ name: "Torrent", extensions: ["torrent"] }],
      multiple: false,
    });
    if (selected && typeof selected === "string") {
      url = selected;
      handleInput();
    }
  }

  async function openBatchFile() {
    const selected = await open({
      title: $t("downloadbox.batch_file_title"),
      filters: [{ name: "Text", extensions: ["txt"] }],
      multiple: false,
    });
    if (!selected || typeof selected !== "string") return;
    try {
      const urls = await invoke<string[]>("parse_batch_file", { path: selected });
      if (urls.length === 0) {
        showToast("info", $t("downloadbox.batch_file_empty"));
        return;
      }
      if (urls.length === 1) {
        url = urls[0];
        handleInput();
        return;
      }
      url = urls.join("\n");
      downloadboxState = { kind: "batch", urls };
    } catch (e: any) {
      const msg = typeof e === "string" ? e : e.message ?? $t("downloadbox.error");
      showToast("error", msg);
    }
  }

  function handleHomeModeChange(mode: HomeInputMode) {
    // batch/torrent/p2p are momentary actions, not persistent input modes:
    // return to "url" so the tab bar never strands on an empty state when
    // the picker/dialog is cancelled
    if (mode === "p2p") {
      showP2pSendDialog = true;
      homeInputMode = "url";
    } else if (mode === "torrent") {
      void openTorrentFile().finally(() => {
        homeInputMode = "url";
      });
    } else if (mode === "batch") {
      void openBatchFile().finally(() => {
        homeInputMode = "url";
      });
    }
  }

  function handleHomeMoreAction(action: MoreAction) {
    if (action === "advanced") {
      advancedMode = true;
    } else {
      handleHomeModeChange(action);
    }
  }

  function handleSubmit() {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    const trimmed = url.trim();
    if (!trimmed) return;
    if (downloadboxState.kind === "detected") {
      void handleAction();
    } else {
      const urls = extractUrls(trimmed);
      if (urls.length === 1) {
        downloadboxState = { kind: "detecting" };
        detectPlatform(urls[0]);
      }
    }
  }

  function handleDismiss() {
    clearMediaPreview();
    downloadboxState = { kind: "idle" };
    url = "";
  }
</script>

<div class="home-mac" class:home-mac--stage={isStage}>
  <div class="home-backdrop" aria-hidden="true"></div>
  {#if advancedMode}
    <div class="home-mac-workspace">
      <div class="home-mac-hero">
        <HomeHero
          emotion={mascotEmotion}
          compact={mascotCompact}
          celebrate={mascotEmotion === "amazed"}
        />
      </div>
      <div class="home-mac-main">
        <div class="home-secondary home-secondary--start">
          <button type="button" class="home-secondary-link" onclick={() => { advancedMode = false; }}>
            {$t('home.action_simple')}
          </button>
        </div>
        <DownloadboxAdvanced />
      </div>
    </div>
  {:else}
    <div class="home-stage">
      <div class="home-stage-inner">
        <div class="home-stage-hero" class:home-stage-hero--detecting={downloadboxState.kind === "detecting"}>
          <HomeHero
            emotion={mascotEmotion}
            stage
            celebrate={mascotEmotion === "amazed" || addedNoticeActive}
          />
        </div>

        <div class="home-mac-main">
          {#if externalNotice}
            <div class="feedback-card feedback-enter external-url-card">
              <div class="card-row">
                <svg class="card-status-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M12 5v14" />
                  <path d="M5 12h14" />
                </svg>
                <span class="card-title">{$t('downloadbox.external_url_ready')}</span>
                <button class="dismiss-btn" onclick={() => { externalNotice = null; }} aria-label={$t('common.close')}>
                  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M18 6L6 18M6 6l12 12" />
                  </svg>
                </button>
              </div>
              <div class="card-row">
                <span class="card-subtext external-url-text">{externalNotice.url}</span>
              </div>
            </div>
          {/if}

          {#if showDownloadbox}
            <HomeUrlBar
              bind:url
              bind:mode={homeInputMode}
              bind:inputEl={urlInput}
              busy={downloadboxState.kind === "detecting" || downloadboxState.kind === "searching"}
              busyLabel={downloadboxState.kind === "detecting" ? $t('downloadbox.detecting') : ""}
              showSubmit={downloadboxState.kind === "detected" || (downloadboxState.kind === "idle" && url.trim().length > 0)}
              onInput={handleInput}
              onSubmit={handleSubmit}
              onMore={handleHomeMoreAction}
              onModeChange={handleHomeModeChange}
              onAdvanced={() => { advancedMode = true; }}
            />
          {/if}

          {#if downloadboxState.kind === "detected"}
            <div class="feedback feedback-enter" data-supported="true">
              <span class="feedback-text">{platformDisplayName(downloadboxState.info.platform)}</span>
            </div>

          {:else if downloadboxState.kind === "idle" && addedNoticeActive}
            <div class="feedback feedback-enter added-notice">
              <strong class="added-notice-title">{$t('downloadbox.added_to_downloads')}</strong>
              <button type="button" class="added-notice-link" onclick={() => goto("/downloads")}>
                {$t('downloadbox.open_downloads')}
              </button>
            </div>

          {:else if downloadboxState.kind === "idle" && !addedNoticeActive}
            {#if dlStats.totalDownloads > 0}
              <a href="/downloads" class="stats-line">
                {$t("home.saved_line", {
                  files: $t(pluralKey("home.saved_count", dlStats.totalDownloads), { count: dlStats.totalDownloads.toLocaleString() }),
                  size: formatBytes(dlStats.totalBytes),
                })}
              </a>
            {/if}

          {:else if downloadboxState.kind === "batch"}
            <div class="batch-options">
              <DownloadModeSelector bind:downloadMode />
              <BatchDownload count={downloadboxState.urls.length} onDownload={handleBatchDownload} />
            </div>

          {:else if downloadboxState.kind === "searching"}
            <div class="feedback feedback-enter">
              <span class="feedback-spinner"></span>
              <span class="feedback-text search-hint">{$t('downloadbox.searching')}</span>
            </div>

          {:else if downloadboxState.kind === "search-results"}
            <SearchResults results={downloadboxState.results} onSelect={selectSearchResult} />

          {:else if downloadboxState.kind === "search-empty"}
            <div class="feedback feedback-enter" data-supported="false">
              <svg class="feedback-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="11" cy="11" r="8" />
                <path d="M21 21l-4.35-4.35" />
              </svg>
              <span class="feedback-text">{$t('downloadbox.search_empty')}</span>
            </div>

          {:else if downloadboxState.kind === "unsupported"}
            <div class="feedback feedback-enter" data-supported="false">
              <svg class="feedback-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10" />
                <path d="M12 8v4m0 4h.01" />
              </svg>
              <span class="feedback-text">{$t('downloadbox.unsupported')}</span>
            </div>
          {/if}

          <HomeInspector open={showInspector} title={$t('home.inspector_title')}>
            {#if downloadboxState.kind === "detected"}
              <FormatSelector
                platform={downloadboxState.info.platform}
                isPlaylist={downloadboxState.info.content_type === "playlist"}
                bind:formats
                bind:selectedFormatId
                {loadingFormats}
                {formatError}
                onLoadFormats={loadFormats}
                onSelectFormat={selectFormat}
                onClearFormat={clearFormatSelection}
                onPresetBest={presetBest}
                onPresetMusic={presetMusic}
              />
              <MediaPreview bind:mediaPreview bind:imageLoading={previewImageLoading} />
              {#if mediaPreview && (mediaPreview.media_type === "photo" || mediaPreview.media_type === "carousel" || mediaPreview.media_type === "gif")}
                <PhotoList qualities={mediaPreview.qualities} onDownload={downloadPhotoOption} />
              {/if}
              {#if mediaPreview && mediaPreview.media_type === "video"}
                <VideoWatermarkChoice qualities={mediaPreview.qualities} onDownload={downloadPhotoOption} />
              {/if}
              {#if downloadboxState.info.content_type === "playlist" && videoOrList}
                <VideoOrListChoice
                  choice={listChoice}
                  isMix={videoOrList.isMix}
                  onChoose={chooseYoutubeMode}
                />
              {/if}
              {#if wantsList(downloadboxState.info)}
                <div class="playlist-picker">
                  <div class="playlist-head">
                    <span class="playlist-count">
                      {$t('downloadbox.playlist_selected', { selected: selectedPlaylistItems.size, total: playlistEntries.length })}
                    </span>
                    {#if !playlistLoading && playlistEntries.length > 0}
                      <div class="playlist-bulk">
                        <button type="button" class="playlist-link" onclick={selectAllPlaylist}>{$t('downloadbox.playlist_all')}</button>
                        <button type="button" class="playlist-link" onclick={selectNonePlaylist}>{$t('downloadbox.playlist_none')}</button>
                      </div>
                    {/if}
                  </div>
                  {#if playlistLoading}
                    <div class="playlist-status"><span class="feedback-spinner"></span> {$t('downloadbox.playlist_loading')}</div>
                  {:else if playlistEntries.length === 0}
                    <span class="playlist-status">{$t('downloadbox.playlist_empty')}</span>
                  {:else}
                    <ul class="playlist-list">
                      {#each playlistEntries as entry (entry.index)}
                        <li>
                          <label class="playlist-item">
                            <input type="checkbox" checked={selectedPlaylistItems.has(entry.index)} onchange={() => togglePlaylistItem(entry.index)} />
                            <span class="playlist-idx">{entry.index}.</span>
                            <span class="playlist-title">{entry.title}</span>
                          </label>
                        </li>
                      {/each}
                    </ul>
                  {/if}
                </div>
              {/if}
              {#if torrentLoading || torrentEntries.length > 0}
                <div class="playlist-picker">
                  <div class="playlist-head">
                    <span class="playlist-count">
                      {$t('downloadbox.torrent_selected', { selected: selectedTorrentFiles.size, total: torrentEntries.length })}
                    </span>
                    {#if !torrentLoading && torrentEntries.length > 0}
                      <div class="playlist-bulk">
                        <button type="button" class="playlist-link" onclick={selectAllTorrent}>{$t('downloadbox.playlist_all')}</button>
                        <button type="button" class="playlist-link" onclick={selectNoneTorrent}>{$t('downloadbox.playlist_none')}</button>
                      </div>
                    {/if}
                  </div>
                  {#if torrentLoading}
                    <div class="playlist-status"><span class="feedback-spinner"></span> {$t('downloadbox.torrent_loading')}</div>
                  {:else}
                    <ul class="playlist-list">
                      {#each torrentEntries as entry (entry.index)}
                        <li>
                          <label class="playlist-item">
                            <input type="checkbox" checked={selectedTorrentFiles.has(entry.index)} onchange={() => toggleTorrentFile(entry.index)} />
                            <span class="playlist-title">{entry.path}</span>
                            <span class="torrent-size">{formatBytes(entry.size_bytes)}</span>
                          </label>
                        </li>
                      {/each}
                    </ul>
                  {/if}
                </div>
              {/if}
              {#if cookieHint}
                <p class="cookie-hint" class:expired={cookieHint === "expired"} role="status">
                  <span>{cookieHint === "expired" ? $t("downloadbox.cookie_hint_expired") : $t("downloadbox.cookie_hint_stale")}</span>
                  <button type="button" class="cookie-hint-link" onclick={() => goto("/settings?tab=cookies")}>{$t("downloadbox.cookie_hint_action")}</button>
                </p>
              {/if}
              {#if COURSE_PLATFORMS.has(downloadboxState.info.platform)}
                {#if coursesPluginInstalled === false}
                  <p class="course-upsell">{$t('downloadbox.courses_plugin_needed')}</p>
                  <button class="download-primary-btn" onclick={() => goto("/tools")}>{$t('downloadbox.install_courses_plugin')}</button>
                {:else}
                  <p class="course-upsell">{$t('downloadbox.courses_plugin_ready')}</p>
                  <button class="download-primary-btn" onclick={handleAction}>{$t(downloadboxState.info.platform === "udemy" ? 'downloadbox.go_to_udemy' : 'downloadbox.go_to_hotmart')}</button>
                {/if}
              {:else}
                {@const playlistBlocked = wantsList(downloadboxState.info) && playlistEntries.length > 0 && selectedPlaylistItems.size === 0}
                {@const torrentBlocked = torrentEntries.length > 0 && selectedTorrentFiles.size === 0}
                {#if downloadboxState.info.platform === "bilibili"}
                  <BilibiliPreviewExtras {url} accountSlug={selectedCookieSlug && selectedCookieSlug !== "_anonymous" ? selectedCookieSlug : null} />
                {/if}
                <button class="download-primary-btn" disabled={playlistBlocked || torrentBlocked} onclick={handleAction}>{$t('downloadbox.download')}</button>
                {#if downloadboxState.info.platform !== "direct_file"}
                  <details class="options-panel">
                    <summary class="options-toggle">{$t('downloadbox.options')}</summary>
                    <div class="options-content">
                      {#if !docFormats}
                        <DownloadModeSelector bind:downloadMode onChange={() => { selectedFormatId = null; }} />
                        {#if downloadMode !== "audio" && downloadboxState.info.platform !== "spotify"}
                          <QualityPicker bind:selectedQuality {selectedFormatId} {availableHeights} {hasAudioOnly} />
                        {/if}
                      {/if}
                      <OutputLocationPicker bind:selectedOutputDir />
                      {#if cookieAccounts.length > 1}
                        <CookieAccountPicker accounts={cookieAccounts} bind:selectedSlug={selectedCookieSlug} />
                      {/if}
                      <details class="options-panel">
                        <summary class="options-toggle">{$t('downloadbox.advanced')}</summary>
                        <div class="options-content">
                          {#if downloadboxState.info.platform === "vimeo" || downloadboxState.info.platform === "generic"}
                            <div class="referer-input-wrapper">
                              <label class="referer-label" for="referer-input">{$t('downloadbox.referer_label')}</label>
                              <input id="referer-input" class="referer-input" type="text" placeholder={$t('downloadbox.referer_placeholder')} bind:value={referer} spellcheck="false" />
                            </div>
                          {/if}
                          {#if downloadboxState.info.content_type !== "playlist"}
                            <div class="timerange-wrapper">
                              <span class="timerange-label">{$t('downloadbox.timerange_label')}</span>
                              <div class="timerange-inputs">
                                <input class="timerange-input" type="text" placeholder={$t('downloadbox.timerange_start')} bind:value={clipStart} spellcheck="false" inputmode="numeric" aria-label={$t('downloadbox.timerange_start') as string} />
                                <span class="timerange-sep" aria-hidden="true">—</span>
                                <input class="timerange-input" type="text" placeholder={$t('downloadbox.timerange_end')} bind:value={clipEnd} spellcheck="false" inputmode="numeric" aria-label={$t('downloadbox.timerange_end') as string} />
                              </div>
                              <span class="timerange-hint">{$t('downloadbox.timerange_hint')}</span>
                            </div>
                          {/if}
                          <div class="timerange-wrapper">
                            <span class="timerange-label">{$t('downloadbox.schedule_label')}</span>
                            <div class="schedule-presets">
                              <button type="button" class="schedule-preset" onclick={() => setSchedulePreset('1h')}>{$t('downloadbox.schedule_1h')}</button>
                              <button type="button" class="schedule-preset" onclick={() => setSchedulePreset('tonight')}>{$t('downloadbox.schedule_tonight')}</button>
                              <button type="button" class="schedule-preset" onclick={() => setSchedulePreset('1d')}>{$t('downloadbox.schedule_1d')}</button>
                              {#if scheduleAt || scheduleStop}
                                <button type="button" class="schedule-preset" onclick={() => { scheduleAt = ""; scheduleStop = ""; }}>{$t('downloadbox.schedule_clear')}</button>
                              {/if}
                            </div>
                            <div class="timerange-inputs schedule-row">
                              <input class="timerange-input schedule-date" type="date" value={schedulePart(scheduleAt, "date")} oninput={(e) => { scheduleAt = withSchedulePart(scheduleAt, "date", e.currentTarget.value); }} aria-label={$t('downloadbox.schedule_start') as string} />
                              <input class="timerange-input schedule-time" type="time" step="60" value={schedulePart(scheduleAt, "time")} oninput={(e) => { scheduleAt = withSchedulePart(scheduleAt, "time", e.currentTarget.value); }} aria-label={$t('downloadbox.schedule_start') as string} />
                              <span class="timerange-sep" aria-hidden="true">—</span>
                              <input class="timerange-input schedule-date" type="date" value={schedulePart(scheduleStop, "date")} oninput={(e) => { scheduleStop = withSchedulePart(scheduleStop, "date", e.currentTarget.value); }} aria-label={$t('downloadbox.schedule_stop') as string} />
                              <input class="timerange-input schedule-time" type="time" step="60" value={schedulePart(scheduleStop, "time")} oninput={(e) => { scheduleStop = withSchedulePart(scheduleStop, "time", e.currentTarget.value); }} aria-label={$t('downloadbox.schedule_stop') as string} />
                            </div>
                            <span class="timerange-hint">{$t('downloadbox.schedule_hint')}</span>
                          </div>
                        </div>
                      </details>
                    </div>
                  </details>
                {/if}
              {/if}
            {:else if downloadboxState.kind === "preparing"}
              <div class="feedback-card feedback-enter">
                <div class="card-row">
                  <span class="feedback-spinner"></span>
                  <span class="card-text">{$t('downloadbox.preparing')}</span>
                </div>
              </div>
            {:else if downloadboxState.kind === "error"}
              <div class="feedback-card feedback-enter" data-status="error">
                <div class="card-row">
                  <span class="card-title card-error-text">{downloadboxState.message}</span>
                </div>
                <div class="card-row card-actions">
                  <button class="button card-action-btn" onclick={handleRetry}>{$t('downloadbox.retry')}</button>
                </div>
              </div>
            {/if}
          </HomeInspector>
        </div>
      </div>
    </div>
  {/if}

  {#if showP2pSendDialog}
    <P2pSendDialog onClose={() => { showP2pSendDialog = false; }} />
  {/if}

  {#if p2pReceiveCode}
    <P2pReceiveDialog
      code={p2pReceiveCode}
      onAccept={handleP2pAccept}
      onReject={handleP2pReject}
    />
  {/if}
</div>

<style>
  .home-mac {
    width: 100%;
  }



  .batch-options {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  /* detection line under the downloadbox */
  .feedback {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 20px;
    padding: 0 var(--space-1);
    color: var(--text-muted);
    width: 100%;
  }

  .feedback-icon {
    flex-shrink: 0;
    pointer-events: none;
  }

  .feedback[data-supported="true"] {
    color: var(--text-muted);
  }

  .feedback[data-supported="false"] {
    color: var(--text-dim);
  }

  .feedback-text {
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .added-notice {
    gap: var(--space-2);
    font-size: var(--text-sm);
  }

  .added-notice-title {
    font-weight: 600;
    color: var(--text);
  }

  .added-notice-link {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--accent-hi);
    cursor: pointer;
  }

  .added-notice-link:hover {
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .feedback-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--fill-3);
    border-top-color: var(--text-muted);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .feedback-enter {
    animation: feedbackEnter var(--duration-base) var(--ease-out);
  }

  @keyframes feedbackEnter {
    from { opacity: 0; transform: translateY(-2px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* the one primary action on the page */
  .download-primary-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    background: var(--cta);
    color: var(--on-cta);
    font-size: var(--text-md);
    font-weight: 700;
    letter-spacing: var(--track-snug);
    height: 40px;
    padding: 0 var(--space-4);
    border-radius: var(--radius-md);
    border: 2px solid var(--border-ink);
    cursor: pointer;
    width: 100%;
    box-shadow: 4px 4px 0 var(--shadow-hard);
    transition: background var(--duration-fast) var(--ease-out), transform var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out);
  }

  @media (hover: hover) {
    .download-primary-btn:hover:not(:disabled) {
      background: var(--cta-hover);
    }
  }

  .download-primary-btn:active:not(:disabled) {
    background: var(--cta-press);
    transform: translate(2px, 2px);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  .download-primary-btn:focus-visible {
    outline: var(--focus-ring);
    outline-offset: var(--focus-ring-offset);
  }

  .download-primary-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  /* playlist / torrent pickers: a grouped list */
  .playlist-picker {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-mut);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md);
    box-shadow: 4px 4px 0 var(--shadow-hard);
  }

  .playlist-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .playlist-count {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .playlist-bulk {
    display: flex;
    gap: var(--space-3);
  }

  .playlist-link {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--accent-hi);
    cursor: pointer;
  }

  .playlist-link:hover {
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .playlist-status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
    color: var(--text-dim);
  }

  .playlist-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 220px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .playlist-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 28px;
    padding: 3px var(--space-1);
    border-radius: var(--radius-xs);
    cursor: pointer;
    font-size: var(--text-base);
    color: var(--text);
  }

  .playlist-item:hover {
    background: var(--fill-1);
  }

  .playlist-idx {
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
    font-size: var(--text-sm);
  }

  .playlist-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .torrent-size {
    margin-left: auto;
    flex-shrink: 0;
    padding-left: var(--space-2);
    color: var(--text-dim);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  /* disclosure: "Options" / "Advanced" */
  .options-panel {
    width: 100%;
  }

  .options-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-muted);
    cursor: pointer;
    list-style: none;
    padding: var(--space-1) 0;
    user-select: none;
  }

  .options-toggle::-webkit-details-marker {
    display: none;
  }

  .options-toggle::marker {
    content: "";
  }

  .options-toggle::before {
    content: "";
    width: 9px;
    height: 9px;
    background: currentColor;
    clip-path: polygon(30% 10%, 75% 50%, 30% 90%, 22% 82%, 58% 50%, 22% 18%);
    transition: transform var(--duration-fast) var(--ease-out);
  }

  .options-panel[open] > .options-toggle::before {
    transform: rotate(90deg);
  }

  @media (hover: hover) {
    .options-toggle:hover {
      color: var(--text);
    }
  }

  .options-content {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-3) 0 var(--space-1);
    width: 100%;
  }

  .referer-input-wrapper,
  .timerange-wrapper {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .referer-label,
  .timerange-label {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-dim);
  }

  .referer-input,
  .timerange-input {
    height: var(--control-h);
    padding: 0 var(--space-2);
    font-size: var(--text-base);
    background: var(--control-bg);
    border: none;
    border-radius: var(--radius-sm);
    box-shadow: inset 0 0 0 var(--hairline) var(--content-border);
    color: var(--text);
    transition: box-shadow var(--duration-fast) var(--ease-out);
  }

  .referer-input::placeholder,
  .timerange-input::placeholder {
    color: var(--text-dim);
  }

  .referer-input:focus-visible,
  .timerange-input:focus-visible {
    outline: none;
    box-shadow:
      inset 0 0 0 var(--hairline) var(--accent),
      0 0 0 3px var(--accent-soft);
  }

  .timerange-inputs {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .timerange-input {
    width: 96px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  .timerange-sep {
    color: var(--text-dim);
  }

  .timerange-hint {
    font-size: var(--text-sm);
    color: var(--text-dim);
  }

  .schedule-presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 2px;
  }

  .schedule-preset {
    height: 24px;
    padding: 0 var(--space-3);
    font-size: var(--text-sm);
    font-weight: 600;
    background: var(--surface-hi);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-full);
    color: var(--text);
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out), transform var(--duration-fast) var(--ease-out);
  }

  .schedule-preset:hover {
    background: var(--surface-hi);
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  .schedule-preset:active {
    transform: translate(2px, 2px);
    box-shadow: none;
  }

  .schedule-row {
    flex-wrap: wrap;
  }

  .schedule-date {
    width: auto;
    flex: 1 1 auto;
    min-width: 8.6em;
  }

  .schedule-time {
    width: auto;
    flex: 0 1 auto;
    min-width: 5.4em;
  }

  .course-upsell {
    margin: 0;
    font-size: var(--text-base);
    line-height: var(--leading-base);
    color: var(--text-muted);
  }

  .cookie-hint {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--warning);
  }
  .cookie-hint.expired {
    color: var(--error);
  }
  .cookie-hint-link {
    background: none;
    border: 0;
    padding: 0;
    font: inherit;
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--accent-hi);
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .feedback-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    background: var(--surface-mut);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-lg);
    box-shadow: 4px 4px 0 var(--shadow-hard);
  }

  .feedback-card[data-status="error"] {
    background: color-mix(in srgb, var(--error) 10%, var(--surface-mut));
    box-shadow: inset 0 0 0 var(--hairline) color-mix(in srgb, var(--error) 30%, transparent);
  }

  .external-url-card {
    width: 100%;
    background: color-mix(in srgb, var(--accent) 10%, var(--surface-mut));
    box-shadow: inset 0 0 0 var(--hairline) color-mix(in srgb, var(--accent) 30%, transparent);
  }

  .card-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .card-text {
    font-size: var(--text-base);
    font-weight: 500;
    color: var(--text);
  }

  .card-title {
    font-size: var(--text-base);
    font-weight: 600;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .card-error-text {
    color: var(--text);
    white-space: normal;
    line-height: var(--leading-base);
  }

  .card-status-icon {
    flex-shrink: 0;
    pointer-events: none;
    color: var(--accent-hi);
  }

  .card-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .card-subtext {
    font-size: var(--text-sm);
    font-weight: 400;
    color: var(--text-dim);
  }

  .external-url-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .card-action-btn {
    height: var(--control-h);
    padding: 0 var(--space-3);
    font-size: var(--text-base);
  }

  .dismiss-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: transparent;
    border: none;
    border-radius: var(--radius-full);
    cursor: pointer;
    color: var(--text-dim);
    padding: 0;
  }

  .dismiss-btn:hover {
    background: var(--fill-2);
    color: var(--text);
  }

  .search-hint {
    color: var(--text-dim);
  }

  @media (prefers-reduced-motion: reduce) {
    .feedback-enter {
      animation: none;
    }

    .feedback-spinner {
      animation-duration: 1.5s;
    }
  }
</style>
