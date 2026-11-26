import { describe, it, expect } from "vitest";
import fs from "node:fs";
import path from "node:path";

describe("Desktop Security Configuration & Build Setup", () => {
  const rootDir = path.resolve(__dirname, "../..");
  const tauriConfPath = path.join(rootDir, "src-tauri/tauri.conf.json");
  const capabilitiesPath = path.join(rootDir, "src-tauri/capabilities/default.json");
  const packageJsonPath = path.join(rootDir, "package.json");

  it("should enforce a strict non-null Content Security Policy (CSP)", () => {
    const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, "utf-8"));
    const csp = tauriConf.app?.security?.csp;
    expect(csp).toBeTruthy();
    expect(typeof csp).toBe("string");
    expect(csp).toContain("default-src 'self'");
    expect(csp).not.toContain("'unsafe-eval'");
  });

  it("permits the loopback media bridge without allowing arbitrary HTTP origins", () => {
    const csp = JSON.parse(fs.readFileSync(tauriConfPath, "utf-8")).app.security.csp as string;
    const directives = new Map(csp.split(";").filter((part) => part.trim()).map((part) => {
      const [name, ...sources] = part.trim().split(/\s+/);
      return [name, sources];
    }));
    for (const name of ["connect-src", "media-src"]) {
      const sources = directives.get(name) ?? [];
      const httpSources = sources.filter((source) => source.startsWith("http:"));
      expect(httpSources.length).toBeGreaterThan(0);
      expect(httpSources.every((source) => /^http:\/\/(ipc\.localhost|asset\.localhost|127\.0\.0\.1:4772[0-9])$/.test(source))).toBe(true);
      expect(sources).not.toContain("*");
    }
    expect(directives.get("script-src")).not.toContain("http://127.0.0.1:47720");
  });

  it("should not allow wildcard '**' filesystem access in assetProtocol scope", () => {
    const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, "utf-8"));
    const allowList: string[] = tauriConf.app?.security?.assetProtocol?.scope?.allow ?? [];
    expect(allowList).not.toContain("**");
    expect(allowList.length).toBeGreaterThan(0);
  });

  it("should not use unscoped 'shell:default' in default capability", () => {
    const caps = JSON.parse(fs.readFileSync(capabilitiesPath, "utf-8"));
    const permissions: (string | Record<string, unknown>)[] = caps.permissions ?? [];
    expect(permissions).not.toContain("shell:default");

    const shellOpenPermission = permissions.find(
      (p) => typeof p === "object" && p !== null && (p as any).identifier === "shell:allow-open"
    ) as any;
    expect(shellOpenPermission).toBeDefined();
    expect(shellOpenPermission.allow).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ url: "https://github.com/**" }),
        expect.objectContaining({ url: "https://zypage.com/**" }),
      ])
    );
  });

  it("should configure 4GB heap size in package.json build script to prevent OOM", () => {
    const pkg = JSON.parse(fs.readFileSync(packageJsonPath, "utf-8"));
    const buildScript = pkg.scripts?.build ?? "";
    expect(buildScript).toContain("--max-old-space-size=4096");
  });
});
