import {
  runGalleryDlDownload,
  cancelDownload,
  listenToDownloadProgress,
  generateDownloadId,
  type DownloadProgress,
  type DownloadResult,
} from "$lib/api/social-media";
import { parseProfileUrl } from "$lib/utils/social_media_parser";
import { dispatchMultiTierDownload } from "$lib/social-media/dispatcher";
import {
  registerGenericDownload,
  upsertGenericProgress,
  markGenericComplete,
} from "$lib/stores/download-store.svelte";

export type StatusType = "success" | "error" | "info";

export interface SocialMediaDraft {
  url: string;
  contentType: string;
  selectedProfileUrl: string | null;
}

export interface CreatorMetaSnapshot {
  name: string;
  handle: string;
  avatar?: string;
  platform: string;
  platformName?: string;
}

interface ActiveSocialMediaTask {
  queueId: number;
  downloadId: string;
  title: string;
  platform: string;
  filesDownloaded: number;
  stage: string | null;
  stageIndex: number | null;
  stageTotal: number | null;
  lastMessage: string;
  unlisten?: () => void;
}

let draft = $state<SocialMediaDraft>({
  url: "",
  contentType: "all",
  selectedProfileUrl: null,
});

const activeTasks = new Map<string, ActiveSocialMediaTask>();
let activeCount = $state(0);
let latestFilesDownloaded = $state(0);
let latestStage = $state<string | null>(null);
let latestStageIndex = $state<number | null>(null);
let latestStageTotal = $state<number | null>(null);
let latestMessage = $state("");
let status = $state("");
let statusType = $state<StatusType>("info");
let statusDetail = $state("");
let cancelling = $state(false);

export function getDraft(): SocialMediaDraft {
  return draft;
}

export function setDraft(newDraft: Partial<SocialMediaDraft>) {
  if (newDraft.url !== undefined) draft.url = newDraft.url;
  if (newDraft.contentType !== undefined) draft.contentType = newDraft.contentType;
  if (newDraft.selectedProfileUrl !== undefined) draft.selectedProfileUrl = newDraft.selectedProfileUrl;
}

export function isActive(): boolean {
  return activeCount > 0;
}

export function getDownloadId(): string | null {
  const first = activeTasks.keys().next();
  return first.done ? null : first.value;
}

export function getFilesDownloaded(): number {
  return latestFilesDownloaded;
}

export function getStage(): string | null {
  return latestStage;
}

export function getStageIndex(): number | null {
  return latestStageIndex;
}

export function getStageTotal(): number | null {
  return latestStageTotal;
}

export function getLastMessage(): string {
  return latestMessage;
}

export function getStatus(): string {
  return status;
}

export function getStatusType(): StatusType {
  return statusType;
}

export function getStatusDetail(): string {
  return statusDetail;
}

export function isCancelling(): boolean {
  return cancelling;
}

export function clearStatus() {
  status = "";
  statusType = "info";
  statusDetail = "";
}

