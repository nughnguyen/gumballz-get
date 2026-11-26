import type { GithubRelease, ReleaseDownloads, ReleaseFile, ReleaseVersion } from './types';
import { fetchGithubReleases } from './github-client';

const REPO_OWNER = 'nughnguyen';
const REPO_NAME = 'gumballz-get';
const RELEASES_PAGE = `https://github.com/${REPO_OWNER}/${REPO_NAME}/releases`;

const FALLBACK_DOWNLOADS: ReleaseDownloads = {
  // RULE: this block always names the last RELEASED version, never the
  // version being built. The web deploys on every push to main, but a
  // release only exists after its tag's build finishes — a fallback
  // naming the unreleased version sends every button to a 404 (seen
  // 2026-11-25: fallback said v1.0.3 before the tag, hero Windows
  // button 404'd). Bump this block only after the release is live.
  version: 'v1.0.1',
  releaseUrl: RELEASES_PAGE,
  publishedAt: '2026-11-26',
  notes: 'About redesign (Bento layout), real download speed, Scribd thumbnails, in-app login, Friends tab, 2-click extension install.',
  // Asset names use the Tauri bundle name `GumballZ.Get_...` (dot), NOT
  // `GumballZ-Get_...` (hyphen) — the hyphenated URLs 404. Keep these in
  // sync with the real release assets on every version bump.
  windowsExe: `${RELEASES_PAGE}/download/v1.0.1/GumballZ.Get_1.0.1_x64-setup.exe`,
  windowsMsi: `${RELEASES_PAGE}/download/v1.0.1/GumballZ.Get_1.0.1_x64_en-US.msi`,
  macDmg: `${RELEASES_PAGE}/download/v1.0.1/GumballZ.Get_1.0.1_aarch64.dmg`,
  linuxAppImage: `${RELEASES_PAGE}/download/v1.0.1/GumballZ.Get_1.0.1_amd64.AppImage`,
  linuxDeb: `${RELEASES_PAGE}/download/v1.0.1/GumballZ.Get_1.0.1_amd64.deb`,
  chromeExtensionZip: `${RELEASES_PAGE}/download/v1.0.1/gumballz-get-chrome-extension-v1.0.1.zip`
};

/** File suffixes that are release metadata, not user downloads. */
const NON_DOWNLOADABLE_SUFFIXES = [
  ".sig",
  ".zsync",
  ".blockmap",
  ".json",
  ".yml",
  ".yaml",
  ".sha256",
  ".txt",
  ".asc"
];

/**
 * Map a release asset filename to the OS it supports and its file format.
 * Returns null for release metadata files (signatures, zsync, latest.json…)
 * that visitors should never download directly.
 */
export function classifyReleaseAsset(
  name: string
): Omit<ReleaseFile, "url" | "size"> | null {
  const lower = name.toLowerCase();
  if (NON_DOWNLOADABLE_SUFFIXES.some((ext) => lower.endsWith(ext))) return null;
  if (lower.endsWith(".exe")) {
    return {
      name,
      os: "windows",
      format: "EXE",
      variant: lower.includes("setup") ? "setup" : "portable"
    };
  }
  if (lower.endsWith(".msi")) return { name, os: "windows", format: "MSI" };
  if (lower.endsWith(".dmg")) return { name, os: "macos", format: "DMG" };
  if (lower.endsWith(".appimage")) return { name, os: "linux", format: "AppImage" };
  if (lower.endsWith(".deb")) return { name, os: "linux", format: "DEB" };
  if (lower.endsWith(".rpm")) return { name, os: "linux", format: "RPM" };
  if (lower.endsWith(".tar.gz")) return { name, os: "linux", format: "TAR.GZ" };
  if (lower.endsWith(".zip")) {
    if (lower.includes("extension")) return { name, os: "extension", format: "ZIP" };
    return { name, os: "other", format: "ZIP" };
  }
  // Unknown file type: still list it, with its extension as the format.
  const ext = lower.includes(".")
    ? lower.slice(lower.lastIndexOf(".") + 1).toUpperCase()
    : "FILE";
  return { name, os: "other", format: ext };
}

const OS_ORDER: Record<ReleaseFile["os"], number> = {
  windows: 0,
  macos: 1,
  linux: 2,
  extension: 3,
  other: 4
};

