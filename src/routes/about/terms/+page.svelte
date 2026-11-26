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
  import { t } from "$lib/i18n";
</script>

<section class="card">
  <h5 class="card-title">{$t('about.terms_title')}</h5>
  <p class="card-text">{$t('about.terms_text')}</p>
</section>

<section class="card">
  <h5 class="card-title">{$t('about.legal_title')}</h5>
  <p class="card-text">{$t('about.legal_text')}</p>
</section>

<section class="card">
  <h5 class="card-title">{$t('about.responsibility_title')}</h5>
  <p class="card-text">{$t('about.responsibility_text')}</p>
</section>

<section class="card">
  <h5 class="card-title">{$t('about.mascot_title')}</h5>
  <p class="card-text">{$t('about.mascot_text')}</p>
</section>

<section class="card">
  <h5 class="card-title">{$t('about.license_title')}</h5>
  <p class="card-text">{$t('about.license_text')}</p>
</section>

<style>
  .card {
    width: 100%;
    background: var(--surface);
    border: 2px solid var(--border-ink);
    box-shadow: 4px 4px 0 var(--shadow-hard);
    border-radius: 12px;
    padding: calc(var(--padding) + 4px);
    display: flex;
    flex-direction: column;
    gap: calc(var(--padding) / 2);
  }

  .card-title {
    color: var(--secondary);
    font-size: var(--text-md);
    font-weight: 600;
  }

  .card-text {
    font-size: 13px;
    font-weight: 400;
    line-height: 1.8;
    color: var(--text-muted);
    user-select: text;
  }
</style>
