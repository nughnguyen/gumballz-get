import { describe, expect, it } from "vitest";
import { classifyReleaseAsset, parseReleaseVersion } from "./releases";
import type { GithubRelease } from "./types";

describe("classifyReleaseAsset", () => {
  it("maps setup and portable .exe to Windows", () => {
    expect(classifyReleaseAsset("GumballZ-Get_1.0.0_x64-setup.exe")).toEqual({
      name: "GumballZ-Get_1.0.0_x64-setup.exe",
      os: "windows",
      format: "EXE",
      variant: "setup"
    });
    expect(classifyReleaseAsset("GumballZ.Get_1.0.0_x64-portable.exe")).toEqual({
      name: "GumballZ.Get_1.0.0_x64-portable.exe",
      os: "windows",
      format: "EXE",
      variant: "portable"
    });
  });

  it("maps .msi to Windows and .dmg to macOS", () => {
    expect(classifyReleaseAsset("GumballZ-Get_1.0.0_x64_en-US.msi")).toMatchObject({
      os: "windows",
      format: "MSI"
    });
    expect(classifyReleaseAsset("GumballZ-Get_1.0.0_aarch64.dmg")).toMatchObject({
      os: "macos",
      format: "DMG"
    });
  });

  it("maps AppImage, deb, rpm and tar.gz to Linux", () => {
    expect(classifyReleaseAsset("GumballZ.Get_1.0.0_amd64.AppImage")).toMatchObject({
      os: "linux",
      format: "AppImage"
    });
    expect(classifyReleaseAsset("GumballZ.Get_1.0.0_amd64.deb")).toMatchObject({
      os: "linux",
      format: "DEB"
    });
    expect(classifyReleaseAsset("GumballZ.Get-1.0.0-1.x86_64.rpm")).toMatchObject({
      os: "linux",
      format: "RPM"
    });
    expect(
      classifyReleaseAsset("gumballz-cli-1.0.0-x86_64-unknown-linux-gnu.tar.gz")
    ).toMatchObject({ os: "linux", format: "TAR.GZ" });
  });

  it("maps the extension zip to Chrome Extension, other zips to Other", () => {
    expect(
      classifyReleaseAsset("gumballz-get-chrome-extension-v1.0.0.zip")
    ).toMatchObject({ os: "extension", format: "ZIP" });
    expect(classifyReleaseAsset("some-tool-1.0.0.zip")).toMatchObject({
      os: "other",
      format: "ZIP"
    });
  });

  it("excludes release metadata files", () => {
    for (const name of [
      "GumballZ.Get_1.0.0_amd64.AppImage.sig",
      "GumballZ.Get_1.0.0_amd64.AppImage.zsync",
      "latest.json",
      "checksums.sha256",
      "notes.txt"
    ]) {
      expect(classifyReleaseAsset(name)).toBeNull();
    }
  });
});

describe("parseReleaseVersion", () => {
  const release: GithubRelease = {
    tag_name: "v1.0.0",
    name: "GumballZ Get 1.0.0",
    body: "Initial release",
    published_at: "2026-11-25T01:36:15Z",
    html_url: "https://github.com/nughnguyen/gumballz-get/releases/tag/v1.0.0",
    prerelease: false,
    assets: [
      {
        name: "GumballZ.Get_1.0.0_amd64.AppImage",
        browser_download_url: "https://example.com/app.AppImage",
        size: 173558264,
        download_count: 0,
        content_type: "application/octet-stream"
      },
      {
        name: "GumballZ.Get_1.0.0_amd64.AppImage.zsync",
        browser_download_url: "https://example.com/app.AppImage.zsync",
        size: 296849,
        download_count: 0,
        content_type: "application/octet-stream"
      },
      {
        name: "GumballZ-Get_1.0.0_x64-setup.exe",
        browser_download_url: "https://example.com/setup.exe",
        size: 42000000,
        download_count: 0,
        content_type: "application/octet-stream"
      },
      {
        name: "gumballz-get-chrome-extension-v1.0.0.zip",
        browser_download_url: "https://example.com/ext.zip",
        size: 1234567,
        download_count: 0,
        content_type: "application/zip"
      }
    ]
  };

  it("keeps only downloadable files, sorted Windows → Linux → Extension", () => {
    const parsed = parseReleaseVersion(release, true);
    expect(parsed.version).toBe("v1.0.0");
    expect(parsed.isLatest).toBe(true);
    expect(parsed.files.map((f) => f.name)).toEqual([
      "GumballZ-Get_1.0.0_x64-setup.exe",
      "GumballZ.Get_1.0.0_amd64.AppImage",
      "gumballz-get-chrome-extension-v1.0.0.zip"
    ]);
    expect(parsed.files[0]).toMatchObject({
      os: "windows",
      format: "EXE",
      variant: "setup",
      url: "https://example.com/setup.exe",
      size: 42000000
    });
  });
});
