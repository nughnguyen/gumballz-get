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

    const cards = [
        { href: "/about/changelog", titleKey: "about.card_changelog_title", descKey: "about.card_changelog_desc" },
        { href: "/about/project", titleKey: "about.card_project_title", descKey: "about.card_project_desc" },
        { href: "/about/terms", titleKey: "about.card_terms_title", descKey: "about.card_terms_desc" },
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

<div class="about-overview">
    <header class="about-hero">
        <BrandMark size={72} />
        <div class="about-identity">
            <div class="about-name-row">
                <h1>GumballZ Get</h1>
                {#if version}
                    <span class="tag about-version">{$t("about.version")} {version}</span>
                {/if}
            </div>
            <p class="about-tagline">{$t("about.tagline")}</p>
            <p class="about-desc">{$t("about.description")}</p>
            {#if buildDetails}
                <span class="about-build">{buildDetails}</span>
            {/if}
        </div>
    </header>

    <div class="about-cards">
        {#each cards as card}
            <a href={card.href} class="surface-card interactive about-card">
                <span class="list-row">
                    <span class="list-row-content">
                        <span class="list-row-title">{$t(card.titleKey)}</span>
                        <span class="list-row-sub about-card-desc">{$t(card.descKey)}</span>
                    </span>
                    <span class="list-row-trailing about-card-chevron" aria-hidden="true">›</span>
                </span>
            </a>
        {/each}
    </div>

    <section class="about-credits bento-card">
        <div><span class="credit-label">Maintained by</span><strong>Nguyen Quoc Hung (GumballZ)</strong></div>
        <div><span class="credit-label">License</span><strong>GPL-3.0</strong></div>
        <a class="btn about-ext-link" href="https://github.com/nughnguyen" target="_blank" rel="noopener">GitHub profile</a>
    </section>

    <div class="about-external">
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

    <footer class="about-footer">
        <hr class="separator" />
        <p class="about-credit">{$t("about.credit")}</p>
        <a href="https://github.com/nughnguyen" class="about-watermark" onclick={openAuthorGithub} title="@Nguyen Quoc Hung (GumballZ)">
            @Nguyen Quoc Hung (GumballZ)
        </a>
    </footer>
</div>

<style>
    .about-overview {
        display: flex;
        flex-direction: column;
        gap: var(--space-6);
    }

    .about-hero {
        display: flex;
        align-items: flex-start;
        gap: var(--space-5);
        padding: var(--space-2) 0;
    }

    .about-app-icon {
        width: 72px;
        height: 72px;
        border-radius: 17px;
        object-fit: cover;
        box-shadow: 0 8px 20px rgba(var(--shadow-ink), var(--elev-alpha-2)), 0 0 0 var(--hairline) var(--content-border);
        flex-shrink: 0;
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
        font-size: var(--text-2xl);
        line-height: var(--leading-2xl);
        font-weight: 700;
        letter-spacing: var(--track-tight);
        margin: 0;
    }

    .about-tagline {
        font-size: var(--text-md);
        line-height: var(--leading-md);
        color: var(--text-muted);
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

    .about-cards {
        display: flex;
        flex-direction: column;
        gap: 0;
        background: var(--surface);
        border: 2px solid var(--border-ink);
        border-radius: 12px;
        box-shadow: 4px 4px 0 var(--shadow-hard);
        overflow: hidden;
    }

    .about-card {
        display: block;
        text-decoration: none;
        color: inherit;
        border-radius: 0;
        box-shadow: none;
        position: relative;
    }

    .about-card + .about-card::before {
        content: "";
        position: absolute;
        top: 0;
        left: var(--space-4);
        right: 0;
        height: var(--hairline);
        background: var(--separator);
    }

    .about-card .list-row {
        padding: var(--space-3) var(--space-4);
        border-radius: 0;
    }

    .about-card-desc {
        white-space: normal;
    }

    .about-card-chevron {
        font-size: var(--text-lg);
        line-height: 1;
    }

    .about-external {
        display: flex;
        gap: var(--space-2);
        flex-wrap: wrap;
    }

    .about-credits {
        display: flex;
        align-items: center;
        justify-content: space-between;
        flex-wrap: wrap;
        gap: var(--space-4);
        padding: var(--space-4);
        background: var(--pane);
    }

    .about-credits > div { display: grid; gap: 4px; }
    .credit-label { color: var(--text-muted); font-size: var(--text-xs); text-transform: uppercase; letter-spacing: .06em; }

    .about-ext-link {
        text-decoration: none;
    }

    .about-footer {
        display: flex;
        flex-direction: column;
        gap: var(--space-1);
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
        .about-watermark {
            transition: none;
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
    }
</style>
