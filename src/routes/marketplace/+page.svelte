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
  The Marketplace was folded into the Tools page. This route only exists
  so old links (sidebar bookmarks, palette entries, guard links) land on
  /tools instead of a 404.
-->
<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";

  onMount(() => {
    void goto("/tools", { replaceState: true });
  });
</script>

<div class="redirect-page">
  <p>{$t("tools.hub.plugin_install_hint")}</p>
  <a href="/tools">{$t("tools.hub.go_to_tools")}</a>
</div>

<style>
  .redirect-page {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-8) var(--space-4);
    text-align: center;
    color: var(--text-muted);
  }

  .redirect-page a {
    padding: 10px 24px;
    font-size: var(--text-base);
    font-weight: 600;
    background: var(--cta);
    color: var(--on-cta);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md);
    text-decoration: none;
    box-shadow: 2px 2px 0 var(--shadow-hard);
  }

  .redirect-page a:active {
    transform: translate(2px, 2px);
    box-shadow: 1px 1px 0 var(--shadow-hard);
  }
</style>
