import type { GithubRelease } from "./types";

/**
 * The ONE place in the app that talks to the GitHub API.
 *
 * Before this client, three call sites hit the GitHub API on their own
 * (hero, versions list, changelog) with no token — the unauthenticated
 * 60 requests/hour/IP limit was hit on 25/11 and the site showed a red
 * error. Every release read now goes through here: one network request
 * at most per 5 minutes (in-memory cache, concurrent callers share one
 * in-flight request), and the last good list is kept in localStorage
 * for 24 h so the site keeps working when the API is unreachable or
 * rate-limited.
 */

const GITHUB_API = "https://api.github.com/repos/nughnguyen/gumballz-get";
const RELEASES_URL = `${GITHUB_API}/releases`;

const MEMORY_TTL = 5 * 60 * 1000; // 5 minutes
const STORAGE_TTL = 24 * 60 * 60 * 1000; // 24 h
const CACHE_KEY = "gumballz-get-releases-raw";

let memoryCache: { at: number; data: GithubRelease[] } | null = null;
let inflight: Promise<GithubRelease[]> | null = null;

function readStoredReleases(): GithubRelease[] | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const { at, releases } = JSON.parse(raw) as { at: number; releases: GithubRelease[] };
    if (!Array.isArray(releases) || releases.length === 0) return null;
    if (Date.now() - at > STORAGE_TTL) return null;
    return releases;
  } catch {
    return null;
  }
}

function writeStoredReleases(releases: GithubRelease[]): void {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify({ at: Date.now(), releases }));
  } catch {
    // Private mode / no storage — the in-memory cache still works.
  }
}

/**
 * The full release list (newest first) from the GitHub API.
 * Serves the 24 h localStorage copy on API failure instead of a bare
 * error; throws only when there is no cached list at all.
 */
export function fetchGithubReleases(): Promise<GithubRelease[]> {
  const now = Date.now();
  if (memoryCache && now - memoryCache.at < MEMORY_TTL) {
    return Promise.resolve(memoryCache.data);
  }
  if (inflight) return inflight;
  inflight = (async () => {
    try {
      const res = await fetch(RELEASES_URL, {
        headers: { Accept: "application/vnd.github.v3+json" }
      });
      if (!res.ok) {
        throw new Error(`GitHub API returned ${res.status}`);
      }
      const releases: GithubRelease[] = await res.json();
      if (!Array.isArray(releases) || releases.length === 0) {
        throw new Error("No releases found");
      }
      memoryCache = { at: Date.now(), data: releases };
      writeStoredReleases(releases);
      return releases;
    } catch (err) {
      const cached = readStoredReleases();
      if (cached) {
        memoryCache = { at: Date.now(), data: cached };
        return cached;
      }
      throw err;
    } finally {
      inflight = null;
    }
  })();
  return inflight;
}
