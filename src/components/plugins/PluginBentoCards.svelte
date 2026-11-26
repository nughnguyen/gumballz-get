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

<script lang="ts">
  import { ENABLED_PLUGINS } from "$lib/plugins/registry";
</script>

<section class="plugin-section" aria-labelledby="plugins-title">
  <div class="section-head"><span class="section-title" id="plugins-title">Plugin Studio</span><span class="count">{ENABLED_PLUGINS.length} ACTIVE</span></div>
  <div class="plugin-grid">
    {#each ENABLED_PLUGINS as plugin (plugin.id)}
      <a class="plugin-card" class:learning={plugin.category === "learning"} href={plugin.route}>
        <span class="tag">{plugin.category.toUpperCase()} · v{plugin.version}</span>
        <h2>{plugin.name}</h2>
        <p>{plugin.description}</p>
        <b>Open plugin →</b>
      </a>
    {/each}
  </div>
</section>

<style>
  .plugin-section { display:grid; gap:12px; margin-block:16px 28px; }.section-head { display:flex; justify-content:space-between; align-items:center; }.section-title { font-size:13px; font-weight:800; letter-spacing:.04em; }.count,.tag { font:700 10px 'JetBrains Mono',monospace; letter-spacing:.1em; }.count { color:var(--text-dim); }
  .plugin-grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:16px; }.plugin-card { display:flex; flex-direction:column; min-height:160px; padding:18px; border:2px solid #101820; border-radius:12px; background:#d7f3d1; color:#101820; text-decoration:none; box-shadow:4px 4px 0 #101820; }.plugin-card.learning { background:#cff6fc; }.plugin-card h2 { margin:14px 0 4px; font-size:20px; }.plugin-card p { margin:0 0 14px; font-size:13px; line-height:1.45; }.plugin-card b { margin-top:auto; font-size:12px; }.plugin-card:active { transform:translate(2px,2px); box-shadow:2px 2px 0 #101820; }
  @media(max-width:700px) {.plugin-grid{grid-template-columns:1fr;}}
</style>
