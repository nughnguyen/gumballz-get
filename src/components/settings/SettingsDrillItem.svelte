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
  let { title, hint, onclick }: {
    title: string;
    hint: string;
    onclick: () => void;
  } = $props();
</script>

<button type="button" class="settings-drill-item" {onclick}>
  <span class="settings-drill-item-text">
    <span class="settings-drill-item-title">{title}</span>
    <span class="settings-drill-item-hint">{hint}</span>
  </span>
  <span class="settings-drill-item-chevron" aria-hidden="true">›</span>
</button>
