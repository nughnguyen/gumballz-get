export interface ReleaseAsset {
  name: string;
  browser_download_url: string;
  size: number;
  download_count: number;
  content_type: string;
}

export interface GithubRelease {
  tag_name: string;
  name: string;
  body: string;
  published_at: string;
  html_url: string;
  assets: ReleaseAsset[];
  prerelease: boolean;
}

export interface ReleaseDownloads {
  version: string;
  releaseUrl: string;
  publishedAt: string;
  notes: string;
  windowsExe?: string;
  windowsMsi?: string;
  macDmg?: string;
  linuxAppImage?: string;
  linuxDeb?: string;
  chromeExtensionZip?: string;
}

export type ReleaseFileOs = "windows" | "macos" | "linux" | "extension" | "other";

export interface ReleaseFile {
  name: string;
  url: string;
  size: number;
  os: ReleaseFileOs;
  /** Uppercase file format, e.g. "EXE", "DMG", "AppImage", "DEB", "RPM", "TAR.GZ", "ZIP". */
  format: string;
  /** Only for .exe files: installer vs portable build. */
  variant?: "setup" | "portable";
}

export interface ReleaseVersion {
  version: string;
  name: string;
  publishedAt: string;
  notes: string;
  releaseUrl: string;
  prerelease: boolean;
  /** First non-prerelease in the list (newest stable). */
  isLatest: boolean;
  /** User-downloadable files, sorted Windows → macOS → Linux → Extension → Other. */
  files: ReleaseFile[];
}
