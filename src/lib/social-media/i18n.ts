/**
 * The Social Media download engine reports stage names and status messages in
 * English from the Rust side. This module maps the known ones onto the active
 * locale and passes everything else through untouched.
 */

export type SocialMediaTranslate = (key: string, values?: Record<string, unknown>) => string;

const STAGE_KEYS: Record<string, string> = {
  setup: 'social_media.stage_setup',
  photos: 'social_media.stage_photos',
  videos: 'social_media.stage_videos',
  stories: 'social_media.stage_stories',
  highlights: 'social_media.stage_highlights'
};

const STATUS_KEYS: Record<string, string> = {
  'Download cancelled': 'social_media.status_cancelled',
  'Provisioning download engine binary...': 'social_media.status_provisioning',
  'Switching to secondary video engine (yt-dlp)...': 'social_media.status_switching_engine',
  'Engine not found (auto-download available)': 'social_media.engine_missing',
  'All download engines ready': 'social_media.engines_ready'
};

export function socialMediaStageLabel(
  stage: string | null | undefined,
  t: SocialMediaTranslate
): string {
  if (!stage) return '';
  const key = STAGE_KEYS[stage.trim().toLowerCase()];
  return key ? t(key) : stage;
}

export function socialMediaStatusMessage(
  message: string | null | undefined,
  t: SocialMediaTranslate
): string {
  if (!message) return '';
  const text = message.trim();

  const exact = STATUS_KEYS[text];
  if (exact) return t(exact);

  if (text.startsWith('Starting ')) {
    return t('social_media.status_starting', {
      stage: socialMediaStageLabel(text.slice('Starting '.length), t)
    });
  }
  if (text.startsWith('Downloaded: ')) {
    return t('social_media.status_downloaded', { file: text.slice('Downloaded: '.length) });
  }
  const success = /^Successfully downloaded (\d+) files$/.exec(text);
  if (success) {
    return t('social_media.status_success', { count: success[1] });
  }

  return text;
}
