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
  import { page } from "$app/state";
  import { t } from "$lib/i18n";
  import { isDebugEnabled } from "$lib/stores/debug-store.svelte";
  import type { Snippet } from "svelte";

  let { children }: { children: Snippet } = $props();

  const ALL_TABS = [
    { href: "/about", labelKey: "about.tab.overview", icon: "overview" },
    { href: "/about/changelog", labelKey: "about.tab.changelog", icon: "changelog" },
    { href: "/about/project", labelKey: "about.tab.project", icon: "project" },
    { href: "/about/terms", labelKey: "about.tab.terms", icon: "terms" },
    { href: "/about/debug", labelKey: "about.tab.debug", icon: "debug" },
  ] as const;

  let visibleTabs = $derived(
    isDebugEnabled() ? ALL_TABS : ALL_TABS.filter((tab) => tab.href !== "/about/debug")
  );

  function isActive(href: string): boolean {
    if (href === "/about") {
      return page.url.pathname === "/about";
    }
    return page.url.pathname === href || page.url.pathname.startsWith(`${href}/`);
  }

  const COLLAPSED_KEY = "gumballz-get.about.sidebar_collapsed";
  let collapsed = $state(
    typeof localStorage === "undefined"
      ? false
      : localStorage.getItem(COLLAPSED_KEY) === "1",
  );

  function toggleCollapsed() {
    collapsed = !collapsed;
    try {
      localStorage.setItem(COLLAPSED_KEY, collapsed ? "1" : "0");
    } catch {}
  }
</script>

<div class="about-layout">
  <nav class="about-sidebar" class:collapsed aria-label={$t("nav.about")}>
    {#each visibleTabs as tab}
      <a
        href={tab.href}
        class="about-nav-item"
        class:active={isActive(tab.href)}
      >
        <span class="about-nav-icon" aria-hidden="true">
          {#if tab.icon === "overview"}
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
          {:else if tab.icon === "changelog"}
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>
          {:else if tab.icon === "project"}
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/></svg>
          {:else if tab.icon === "terms"}
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
          {:else}
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
          {/if}
        </span>
        <span class="about-nav-label">{$t(tab.labelKey)}</span>
      </a>
    {/each}
    <button
      type="button"
      class="about-sidebar-toggle"
      onclick={toggleCollapsed}
      aria-expanded={!collapsed}
      aria-label={$t(collapsed ? "nav.expand_sidebar" : "nav.collapse_sidebar")}
      title={$t(collapsed ? "nav.expand_sidebar" : "nav.collapse_sidebar")}
    >
      <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M10 3L5 8l5 5" />
      </svg>
    </button>
  </nav>

  <div class="about-content">
    {@render children()}
  </div>
</div>

<style>
  .about-layout {
    display: flex;
    gap: var(--space-6);
    max-width: 880px;
    margin: 0 auto;
    padding: var(--space-5) var(--space-6) var(--space-8);
    align-items: flex-start;
  }

  .about-sidebar {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: 176px;
    flex-shrink: 0;
    position: sticky;
    top: var(--space-5);
    transition: width var(--duration-slow) var(--ease-out);
  }

  .about-nav-item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 40px;
    padding: 0 var(--space-3);
    font-size: var(--text-base);
    font-weight: 400;
    color: var(--text);
    border: 2px solid transparent;
    border-radius: var(--radius-sm);
    text-decoration: none;
    white-space: nowrap;
    overflow: hidden;
    transition: background var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out), padding var(--duration-slow) var(--ease-out);
  }

  .about-nav-label {
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: opacity var(--duration-base) var(--ease-out), transform var(--duration-base) var(--ease-out), max-width var(--duration-slow) var(--ease-out);
  }

  .about-sidebar-toggle {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 32px;
    margin-top: var(--space-2);
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
  }

  .about-sidebar-toggle:hover {
    background: var(--surface-hi);
    color: var(--text);
  }

  .about-sidebar-toggle:focus-visible {
    outline: var(--focus-ring);
    outline-offset: -2px;
  }

  .about-sidebar-toggle svg {
    transition: transform var(--duration-slow) var(--ease-spring);
  }

  @media (min-width: 641px) {
    .about-sidebar.collapsed {
      width: 56px;
    }

    .about-sidebar.collapsed .about-nav-item {
      justify-content: center;
      padding: 0;
    }

    .about-sidebar.collapsed .about-nav-label {
      opacity: 0;
      transform: translateX(-8px);
      max-width: 0;
    }

    .about-sidebar.collapsed .about-sidebar-toggle svg {
      transform: rotate(180deg);
    }
  }

  @media (hover: hover) {
    .about-nav-item:hover:not(.active) {
      background: var(--fill-1);
      box-shadow: 2px 2px 0 var(--shadow-hard);
    }
  }

  .about-nav-item.active {
    background: var(--accent);
    color: var(--on-accent);
    font-weight: 600;
    border: 2px solid var(--border-ink);
    box-shadow: 2px 2px 0 var(--shadow-hard);
    cursor: default;
  }

  .about-nav-item:focus-visible {
    outline: var(--focus-ring);
    outline-offset: -2px;
  }

  .about-nav-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: var(--fill-2);
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .about-nav-icon svg {
    width: 13px;
    height: 13px;
  }

  .about-nav-item.active .about-nav-icon {
    background: color-mix(in srgb, var(--on-accent) 14%, transparent);
    color: inherit;
  }

  .about-content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  @media (max-width: 640px) {
    .about-layout {
      flex-direction: column;
    }

    .about-sidebar {
      width: 100%;
      position: static;
      flex-direction: row;
      flex-wrap: wrap;
    }

    .about-sidebar-toggle {
      display: none;
    }

    .about-nav-item {
      flex: 1 1 auto;
    }
  }
</style>
