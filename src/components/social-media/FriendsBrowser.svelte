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
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import {
    browseFriends,
    browsePosts,
    checkBrowseLogin,
    type BrowsePerson,
    type BrowsePost,
  } from '$lib/api/social-media';
  import { startDownload as startStoreDownload } from '$lib/stores/social_media_download_store.svelte';
  import { getOutputDir, getCookiesFile } from '$lib/stores/social_media_settings_store.svelte';
  import { showToast } from '$lib/stores/toast-store.svelte';
  import { t } from '$lib/i18n';

  type BrowsePlatform = 'facebook' | 'instagram';

  const PLATFORMS: Array<{ key: BrowsePlatform; label: string }> = [
    { key: 'instagram', label: 'Instagram' },
    { key: 'facebook', label: 'Facebook' },
  ];

  let platform = $state<BrowsePlatform>('instagram');
  let loggedIn = $state<boolean | null>(null);
  let friends = $state<BrowsePerson[]>([]);
  let friendsLoading = $state(false);
  let friendsError = $state('');
  let search = $state('');

  let selectedFriend = $state<BrowsePerson | null>(null);
  let posts = $state<BrowsePost[]>([]);
  let postsLoading = $state(false);
  let postsError = $state('');
  let selectedPostIds = $state<Set<string>>(new Set());
  let avatarErrors = $state<Record<string, boolean>>({});

  let outputDir = $derived(getOutputDir());
  let cookiesFile = $derived(getCookiesFile());

  let filteredFriends = $derived.by(() => {
    const q = search.trim().toLowerCase();
    if (!q) return friends;
    return friends.filter(
      (f) =>
        f.name.toLowerCase().includes(q) ||
        (f.username ?? '').toLowerCase().includes(q),
    );
  });

  let selectedCount = $derived(selectedPostIds.size);
  let allSelected = $derived(posts.length > 0 && selectedPostIds.size === posts.length);

  async function loadFriends() {
    friendsLoading = true;
    friendsError = '';
    selectedFriend = null;
    posts = [];
    selectedPostIds = new Set();
    try {
      const status = await checkBrowseLogin(platform);
      loggedIn = status.logged_in;
      if (!status.logged_in) {
        friends = [];
        return;
      }
      friends = await browseFriends(platform);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      // A login-shaped failure also means "not logged in" for this UI.
      if (/đăng nhập|login|session|cookie/i.test(msg)) {
        loggedIn = false;
      }
      friendsError = msg;
      friends = [];
    } finally {
      friendsLoading = false;
    }
  }

  async function openFriend(friend: BrowsePerson) {
    selectedFriend = friend;
    posts = [];
    postsError = '';
    selectedPostIds = new Set();
    postsLoading = true;
    try {
      posts = await browsePosts(platform, friend.id);
    } catch (err: unknown) {
      postsError = err instanceof Error ? err.message : String(err);
      posts = [];
    } finally {
      postsLoading = false;
    }
  }

  function backToFriends() {
    selectedFriend = null;
    posts = [];
    postsError = '';
    selectedPostIds = new Set();
  }

  function togglePost(id: string) {
    const next = new Set(selectedPostIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedPostIds = next;
  }

  function toggleSelectAll() {
    selectedPostIds = allSelected ? new Set() : new Set(posts.map((p) => p.id));
  }

  function initialsFor(name: string): string {
    const parts = name.trim().split(/\s+/).filter(Boolean);
    if (parts.length === 0) return '?';
    if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
    return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
  }

  function formatPostDate(takenAt: number | null | undefined): string {
    if (!takenAt) return '';
    return new Date(takenAt * 1000).toLocaleDateString('vi-VN');
  }

  async function downloadPosts(postsToDownload: BrowsePost[]) {
    if (!selectedFriend) return;
    if (!outputDir.trim()) {
      showToast('error', $t('social_media.friends_no_output_dir') as string);
      return;
    }
    if (postsToDownload.length === 0) return;
    const friend = selectedFriend;
    const creatorMeta = {
      name: friend.name,
      handle: friend.username ? `@${friend.username}` : friend.name,
      avatar: friend.avatar_url ?? undefined,
      platform,
      platformName: platform === 'instagram' ? 'Instagram' : 'Facebook',
    };
    for (const post of postsToDownload) {
      void startStoreDownload(post.url, outputDir, cookiesFile, 'all', creatorMeta);
    }
    showToast(
      'info',
      $t('social_media.friends_download_added', { count: String(postsToDownload.length) }) as string,
    );
  }

  function switchPlatform(next: BrowsePlatform) {
    if (next === platform) return;
    platform = next;
    search = '';
    loggedIn = null;
    void loadFriends();
  }

  onMount(() => {
    void loadFriends();
  });
</script>

<div class="fb-page">
  <div class="fb-platform-row">
    <div class="segmented segmented-lg" role="tablist" aria-label={$t('social_media.friends_platform') as string}>
      {#each PLATFORMS as p (p.key)}
        <button
          type="button"
          class="segmented-btn"
          class:active={platform === p.key}
          role="tab"
          aria-selected={platform === p.key}
          onclick={() => switchPlatform(p.key)}
        >
          {p.label}
        </button>
      {/each}
    </div>
    {#if selectedFriend === null}
      <input
        class="input fb-search"
        type="search"
        placeholder={$t('social_media.friends_search_placeholder') as string}
        aria-label={$t('social_media.friends_search_placeholder') as string}
        bind:value={search}
      />
    {/if}
  </div>

  {#if selectedFriend === null}
    {#if friendsLoading}
      <div class="empty-state surface-card">
        <div class="spinner" aria-hidden="true"></div>
        <p class="empty-state-title">{$t('social_media.friends_loading')}</p>
      </div>
    {:else if loggedIn === false}
      <div class="empty-state surface-card">
        <p class="empty-state-title">{$t('social_media.friends_not_logged_in_title')}</p>
        <p class="empty-state-hint">
          {$t('social_media.friends_not_logged_in', { platform: platform === 'instagram' ? 'Instagram' : 'Facebook' })}
        </p>
        <button type="button" class="btn btn-primary" onclick={() => goto('/settings')}>
          {$t('social_media.friends_open_settings')}
        </button>
      </div>
    {:else if friendsError}
      <div class="notice notice-danger" role="alert">
        {$t('social_media.friends_load_failed', { error: friendsError })}
      </div>
      <button type="button" class="btn btn-secondary" onclick={() => void loadFriends()}>
        {$t('social_media.friends_retry') ?? 'Thử lại'}
      </button>
    {:else if friends.length === 0}
      <div class="empty-state surface-card">
        <p class="empty-state-title">{$t('social_media.friends_empty')}</p>
      </div>
    {:else}
      <p class="fb-count">{$t('social_media.friends_count', { count: String(filteredFriends.length) })}</p>
      <ul class="fb-friends-list">
        {#each filteredFriends as friend (friend.id + (friend.kind ?? ''))}
          <li>
            <button type="button" class="list-row interactive fb-friend-row" onclick={() => void openFriend(friend)}>
              {#if friend.avatar_url && !avatarErrors[friend.id + (friend.kind ?? '')]}
                <img
                  src={friend.avatar_url}
                  alt=""
                  class="fb-avatar"
                  loading="lazy"
                  onerror={() => {
                    avatarErrors = { ...avatarErrors, [friend.id + (friend.kind ?? '')]: true };
                  }}
                />
              {:else}
                <span class="fb-initials" data-platform={platform}>{initialsFor(friend.name)}</span>
              {/if}
              <span class="list-row-content">
                <span class="list-row-title">{friend.name}</span>
                {#if friend.username}
                  <span class="list-row-sub">@{friend.username}</span>
                {/if}
              </span>
              {#if friend.kind === 'following'}
                <span class="badge badge-neutral">{$t('social_media.friends_following')}</span>
              {:else if friend.kind === 'followers'}
                <span class="badge badge-neutral">{$t('social_media.friends_followers')}</span>
              {/if}
              <span class="fb-chevron" aria-hidden="true">›</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {:else}
    <div class="fb-posts-head">
      <button type="button" class="btn btn-sm btn-ghost" onclick={backToFriends}>
        ‹ {$t('social_media.friends_back')}
      </button>
      <div class="fb-posts-who">
        {#if selectedFriend.avatar_url && !avatarErrors['who']}
          <img
            src={selectedFriend.avatar_url}
            alt=""
            class="fb-avatar"
            loading="lazy"
            onerror={() => {
              avatarErrors = { ...avatarErrors, who: true };
            }}
          />
        {:else}
          <span class="fb-initials" data-platform={platform}>{initialsFor(selectedFriend.name)}</span>
        {/if}
        <span class="list-row-title">{$t('social_media.friends_posts_of', { name: selectedFriend.name })}</span>
      </div>
    </div>

    {#if postsLoading}
      <div class="empty-state surface-card">
        <div class="spinner" aria-hidden="true"></div>
        <p class="empty-state-title">{$t('social_media.friends_loading_posts')}</p>
      </div>
    {:else if postsError}
      <div class="notice notice-danger" role="alert">
        {$t('social_media.friends_load_failed', { error: postsError })}
      </div>
    {:else if posts.length === 0}
      <div class="empty-state surface-card">
        <p class="empty-state-title">{$t('social_media.friends_posts_empty')}</p>
      </div>
    {:else}
      <div class="fb-posts-toolbar">
        <button type="button" class="btn btn-sm btn-ghost" onclick={toggleSelectAll}>
          {allSelected ? $t('social_media.friends_deselect_all') : $t('social_media.friends_select_all')}
        </button>
        <span class="fb-count">{$t('social_media.friends_selected_count', { count: String(selectedCount) })}</span>
        <span class="fb-spacer"></span>
        <button
          type="button"
          class="btn btn-sm btn-secondary"
          disabled={selectedCount === 0}
          onclick={() => void downloadPosts(posts.filter((p) => selectedPostIds.has(p.id)))}
        >
          {$t('social_media.friends_download_selected')}
        </button>
        <button type="button" class="btn btn-sm btn-primary" onclick={() => void downloadPosts(posts)}>
          {$t('social_media.friends_download_all')}
        </button>
      </div>
      <ul class="fb-posts-grid">
        {#each posts as post (post.id)}
          <li>
            <label class="surface-card fb-post-card" class:selected={selectedPostIds.has(post.id)}>
              <input
                type="checkbox"
                class="fb-post-check"
                checked={selectedPostIds.has(post.id)}
                onchange={() => togglePost(post.id)}
                aria-label={post.caption ?? post.url}
              />
              {#if post.thumbnail_url}
                <img src={post.thumbnail_url} alt="" class="fb-post-thumb" loading="lazy" />
              {:else}
                <span class="fb-post-thumb fb-post-thumb-empty" aria-hidden="true">▦</span>
              {/if}
              <span class="fb-post-meta">
                {#if post.caption}
                  <span class="fb-post-caption">{post.caption}</span>
                {/if}
                <span class="fb-post-sub">
                  {#if post.media_type === 'video'}🎬
                  {:else if post.media_type === 'carousel'}▦
                  {:else}🖼️{/if}
                  {#if formatPostDate(post.taken_at)}
                    · {formatPostDate(post.taken_at)}
                  {/if}
                </span>
              </span>
            </label>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .fb-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .fb-platform-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }

  .fb-search {
    flex: 1;
    min-width: 180px;
  }

  .fb-count {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }

  .fb-friends-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .fb-friend-row {
    width: 100%;
    text-align: left;
  }

  .fb-avatar {
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    object-fit: cover;
    flex-shrink: 0;
    border: 2px solid var(--border-ink);
  }

  .fb-initials {
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    font-weight: 700;
    color: #ffffff;
    background: var(--accent);
    border: 2px solid var(--border-ink);
    flex-shrink: 0;
  }

  .fb-initials[data-platform='instagram'] {
    background: linear-gradient(135deg, #833ab4, #c2185b);
  }

  .fb-initials[data-platform='facebook'] {
    background: #0b5ed7;
  }

  .fb-chevron {
    font-size: 20px;
    line-height: 1;
    color: var(--text-faint);
    flex-shrink: 0;
  }

  .fb-posts-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }

  .fb-posts-who {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .fb-posts-toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .fb-spacer {
    flex: 1;
  }

  .fb-posts-grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: var(--space-3);
  }

  .fb-post-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2);
    cursor: pointer;
    position: relative;
  }

  .fb-post-card.selected {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .fb-post-check {
    position: absolute;
    top: var(--space-2);
    left: var(--space-2);
    z-index: 1;
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .fb-post-thumb {
    width: 100%;
    aspect-ratio: 1;
    object-fit: cover;
    border-radius: var(--radius-sm);
    border: 2px solid var(--border-ink);
    background: var(--surface);
  }

  .fb-post-thumb-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 32px;
    color: var(--text-faint);
  }

  .fb-post-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .fb-post-caption {
    font-size: var(--text-sm);
    color: var(--text);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .fb-post-sub {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
</style>
