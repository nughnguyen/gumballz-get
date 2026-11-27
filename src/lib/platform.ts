/**
 * Shortcut labels that exist on the reader's keyboard.
 *
 * The `⌘` was hardcoded in the toolbar. On Windows and Linux that is worse
 * than ugly: the key doesn't exist there, so the hint stops being a hint — the user
 * can't act on it, and the app starts signalling it was made for another
 * platform. The shortcut itself always accepted both keys
 * (`e.metaKey || e.ctrlKey`); only the label lied.
 */

/** `true` no macOS. */
export function isMac(): boolean {
  if (typeof navigator === "undefined") return false;
  // `userAgentData.platform` is the new path; `navigator.platform` is
  // deprecated but the only one that answers in WebKit, which is exactly the engine
  // the app runs on in macOS.
  const uaData = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData;
  const raw = uaData?.platform || navigator.platform || "";
  return /mac/i.test(raw);
}

/** `⌘` no macOS, `Ctrl` no resto. */
export function modKey(): string {
  return isMac() ? "⌘" : "Ctrl";
}

/**
 * Atalho pronto para exibir, com o separador que cada plataforma usa.
 *
 * macOS joins the keys (`⌘K`); Windows and Linux separate them with `+` (`Ctrl+K`).
 * Writing `⌘+K` or `CtrlK` is the kind of detail that makes the app look
 * translated instead of native.
 */
export function shortcut(...keys: string[]): string {
  return isMac() ? `${modKey()}${keys.join("")}` : [modKey(), ...keys].join("+");
}

/**
 * Label for a shortcut stored in Tauri format (`CmdOrCtrl+Shift+K`).
 *
 * It lives here and not in each component because two copies diverged: one
 * translated `Meta` to `Win` outside macOS and the other left `Meta` raw, so
 * the same shortcut showed up under different names on two screens of the same app.
 */
export function formatBinding(binding: string): string {
  if (!binding) return "";
  const mac = isMac();
  return binding
    .split("+")
    .map((part) => {
      if (part === "CmdOrCtrl") return modKey();
      if (part === "Meta" || part === "Super" || part === "Command") return mac ? "⌘" : "Win";
      if (part === "Alt") return mac ? "⌥" : "Alt";
      if (part === "Shift") return mac ? "⇧" : "Shift";
      if (part === "Control" || part === "Ctrl") return mac ? "⌃" : "Ctrl";
      return part;
    })
    .join(mac ? "" : "+");
}

export type OsName = "windows" | "macos" | "linux";

/** System the app is running on, to warn when a tool doesn't run on it. */
export function currentOs(): OsName {
  if (isMac()) return "macos";
  if (typeof navigator === "undefined") return "linux";
  const uaData = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData;
  const raw = `${uaData?.platform || ""} ${navigator.platform || ""} ${navigator.userAgent || ""}`;
  return /win/i.test(raw) ? "windows" : "linux";
}
