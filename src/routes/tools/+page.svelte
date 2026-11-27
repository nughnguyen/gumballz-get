<!--
  GumballZ Get - Modern cross-platform download manager
  Copyright (C) 2026 Nguyen Quoc Hung (GumballZ) <https://github.com/nughnguyen/gumballz-get>

  This program is free software: you can redistribute it and/or modify
  it under the terms of the GNU General Public License as published by
  the Free Software Foundation, either version 3 of the License, or
  (at your option) any later version.

  This program is distributed in the hope that it will be useful,
  but WITHOUT ANY WARRANTY; without even the implied warranty of
  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
  GNU General Public License for more details.

  You should have received a copy of the GNU General Public License
  along with this program. If not, see <https://www.gnu.org/licenses/>.
-->

<!--
  Tools hub: installed plugins only. Browsing, installing (registry or
  local .zip file) and managing plugins all live in PluginManager below,
  driven by live `list_plugins` state — no hardcoded tool catalog here.
  The catalog (`$lib/tools/catalog`) still backs the /tools/[category]
  routes and the command palette; it is intentionally not rendered here.
-->
<script lang="ts">
  import { t } from "$lib/i18n";
  import PluginManager from "$components/plugins/PluginManager.svelte";
</script>

<section class="tools-hub">
  <header class="tools-head">
    <img src="/icons/explore-64.png" alt="" class="tools-hero-icon" width="56" height="56" draggable="false" />
    <div class="tools-identity">
      <h1>{$t("tools.hub.title")}</h1>
      <p class="tools-subtitle">{$t("tools.hub.subtitle")}</p>
    </div>
  </header>

  <PluginManager />
</section>

<style>
  .tools-hub {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    width: 100%;
    max-width: 1040px;
    margin-inline: auto;
    padding: var(--space-4) var(--space-5) var(--space-9);
  }

  .tools-head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }

  .tools-hero-icon {
    width: 56px;
    height: 56px;
    border-radius: 14px;
    object-fit: contain;
    flex-shrink: 0;
  }

  .tools-identity {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .tools-head h1 {
    margin: 0 0 var(--space-1);
    font-family: var(--font-display);
    font-size: var(--text-2xl);
    font-weight: 700;
    letter-spacing: var(--track-tight);
    color: var(--text);
  }

  .tools-subtitle {
    margin: 0;
    font-size: var(--text-base);
    color: var(--text-muted);
  }
</style>
