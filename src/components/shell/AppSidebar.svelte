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
  import NavIcon from "$components/shell/NavIcon.svelte";
  import { getUpdateInfo } from "$lib/stores/update-store.svelte";
  import type { NavItem } from "$lib/nav-config";
  import { isMac } from "$lib/platform";
  import DonateTrigger from "$components/donate/DonateTrigger.svelte";

  interface Props {
    primaryNav?: NavItem[];
    appNav?: NavItem[];
    pluginNav?: NavItem[];
    badgeLabel?: string;
    badgeCount?: number;
    collapsed?: boolean;
    onToggleCollapsed?: () => void;
  }

  let {
    primaryNav = [],
    appNav = [],
    pluginNav = [],
    badgeLabel = "",
    badgeCount = 0,
    collapsed = false,
    onToggleCollapsed,
  }: Props = $props();

  let updateInfo = $derived(getUpdateInfo());
  const shortcutLabel = isMac() ? "⌃⌘S" : "Ctrl+Shift+S";

  const PLUGINS_KEY = "gumballz-get.sidebar.plugins_expanded";
  let pluginsExpanded = $state(
    typeof localStorage === "undefined"
      ? true
      : localStorage.getItem(PLUGINS_KEY) !== "0",
  );

  function togglePlugins() {
    pluginsExpanded = !pluginsExpanded;
    try {
      localStorage.setItem(PLUGINS_KEY, pluginsExpanded ? "1" : "0");
    } catch {}
  }

  function isActive(href: string): boolean {
    if (href === "/") return page.url.pathname === "/";
    return page.url.pathname.startsWith(href);
  }

  function itemTitle(item: NavItem): string {
    return item.label || (item.labelKey ? $t(item.labelKey) : "");
  }

  let tip = $state<{ text: string; top: number; left: number } | null>(null);
  let tipTimer: ReturnType<typeof setTimeout> | undefined;
  let warmTimer: ReturnType<typeof setTimeout> | undefined;
  let warm = false;

  function showTip(event: MouseEvent | FocusEvent, text: string) {
    if (!collapsed) return;
    const el = event.currentTarget as HTMLElement;
    const place = () => {
      const r = el.getBoundingClientRect();
      tip = { text, top: r.top + r.height / 2, left: r.right + 8 };
      warm = true;
    };
    clearTimeout(tipTimer);
    clearTimeout(warmTimer);
    if (warm || event.type === "focus") place();
    else tipTimer = setTimeout(place, 300);
  }

  function hideTip() {
    clearTimeout(tipTimer);
    tip = null;
    warmTimer = setTimeout(() => (warm = false), 250);
  }

  $effect(() => {
    if (!collapsed) tip = null;
  });
</script>

