export type PreviewQuality = {
  label: string;
  url: string;
  format: string;
  width: number;
  height: number;
};

export type MediaPreview = {
  url: string;
  title: string;
  author: string;
  thumbnail_url: string | null;
  duration_seconds: number | null;
  /** "video" | "audio" | "photo" | "carousel" | ... — from the probe. */
  media_type: string | null;
  /** Native qualities (photos, slideshow, audio...) for this link. */
  qualities: PreviewQuality[];
};

let currentPreview = $state<MediaPreview | null>(null);

export function getMediaPreview(): MediaPreview | null {
  return currentPreview;
}

export function setMediaPreview(preview: MediaPreview | null) {
  currentPreview = preview;
}

export function clearMediaPreview() {
  currentPreview = null;
}
