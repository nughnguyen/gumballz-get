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
    import { getVersion } from "@tauri-apps/api/app";
    import { open } from "@tauri-apps/plugin-shell";
    import { BUILD_INFO } from "$lib/build-info";
    import BrandMark from "$lib/ui/BrandMark.svelte";
    import DonateTrigger from "$components/donate/DonateTrigger.svelte";

    let version = $state("");

    $effect(() => {
        getVersion().then(v => { version = v; }).catch(() => {});
    });

    const buildDetails = $derived(
        [BUILD_INFO.commitShort, BUILD_INFO.branch, BUILD_INFO.date]
            .filter((part) => part && part !== "unknown")
            .join(" · ")
    );

    // Highlights reuse the feature copy already shown on the project page;
    // no new strings. Spans are asymmetric on purpose (5 / 4 / 3).
    const highlights = [
        { key: "about.feature_platforms", toneClass: "tone-cyan", spanClass: "hl-wide" },
        { key: "about.feature_progress", toneClass: "tone-lime", spanClass: "hl-mid" },
        { key: "about.feature_tech", toneClass: "tone-amber", spanClass: "hl-narrow" },
    ] as const;

    const cards = [
        { href: "/about/changelog", titleKey: "about.card_changelog_title", descKey: "about.card_changelog_desc", icon: "changelog", toneClass: "tone-cyan" },
        { href: "/about/project", titleKey: "about.card_project_title", descKey: "about.card_project_desc", icon: "project", toneClass: "tone-lime" },
        { href: "/about/terms", titleKey: "about.card_terms_title", descKey: "about.card_terms_desc", icon: "terms", toneClass: "tone-amber" },
    ] as const;

    async function openAuthorGithub(e: Event) {
        e.preventDefault();
        await open("https://github.com/nughnguyen");
    }

    async function openProjectGithub(e: Event) {
        e.preventDefault();
        await open("https://github.com/nughnguyen/gumballz-get");
    }

    async function openWebsite(e: Event) {
        e.preventDefault();
        await open("https://gumballz-get.vercel.app");
    }

</script>

