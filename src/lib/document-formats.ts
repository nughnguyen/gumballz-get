// Document platforms offer only document formats in the format picker:
// media choices (video/audio) do not apply to them. Keyed by the platform
// string returned by the backend `detect_platform` command
// (Platform::from_url) — no second detector here.
//
// Only list formats the platform's downloader really produces:
// - scribd: the Scribd downloader builds a PDF or a DOCX from the
//   text layers (PDF by default, DOCX when the format is "docx").
// - gdocs: Google's export endpoint serves PDF and DOCX for documents;
//   the backend filters any other request back to PDF, so a DOCX pick
//   on Slides/Sheets/published docs still downloads as PDF.
export const DOCUMENT_FORMATS: Record<string, string[]> = {
  scribd: ["pdf", "docx"],
  gdocs: ["pdf", "docx"],
};

/**
 * Document formats for a detected platform, or null for media/unknown
 * links — those keep the current behaviour (show all formats).
 */
export function documentFormatsFor(platform: string): string[] | null {
  return DOCUMENT_FORMATS[platform] ?? null;
}
