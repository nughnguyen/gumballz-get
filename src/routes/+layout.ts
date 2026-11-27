import { loadTranslations, defaultLocale, detectSystemLocale } from "$lib/i18n";
import { syncTrayLabels } from "$lib/tray-labels";
import { loadSettings } from "$lib/stores/settings-store.svelte";
import { isTauriDesktop } from "$lib/core/platform";

export const ssr = false;

export const load = async ({ url }) => {
  let lang = detectSystemLocale();
  if (isTauriDesktop()) {
  try {
    const settings = await loadSettings();
    lang = settings.appearance?.language || lang;
  } catch {
    // First run or settings unavailable — keep detected system locale
  }
  }
  try {
    await loadTranslations(lang, url.pathname);
  } catch (e) {
    // The static English in `$lib/i18n` holds the UI; just log the reason.
    console.warn(`[i18n] could not load "${lang}":`, e);
  }
  // The native tray has its own strings; push the ones for the active locale.
  if (isTauriDesktop()) await syncTrayLabels();
  return {};
};