<div class="about-bento">
    <!-- Hero: wide Bento panel, mark tile + identity -->
    <header class="bento-panel about-hero tone-cyan">
        <div class="hero-mark">
            <BrandMark size={64} />
        </div>
        <div class="about-identity">
            <div class="about-name-row">
                <h1>GumballZ Get</h1>
                {#if version}
                    <span class="tag tag-accent about-version">{$t("about.version")} {version}</span>
                {/if}
            </div>
            <p class="about-tagline">{$t("about.tagline")}</p>
            <p class="about-desc">{$t("about.description")}</p>
            {#if buildDetails}
                <span class="about-build">{buildDetails}</span>
            {/if}
        </div>
    </header>

    <!-- Highlights: asymmetric strip, one accent per tile -->
    {#each highlights as hl}
        <section class="bento-panel hl-tile {hl.toneClass} {hl.spanClass}">
            <p class="hl-text">{$t(hl.key)}</p>
        </section>
    {/each}

    <!-- Destinations: Bento tiles, not a stacked list -->
    {#each cards as card}
        <a href={card.href} class="bento-panel nav-tile {card.toneClass}">
            <span class="nav-top">
                <span class="symbol-tile symbol-tile-lg nav-icon" aria-hidden="true">
                    {#if card.icon === "changelog"}
                        <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>
                    {:else if card.icon === "project"}
                        <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/></svg>
                    {:else}
                        <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                    {/if}
                </span>
                <span class="nav-chevron" aria-hidden="true">›</span>
            </span>
            <span class="nav-title">{$t(card.titleKey)}</span>
            <span class="nav-desc">{$t(card.descKey)}</span>
        </a>
    {/each}

    <!-- Credits + actions: side-by-side panels, not one long column -->
    <section class="bento-panel about-credits">
        <div class="credit-row">
            <span class="credit-label">Maintained by</span>
            <strong>Nguyen Quoc Hung (GumballZ)</strong>
        </div>
        <div class="credit-row">
            <span class="credit-label">License</span>
            <strong>GPL-3.0</strong>
        </div>
        <a class="credit-link" href="https://github.com/nughnguyen" target="_blank" rel="noopener">
            GitHub profile <span aria-hidden="true">↗</span>
        </a>
    </section>

    <aside class="bento-panel about-actions">
        <p class="actions-lead">{$t("about.star_description")}</p>
        <div class="actions-stack">
            <DonateTrigger inline />
            <a href="https://gumballz-get.vercel.app" target="_blank" rel="noopener" class="btn about-ext-link" onclick={openWebsite}>
                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <circle cx="12" cy="12" r="10" />
                    <line x1="2" y1="12" x2="22" y2="12" />
                    <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                </svg>
                {$t("about.website_button")}
            </a>
            <a href="https://github.com/nughnguyen/gumballz-get" target="_blank" rel="noopener" class="btn about-ext-link" onclick={openProjectGithub}>
                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 0 0-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0 0 20 4.77 5.07 5.07 0 0 0 19.91 1S18.73.65 16 2.48a13.38 13.38 0 0 0-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 0 0 5 4.77a5.44 5.44 0 0 0-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 0 0 9 18.13V22"/>
                </svg>
                {$t("about.star_button")}
            </a>
            <a href="https://github.com/nughnguyen" target="_blank" rel="noopener" class="btn about-ext-link">Creator profile</a>
        </div>
    </aside>

    <footer class="about-footer">
        <hr class="separator" />
        <p class="about-credit">{$t("about.credit")}</p>
        <a href="https://github.com/nughnguyen" class="about-watermark" onclick={openAuthorGithub} title="@Nguyen Quoc Hung (GumballZ)">
            @Nguyen Quoc Hung (GumballZ)
        </a>
    </footer>
</div>

<style>
    /* Asymmetric Bento grid: hero 12, highlights 5/4/3, nav 4/4/4,
       credits 7 + actions 5, footer 12. One column below 720px. */
    .about-bento {
        display: grid;
        grid-template-columns: repeat(12, minmax(0, 1fr));
        gap: 16px;
    }

    .bento-panel {
        background: var(--surface);
        border: 2px solid var(--border-ink);
        border-radius: 12px;
        box-shadow: 4px 4px 0 var(--shadow-hard);
        position: relative;
        overflow: hidden;
    }

    /* Deliberate accents: one per panel, as a top bar (decorative only,
       never behind text, so contrast holds in both themes). */
    .tone-cyan::before,
    .tone-lime::before,
    .tone-amber::before {
        content: "";
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: 4px;
    }
    .tone-cyan::before { background: var(--accent); }
    .tone-lime::before { background: #A3E635; }
    .tone-amber::before { background: #FBBF24; }

    /* ---------- Hero ---------- */
    .about-hero {
        grid-column: span 12;
        display: flex;
        align-items: flex-start;
        gap: var(--space-5);
        padding: calc(var(--space-5) + 4px) var(--space-5) var(--space-5);
    }

    .hero-mark {
        flex-shrink: 0;
        width: 88px;
        height: 88px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: var(--pane);
        border: 2px solid var(--border-ink);
        border-radius: 12px;
        box-shadow: 3px 3px 0 var(--shadow-hard);
    }

    .about-identity {
        display: flex;
        flex-direction: column;
        gap: var(--space-2);
        min-width: 0;
    }

    .about-name-row {
        display: flex;
        align-items: center;
        gap: var(--space-3);
        flex-wrap: wrap;
    }

    .about-identity h1 {
        font-family: var(--font-display);
        font-size: 28px;
        line-height: 1.15;
        font-weight: 700;
        letter-spacing: var(--track-tight);
        margin: 0;
    }

    .about-tagline {
        font-size: var(--text-md);
        line-height: var(--leading-md);
        font-weight: 600;
        color: var(--text);
        margin: 0;
    }

    .about-desc {
        font-size: var(--text-base);
        line-height: var(--leading-base);
        color: var(--text-muted);
        margin: 0;
        max-width: 60ch;
    }

    .about-build {
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text-dim);
        user-select: all;
    }

    /* ---------- Highlights ---------- */
    .hl-tile {
        padding: calc(var(--space-4) + 4px) var(--space-4) var(--space-4);
        display: flex;
    }

    .hl-wide { grid-column: span 5; }
    .hl-mid { grid-column: span 4; }
    .hl-narrow { grid-column: span 3; }

    .hl-text {
        font-size: 13px;
        line-height: 1.65;
        color: var(--text-muted);
        margin: 0;
        display: -webkit-box;
        -webkit-line-clamp: 6;
        -webkit-box-orient: vertical;
        overflow: hidden;
    }

    /* ---------- Nav tiles ---------- */
    .nav-tile {
        grid-column: span 4;
        display: flex;
        flex-direction: column;
        gap: var(--space-2);
        padding: calc(var(--space-4) + 4px) var(--space-4) var(--space-4);
        text-decoration: none;
        color: inherit;
        cursor: pointer;
        transition:
            transform var(--duration-fast) var(--ease-out),
            box-shadow var(--duration-fast) var(--ease-out),
            background var(--duration-fast) var(--ease-out);
    }

    .nav-top {
        display: flex;
        align-items: center;
        justify-content: space-between;
    }

    .nav-icon {
        width: 40px;
        height: 40px;
        border-radius: 10px;
    }

    /* Icon tile inherits its panel's accent: cyan is the default
       .symbol-tile; lime/amber need dark ink on the bright fill. */
    .tone-lime .nav-icon { background: #A3E635; color: #0F172A; }
    .tone-amber .nav-icon { background: #FBBF24; color: #101820; }

    .nav-chevron {
        font-size: 20px;
        line-height: 1;
        color: var(--text-dim);
        transition: transform var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
    }

    .nav-title {
        font-family: var(--font-display);
        font-size: 15px;
        font-weight: 700;
        letter-spacing: var(--track-tight);
        color: var(--text);
    }

    .nav-desc {
        font-size: 13px;
        line-height: 1.55;
        color: var(--text-muted);
    }

    @media (hover: hover) {
        .nav-tile:hover {
            background: var(--surface-hi);
            transform: translate(-2px, -2px);
            box-shadow: 6px 6px 0 var(--shadow-hard);
        }

        .nav-tile:hover .nav-chevron {
            transform: translateX(2px);
            color: var(--accent-text);
        }
    }

    .nav-tile:active {
        transform: translate(2px, 2px);
        box-shadow: 2px 2px 0 var(--shadow-hard);
    }

    .nav-tile:focus-visible {
        outline: var(--focus-ring);
        outline-offset: 2px;
    }

    /* ---------- Credits + actions ---------- */
    .about-credits {
        grid-column: span 7;
        display: flex;
        flex-direction: column;
        padding: var(--space-4) var(--space-5);
    }

    .credit-row {
        display: flex;
        flex-direction: column;
        gap: 2px;
        padding: var(--space-3) 0;
    }

    .credit-row + .credit-row {
        border-top: var(--hairline) solid var(--separator);
    }

    .credit-label {
        color: var(--text-muted);
        font-size: var(--text-xs);
        text-transform: uppercase;
        letter-spacing: 0.06em;
    }

    .credit-row strong {
        font-size: var(--text-base);
        font-weight: 600;
        color: var(--text);
    }

    .credit-link {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding-top: var(--space-3);
        border-top: var(--hairline) solid var(--separator);
        font-size: var(--text-base);
        font-weight: 600;
        color: var(--accent-text);
        text-decoration: none;
    }

    .credit-link:focus-visible {
        outline: var(--focus-ring);
        outline-offset: 2px;
    }

    @media (hover: hover) {
        .credit-link:hover {
            color: var(--accent-hi);
        }
    }

    .about-actions {
        grid-column: span 5;
        display: flex;
        flex-direction: column;
        gap: var(--space-3);
        padding: var(--space-4) var(--space-5);
    }

    .actions-lead {
        font-size: 13px;
        line-height: 1.55;
        color: var(--text-muted);
        margin: 0;
    }

    .actions-stack {
        display: flex;
        flex-direction: column;
        gap: var(--space-2);
    }

    .actions-stack .btn {
        width: 100%;
        text-decoration: none;
    }

    /* DonateTrigger renders inside its own component, so reach it via
       :global; the amber button goes full-width like the rest. */
    .actions-stack :global(.donate-trigger.inline) {
        width: 100%;
    }

    .about-ext-link {
        text-decoration: none;
    }

    /* ---------- Footer ---------- */
    .about-footer {
        grid-column: span 12;
        display: flex;
        flex-direction: column;
        gap: var(--space-1);
        padding-top: var(--space-1);
    }

    .about-footer .separator {
        margin-bottom: var(--space-3);
    }

    .about-credit {
        font-size: var(--text-xs);
        line-height: var(--leading-xs);
        color: var(--text-muted);
        margin: 0;
    }

    .about-watermark {
        font-size: var(--text-xs);
        color: var(--text-dim);
        text-decoration: none;
        width: fit-content;
        transition: color var(--duration-fast) var(--ease-out);
    }

    @media (hover: hover) {
        .about-watermark:hover {
            color: var(--text);
        }
    }

    @media (prefers-reduced-motion: reduce) {
        .nav-tile,
        .nav-chevron,
        .about-watermark {
            transition: none;
        }
    }

    @media (max-width: 720px) {
        .hl-wide,
        .hl-mid,
        .hl-narrow,
        .nav-tile,
        .about-credits,
        .about-actions {
            grid-column: span 12;
        }
    }

    @media (max-width: 520px) {
        .about-hero {
            flex-direction: column;
            align-items: center;
            text-align: center;
        }

        .about-name-row {
            justify-content: center;
        }

        .about-identity {
            align-items: center;
        }
    }
</style>
