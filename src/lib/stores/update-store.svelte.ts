import { checkForUpdate, installUpdate, type UpdateInfo } from "$lib/updater";

let updateInfo: UpdateInfo = $state({ available: false });
let updateDismissed: boolean = $state(false);
let isUpdating: boolean = $state(false);

// Version the user chose to skip, persisted in localStorage so the app
// does not nag about that same version again after a restart. A newer
// release has a different version string, so it will prompt again.
const SKIPPED_UPDATE_VERSION_KEY = "gumballz-get-skipped-update-version";

function readSkippedVersion(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    return localStorage.getItem(SKIPPED_UPDATE_VERSION_KEY);
  } catch {
    return null;
  }
}

export function getSkippedVersion(): string | null {
  return readSkippedVersion();
}

export function skipUpdate(): void {
  if (updateInfo.version) {
    if (typeof localStorage !== "undefined") {
      try {
        localStorage.setItem(SKIPPED_UPDATE_VERSION_KEY, updateInfo.version);
      } catch {
        // Storage unavailable; the banner still hides for this session.
      }
    }
  }
  updateInfo = { available: false };
}

export function getUpdateInfo(): UpdateInfo {
  return updateInfo;
}

export function isUpdateDismissed(): boolean {
  return updateDismissed;
}

export function dismissUpdate(): void {
  updateDismissed = true;
}

export function resetUpdateDismissed(): void {
  updateDismissed = false;
}

export function getIsUpdating(): boolean {
  return isUpdating;
}

export function setUpdateInfoForTest(info: UpdateInfo): void {
  updateInfo = info;
}

export async function refreshUpdateInfo(): Promise<void> {
  const info = await checkForUpdate();
  if (info.available && info.version && info.version === readSkippedVersion()) {
    // User already skipped this version; stay quiet until a newer one arrives.
    updateInfo = { available: false };
    return;
  }
  updateInfo = info;
}

export async function performUpdate(): Promise<void> {
  if (isUpdating) return;
  isUpdating = true;
  try {
    await installUpdate();
  } catch {
    // Update failed (network, signature, or install error); stay on the
    // current version and let the user retry from the banner or Settings.
  } finally {
    isUpdating = false;
  }
}