export async function startDownload(
  url: string,
  outputDir: string,
  cookiesFile: string,
  contentType: string,
  creatorMeta?: CreatorMetaSnapshot | null,
): Promise<DownloadResult | null> {
  const parsed = parseProfileUrl(url.trim());
  const cleanUrl = parsed.cleanUrl || url.trim();
  const cleanOutputDir = outputDir.trim();
  const cleanCookiesFile = cookiesFile.trim();

  if (!cleanUrl || !cleanOutputDir) {
    status = "Please fill in all required fields";
    statusType = "error";
    statusDetail = "";
    return null;
  }

  const qId = Date.now() + Math.floor(Math.random() * 1000);
  const id = String(qId);
  const currentPlatform = creatorMeta?.platform || parsed.platform || "social-media";
  const currentTitle = creatorMeta?.name
    ? `${creatorMeta.name} (${creatorMeta.handle}) - ${creatorMeta.platformName || currentPlatform} Profile Archive`
    : `${cleanUrl} - Profile Archive`;

  registerGenericDownload(
    qId,
    currentTitle,
    currentPlatform,
    creatorMeta?.avatar || null,
    creatorMeta?.handle || null,
    "image",
  );

  const task: ActiveSocialMediaTask = {
    queueId: qId,
    downloadId: id,
    title: currentTitle,
    platform: currentPlatform,
    filesDownloaded: 0,
    stage: null,
    stageIndex: null,
    stageTotal: null,
    lastMessage: "",
  };

  activeTasks.set(id, task);
  activeCount = activeTasks.size;

  let taskUnlisten: (() => void) | null = null;
  try {
    taskUnlisten = await listenToDownloadProgress(id, (progressData: DownloadProgress) => {
      if (progressData.message && progressData.message.trim()) {
        task.lastMessage = progressData.message.trim();
        latestMessage = task.lastMessage;
      }
      if (typeof progressData.files_downloaded === "number" && progressData.files_downloaded >= 0) {
        task.filesDownloaded = progressData.files_downloaded;
        latestFilesDownloaded = task.filesDownloaded;
      }
      if (progressData.stage) {
        task.stage = progressData.stage;
        latestStage = task.stage;
      }
      if (typeof progressData.stage_index === "number") {
        task.stageIndex = progressData.stage_index;
        latestStageIndex = task.stageIndex;
      }
      if (typeof progressData.stage_total === "number") {
        task.stageTotal = progressData.stage_total;
        latestStageTotal = task.stageTotal;
      }

      const percent = 0;
      const stagePrefix = task.stageTotal && task.stageTotal > 1 && task.stageIndex !== null
        ? `[${task.stageIndex}/${task.stageTotal}] `
        : "";

      upsertGenericProgress(
        qId,
        task.title,
        task.platform,
        percent,
        0,
        0,
        null,
        stagePrefix + (progressData.message || "Archiving profile..."),
        null,
        { fileCount: task.filesDownloaded },
      );
    });
    task.unlisten = taskUnlisten;

    const result = await dispatchMultiTierDownload({
      profile: parsed,
      cleanOutputDir,
      cookiesFile: cleanCookiesFile || null,
      contentType,
      downloadId: id,
      onProgress: (msg) => {
        status = msg;
        task.lastMessage = msg;
        latestMessage = msg;
      },
    });

    if (result.cancelled) {
      markGenericComplete(qId, false, "Download cancelled", cleanOutputDir, result.files_count);
    } else if (result.success) {
      markGenericComplete(qId, true, undefined, cleanOutputDir, result.files_count);
    } else {
      const lines = (result.message || "Download failed").split("\n");
      const errHeader = lines[0] || "Download failed";
      markGenericComplete(qId, false, errHeader, cleanOutputDir, result.files_count);
    }

    return result;
  } catch (error: unknown) {
    const msg = error instanceof Error ? error.message : String(error);
    markGenericComplete(qId, false, msg, cleanOutputDir, 0);
    return null;
  } finally {
    if (taskUnlisten) {
      try {
        taskUnlisten();
      } catch {}
    }
    activeTasks.delete(id);
    activeCount = activeTasks.size;
  }
}

export async function stopDownload(targetId?: string) {
  if (targetId) {
    const task = activeTasks.get(targetId);
    if (task) {
      markGenericComplete(task.queueId, false, "Download cancelled");
      try {
        await cancelDownload(targetId);
      } catch (error) {
        console.error(`Failed to cancel download ${targetId}:`, error);
      }
    }
    return;
  }

  cancelling = true;
  for (const [id, task] of activeTasks) {
    markGenericComplete(task.queueId, false, "Download cancelled");
    try {
      await cancelDownload(id);
    } catch (error) {
      console.error(`Failed to cancel download ${id}:`, error);
    }
  }
  cancelling = false;
}
