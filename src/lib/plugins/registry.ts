export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  category: "learning" | "media" | "utility";
  icon: string;
  enabled: boolean;
  description: string;
  route: string;
  iconSvg: string;
}

export const PLUGIN_REGISTRY = [
  {
    id: "listening-studio",
    name: "Listening & Vocab Studio",
    version: "1.0.0",
    category: "learning",
    icon: "headphones",
    enabled: true,
    description: "Split subtitle-timed practice clips and export cloze worksheets.",
    route: "/plugins/listening-studio",
    iconSvg: "M4 10v4m4-8v12m4-16v16m4-12v8m4-6v4",
  },
  {
    id: "asset-forge",
    name: "Batch Media & Asset Forge",
    version: "1.0.0",
    category: "media",
    icon: "wand",
    enabled: true,
    description: "Convert audio, video, and image folders in parallel with FFmpeg.",
    route: "/plugins/asset-forge",
    iconSvg: "M12 3v18m9-9H3m15.36-6.36L5.64 18.36m12.72 0L5.64 5.64",
  },
] as const satisfies readonly PluginManifest[];

export const ENABLED_PLUGINS = PLUGIN_REGISTRY.filter((plugin) => plugin.enabled);