{#snippet navLink(item: NavItem)}
  {@const title = itemTitle(item)}
  {@const active = isActive(item.href)}
  <a
    href={item.href}
    class="mac-nav-item"
    class:active
    title={collapsed ? undefined : title}
    aria-label={item.badge === "downloads" && badgeCount > 0 ? `${title} (${badgeLabel})` : title}
    aria-current={active ? "page" : undefined}
    onmouseenter={(e) => showTip(e, title)}
    onmouseleave={hideTip}
    onfocus={(e) => showTip(e, title)}
    onblur={hideTip}
  >
    <NavIcon icon={item.icon} iconSvg={item.iconSvg} size={collapsed ? 30 : 26} {active} />
    <span class="mac-nav-label">{title}</span>
    {#if item.badge === "downloads" && badgeCount > 0}
      <span class="mac-nav-badge live">{badgeLabel}</span>
    {:else if item.href === "/settings" && updateInfo.available}
      <span class="mac-nav-badge live update-badge">UPDATE</span>
    {/if}
  </a>
{/snippet}

<aside class="mac-source-list" class:collapsed aria-label={$t("nav.section_primary")}>
  <div class="mac-source-list-drag" data-tauri-drag-region></div>

  {#if pluginNav.length > 0}
    <nav class="mac-nav-section" aria-label={$t("nav.section_plugins")}>
      <div class="mac-nav-section-header">
        <span>{$t("nav.section_plugins")}</span>
        {#if !collapsed}
          <button
            type="button"
            class="mac-plugins-toggle"
            onclick={togglePlugins}
            aria-expanded={pluginsExpanded}
            aria-label={pluginsExpanded ? $t("nav.collapse_plugins") : $t("nav.expand_plugins")}
          >
            <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              {#if pluginsExpanded}
                <path d="M4 6l4 4 4-4" />
              {:else}
                <path d="M6 4l4 4-4 4" />
              {/if}
            </svg>
          </button>
        {/if}
      </div>
      {#if collapsed || pluginsExpanded}
        {#each pluginNav as item (item.href)}
          {@render navLink(item)}
        {/each}
      {/if}
    </nav>
  {/if}

  <nav class="mac-nav-section" aria-label={$t("nav.section_primary")}>
    {#each primaryNav as item (item.href)}
      {@render navLink(item)}
    {/each}
  </nav>

  <nav class="mac-nav-section" aria-label={$t("nav.section_app")}>
    <div class="mac-nav-section-header">{$t("nav.section_app")}</div>
    {#each appNav as item (item.href)}
      {@render navLink(item)}
    {/each}
  </nav>

  <div class="mac-source-list-footer">
    <DonateTrigger compact={collapsed} />
    <button
      type="button"
      class="mac-sidebar-collapse"
      onclick={onToggleCollapsed}
      aria-label={`${$t(collapsed ? "nav.expand_sidebar" : "nav.collapse_sidebar")} ${shortcutLabel}`}
      aria-keyshortcuts={isMac() ? "Control+Meta+S" : "Control+Shift+S"}
      aria-expanded={!collapsed}
      title={$t(collapsed ? "nav.expand_sidebar" : "nav.collapse_sidebar")}
      data-ui-tooltip={$t(collapsed ? "nav.expand_sidebar" : "nav.collapse_sidebar")}
      onmouseenter={(e) => showTip(e, `${collapsed ? $t("nav.expand_sidebar") : $t("nav.collapse_sidebar")} ${shortcutLabel}`)}
      onmouseleave={hideTip}
      onfocus={(e) => showTip(e, `${collapsed ? $t("nav.expand_sidebar") : $t("nav.collapse_sidebar")} ${shortcutLabel}`)}
      onblur={hideTip}
    >
      <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <rect x="2.5" y="3.5" width="15" height="13" rx="3" />
        <path d="M7.5 3.5v13" />
        {#if collapsed}
          <path d="M11 8l2 2-2 2" />
        {:else}
          <path d="M13 8l-2 2 2 2" />
        {/if}
      </svg>
      <span class="mac-sidebar-collapse-label">{$t("nav.collapse_sidebar")}</span>
      <kbd class="mac-sidebar-collapse-kbd" aria-hidden="true">{shortcutLabel}</kbd>
    </button>
  </div>
</aside>

{#if tip}
  <div class="mac-rail-tip" role="tooltip" style:top="{tip.top}px" style:left="{tip.left}px">{tip.text}</div>
{/if}

<style>
  .mac-source-list-footer {
    margin-top: auto;
    padding-top: var(--space-2);
  }

  .mac-sidebar-collapse {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    width: 100%;
    padding: 0 6px 0 8px;
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
    flex-shrink: 0;
    overflow: hidden;
    transition: background var(--duration-fast, 120ms) var(--ease-out, ease-out), color var(--duration-fast, 120ms) var(--ease-out, ease-out);
  }

  .mac-sidebar-collapse:hover {
    background: var(--surface-hi);
    color: var(--text);
  }

  .mac-sidebar-collapse:active {
    transform: translate(2px, 2px);
  }

  .mac-sidebar-collapse:focus-visible {
    outline: var(--focus-ring);
    outline-offset: -2px;
  }

  .mac-sidebar-collapse-label {
    flex: 1;
    min-width: 0;
    text-align: start;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mac-sidebar-collapse-kbd {
    font: inherit;
    font-size: var(--text-caption);
    color: var(--text);
    padding: 1px 4px;
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-xs);
    background: var(--surface);
    flex-shrink: 0;
  }

  :global([data-sidebar="collapsed"]) .mac-sidebar-collapse,
  .collapsed .mac-sidebar-collapse {
    justify-content: center;
    padding: 0;
  }

  :global([data-sidebar="collapsed"]) .mac-sidebar-collapse-label,
  .collapsed .mac-sidebar-collapse-label,
  :global([data-sidebar="collapsed"]) .mac-sidebar-collapse-kbd,
  .collapsed .mac-sidebar-collapse-kbd {
    display: none;
  }

  @media (max-width: 860px) {
    .mac-sidebar-collapse {
      justify-content: center;
      padding: 0;
    }

    .mac-sidebar-collapse-label,
    .mac-sidebar-collapse-kbd {
      display: none;
    }
  }

  .mac-rail-tip {
    position: fixed;
    transform: translateY(-50%);
    padding: 4px 9px;
    border: 2px solid var(--border-ink);
    border-radius: var(--radius-md);
    background: var(--surface);
    color: var(--text);
    font-size: var(--text-xs);
    font-weight: 500;
    white-space: nowrap;
    box-shadow:
      inset 0 0 0 var(--hairline) var(--border-hi),
      0 6px 18px rgba(var(--shadow-ink), 0.18);
    pointer-events: none;
    z-index: 1000;
    animation: tip-in 140ms var(--ease-out);
  }

  @keyframes tip-in {
    from { opacity: 0; transform: translateY(-50%) translateX(-4px); }
    to { opacity: 1; transform: translateY(-50%) translateX(0); }
  }
</style>
