/** Runtime boundary for features that work in both the browser and Tauri. */
export function isTauriDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const workerBase = import.meta.env.VITE_WORKER_API_URL?.replace(/\/$/, "");

export const PlatformBridge = {
  isDesktop: isTauriDesktop,

  async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    if (!isTauriDesktop()) {
      if (!workerBase) throw new Error("This action requires the GumballZ Get desktop app or a configured web worker.");
      const response = await fetch(`${workerBase}/api/commands/${encodeURIComponent(command)}`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(args ?? {}),
      });
      if (!response.ok) throw new Error(`Web worker request failed (${response.status}).`);
      return (await response.json()) as T;
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  },

  async openExternal(url: string): Promise<void> {
    const parsed = new URL(url);
    if (parsed.protocol !== "https:" && parsed.protocol !== "mailto:") throw new Error("Only HTTPS and mail links can be opened.");
    if (isTauriDesktop()) {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(parsed.href);
    } else {
      window.open(parsed.href, "_blank", "noopener,noreferrer");
    }
  },

  async copyText(value: string): Promise<void> {
    if (isTauriDesktop()) {
      const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
      await writeText(value);
      return;
    }
    await navigator.clipboard.writeText(value);
  },

  async chooseFile(accept = "*/*"): Promise<File | null> {
    if (isTauriDesktop()) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: false, filters: [{ name: "Files", extensions: accept.split(",").map((item) => item.trim().replace(/^\./, "")) }] });
      if (!selected || Array.isArray(selected)) return null;
      const { invoke } = await import("@tauri-apps/api/core");
      const bytes = await invoke<number[]>("plugin:fs|read_file", { path: selected });
      return new File([new Uint8Array(bytes)], selected.split(/[\\/]/).at(-1) ?? "file", { type: "application/octet-stream" });
    }
    return new Promise((resolve) => {
      const input = document.createElement("input");
      input.type = "file";
      input.accept = accept;
      input.onchange = () => resolve(input.files?.[0] ?? null);
      input.click();
    });
  },

  async selectPath(options: { directory?: boolean; multiple?: boolean; extensions?: string[] } = {}): Promise<string | string[] | null> {
    if (!isTauriDesktop()) return null;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const filters = options.extensions?.length ? [{ name: "Supported files", extensions: options.extensions }] : undefined;
    return open({ directory: options.directory ?? false, multiple: options.multiple ?? false, filters });
  },

  async listen<T>(event: string, callback: (payload: T) => void): Promise<() => void> {
    if (!isTauriDesktop()) return () => {};
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>(event, (message) => callback(message.payload));
  },

  downloadBlob(blob: Blob, filename: string): void {
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = filename.replace(/[\\/:*?"<>|\0]/g, "_");
    anchor.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  },

  async inspectUrl(url: string): Promise<unknown> {
    if (!workerBase) throw new Error("Web downloads are not configured. Install the desktop app for unrestricted downloads.");
    const response = await fetch(`${workerBase}/api/inspect`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ url }),
    });
    if (!response.ok) throw new Error(`Could not inspect link (${response.status}).`);
    return response.json();
  },

  async downloadUrl(url: string): Promise<void> {
    if (!workerBase) throw new Error("Web downloads are not configured. Install the desktop app for unrestricted downloads.");
    const response = await fetch(`${workerBase}/api/download?url=${encodeURIComponent(url)}`);
    if (!response.ok) throw new Error(`Could not download this link (${response.status}).`);
    const blob = await response.blob();
    this.downloadBlob(blob, "gumballz-media");
  },
};
