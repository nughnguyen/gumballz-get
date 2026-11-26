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
  let { size = 64, decorative = true }: { size?: number; decorative?: boolean } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 64 64" role={decorative ? undefined : "img"} aria-label={decorative ? undefined : "GumballZ Get"} aria-hidden={decorative ? "true" : undefined}>
  <rect x="3" y="3" width="58" height="58" rx="10" fill="#22D3EE" stroke="#0F172A" stroke-width="4" />
  <path d="M16 20h30v9H26v6h15v9H26v4H16V20Z" fill="#0F172A" />
  <circle cx="47" cy="46" r="5" fill="#FBBF24" stroke="#0F172A" stroke-width="3" />
</svg>
