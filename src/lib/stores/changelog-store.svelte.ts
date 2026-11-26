import { getVersion } from "@tauri-apps/api/app";
import { fetchGithubReleases } from "$lib/releases/github-client";

const STORAGE_KEY = "gumballz-get_last_seen_version";
const CHANGELOG_BODY_KEY = "gumballz-get_pending_changelog";

let showDialog = $state(false);
let changelogBody = $state("");
let currentVersion = $state("");

export function getChangelogVisible(): boolean {
  return showDialog;
}

export function getChangelogBody(): string {
  return changelogBody;
}

export function getCurrentVersion(): string {
  return currentVersion;
}

export function dismissChangelog(): void {
  showDialog = false;
  if (currentVersion) {
    localStorage.setItem(STORAGE_KEY, currentVersion);
  }
  localStorage.removeItem(CHANGELOG_BODY_KEY);
}

export function showChangelog(): void {
  showDialog = true;
}

export function storeChangelogForUpdate(body: string, version: string): void {
  localStorage.setItem(CHANGELOG_BODY_KEY, JSON.stringify({ body, version }));
}

export async function initChangelog(): Promise<void> {
  try {
    currentVersion = await getVersion();
  } catch {
    currentVersion = "1.0.1";
  }

  const lastSeen = localStorage.getItem(STORAGE_KEY);
  const pending = localStorage.getItem(CHANGELOG_BODY_KEY);

  if (pending) {
    try {
      const { body, version } = JSON.parse(pending);
      if (version === currentVersion && body) {
        changelogBody = body;
        if (lastSeen !== currentVersion) {
          showDialog = true;
          localStorage.setItem(STORAGE_KEY, currentVersion);
        }
        localStorage.removeItem(CHANGELOG_BODY_KEY);
        return;
      }
    } catch {}
    localStorage.removeItem(CHANGELOG_BODY_KEY);
  }

  if (!lastSeen) {
    localStorage.setItem(STORAGE_KEY, currentVersion);
  }
}

export async function fetchChangelog(): Promise<string> {
  if (changelogBody) return changelogBody;

  try {
    // Same shared client as the hero and the versions list: one cached
    // API response serves all three, so opening the changelog no longer
    // costs extra GitHub API calls (the old code fetched the list and
    // then /releases/latest again on failure).
    const releases = await fetchGithubReleases();
    const release = releases.find((item) => item.tag_name === `v${currentVersion}`) ?? releases[0];
    if (release?.body) {
      changelogBody = release.body;
      return release.body;
    }
  } catch {}

  return "";
}