export function parseReleaseVersion(
  rel: GithubRelease,
  isLatest: boolean
): ReleaseVersion {
  const files: ReleaseFile[] = [];
  for (const asset of rel.assets || []) {
    const classified = classifyReleaseAsset(asset.name);
    if (!classified) continue;
    files.push({
      ...classified,
      url: asset.browser_download_url,
      size: asset.size
    });
  }
  // Windows first, then macOS, Linux, Extension, Other; setup before portable.
  files.sort((a, b) => {
    const osDiff = OS_ORDER[a.os] - OS_ORDER[b.os];
    if (osDiff !== 0) return osDiff;
    const variantOrder = (v?: string) => (v === "setup" ? 0 : v === "portable" ? 1 : 2);
    const variantDiff = variantOrder(a.variant) - variantOrder(b.variant);
    if (variantDiff !== 0) return variantDiff;
    return a.name.localeCompare(b.name);
  });
  return {
    version: rel.tag_name || "v1.0.0",
    name: rel.name || rel.tag_name || "",
    publishedAt: rel.published_at || "",
    notes: rel.body || "",
    releaseUrl: rel.html_url || RELEASES_PAGE,
    prerelease: !!rel.prerelease,
    isLatest,
    files
  };
}

/**
 * The full release list (newest first). Read through the shared
 * GitHub client, so the hero, the versions list and the changelog all
 * share one cached API response instead of each calling the API.
 * Throws only when the API fails and there is no cached list at all.
 */
export async function fetchAllReleases(): Promise<ReleaseVersion[]> {
  const releases = await fetchGithubReleases();
  const latestIndex = releases.findIndex((r) => !r.prerelease);
  return releases.map((r, i) => parseReleaseVersion(r, i === latestIndex));
}

export async function fetchLatestRelease(): Promise<ReleaseDownloads> {
  try {
    const releases = await fetchGithubReleases();
    const latest = releases.find((r) => !r.prerelease) || releases[0];
    const parsed: ReleaseDownloads = {
      version: latest.tag_name || 'v1.0.0',
      releaseUrl: latest.html_url || RELEASES_PAGE,
      publishedAt: latest.published_at || new Date().toISOString(),
      notes: latest.body || FALLBACK_DOWNLOADS.notes
    };

    for (const asset of latest.assets || []) {
      const name = asset.name.toLowerCase();
      if (name.endsWith('.exe')) {
        // Prefer the setup installer over the portable build: the hero
        // "Windows (.exe)" button should install the app (with updater),
        // not download the 93 MB portable.
        if (name.includes('setup') || !parsed.windowsExe) {
          parsed.windowsExe = asset.browser_download_url;
        }
      }
      else if (name.endsWith('.msi')) parsed.windowsMsi = asset.browser_download_url;
      else if (name.endsWith('.dmg')) parsed.macDmg = asset.browser_download_url;
      else if (name.endsWith('.appimage')) parsed.linuxAppImage = asset.browser_download_url;
      else if (name.endsWith('.deb')) parsed.linuxDeb = asset.browser_download_url;
      else if (name.includes('extension') && name.endsWith('.zip')) {
        parsed.chromeExtensionZip = asset.browser_download_url;
      }
    }

    // Fill in gaps from the same release's own files (a build still in
    // progress may not have attached every asset yet). Build the URLs
    // from this release's tag — never from FALLBACK_DOWNLOADS, whose
    // version can be ahead of the latest published release.
    const tag = latest.tag_name || 'v1.0.0';
    const ver = tag.replace(/^v/, '');
    const assetUrl = (file: string) => `${RELEASES_PAGE}/download/${tag}/${file}`;
    if (!parsed.windowsExe) parsed.windowsExe = assetUrl(`GumballZ.Get_${ver}_x64-setup.exe`);
    if (!parsed.macDmg) parsed.macDmg = assetUrl(`GumballZ.Get_${ver}_aarch64.dmg`);
    if (!parsed.linuxAppImage) parsed.linuxAppImage = assetUrl(`GumballZ.Get_${ver}_amd64.AppImage`);
    if (!parsed.chromeExtensionZip) parsed.chromeExtensionZip = assetUrl(`gumballz-get-chrome-extension-v${ver}.zip`);

    return parsed;
  } catch (err) {
    return FALLBACK_DOWNLOADS;
  }
}
