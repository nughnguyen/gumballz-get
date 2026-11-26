let draftUrl = $state("");

export function getDownloadboxDraftUrl(): string {
  return draftUrl;
}

export function setDownloadboxDraftUrl(url: string) {
  draftUrl = url;
}

export function clearDownloadboxDraftUrl() {
  draftUrl = "";
}

