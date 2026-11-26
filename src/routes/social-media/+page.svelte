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
  import DownloadManager from '$components/social-media/DownloadManager.svelte';
  import ProfileManager from '$components/social-media/ProfileManager.svelte';
  import HistoryManager from '$components/social-media/HistoryManager.svelte';
  import SettingsManager from '$components/social-media/SettingsManager.svelte';
  import { setToolbar } from '$lib/stores/toolbar-store.svelte';
  import { setDraft } from '$lib/stores/social_media_download_store.svelte';
  import { t } from '$lib/i18n';

  type TabKey = 'download' | 'profiles' | 'history' | 'settings';

  let activeTab = $state<TabKey>('download');

  $effect(() => {
    return setToolbar({
      segments: [
        { id: 'download', label: $t('social_media.tab_download') || 'Home' },
        { id: 'profiles', label: $t('social_media.tab_profiles') || 'Profiles' },
        { id: 'history', label: $t('social_media.tab_history') || 'History' },
        { id: 'settings', label: $t('social_media.tab_settings') || 'Settings' },
      ],
      activeSegment: activeTab,
      onSegment: (id) => {
        activeTab = id as TabKey;
      },
    });
  });

  function switchTab(tab: TabKey) {
    activeTab = tab;
  }
</script>

<svelte:head>
  <title>{$t('social_media.page_title') || 'GumballZ Get — Social Media'}</title>
</svelte:head>

<div class="social-media-container">
  <div class="social-media-content" role="tabpanel" id="tabpanel-{activeTab}">
    {#if activeTab === 'download'}
      <DownloadManager
        onSwitchToProfiles={() => switchTab('profiles')}
        onSwitchToSettings={() => switchTab('settings')}
      />
    {:else if activeTab === 'profiles'}
      <ProfileManager
        onSelectProfileForDownload={(profileUrl) => {
          setDraft({ url: profileUrl, selectedProfileUrl: profileUrl });
          switchTab('download');
        }}
      />
    {:else if activeTab === 'history'}
      <HistoryManager onSwitchToDownload={() => switchTab('download')} />
    {:else}
      <SettingsManager />
    {/if}
  </div>
</div>

<style>
  .social-media-container {
    flex: 1;
    min-height: 0;
    width: 100%;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    overscroll-behavior-y: contain;
    transform: translateZ(0);
    will-change: scroll-position;
  }

  .social-media-content {
    width: 100%;
    max-width: 780px;
    margin: 0 auto;
    padding: var(--space-4) var(--space-5) var(--space-7);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
