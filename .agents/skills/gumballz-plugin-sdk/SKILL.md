# GumballZ Plugin SDK

- Describe plugins with `plugin.manifest.json` (`id`, `name`, `version`, `category`, `icon`, `enabled`).
- Keep each plugin isolated in its own directory and expose Rust commands with typed inputs and outputs.
- Register frontend cards through the typed Svelte plugin registry; render enabled plugins in Tools Hub and sidebar Bento slots.
- Validate plugin identifiers, versions, and permissions at load time. Plugin UI must not bypass Tauri command authorization.
- Document public commands, events, permissions, and a copyable scaffold in `docs/PLUGIN_SDK.md`.

