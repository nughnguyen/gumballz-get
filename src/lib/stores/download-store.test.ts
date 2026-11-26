import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";

type DownloadStore = typeof import("./download-store.svelte");

let store: DownloadStore;

const queueItem = (id: number) => ({
  id,
  url: "https://example.com/video",
  platform: "youtube",
  title: "Example video",
  status: { type: "Active" as const },
  percent: 0,
  speed_bytes_per_sec: 0,
  downloaded_bytes: 0,
  total_bytes: 100,
  file_path: null,
  file_size_bytes: null,
  file_count: null,
  thumbnail_url: null,
});

beforeAll(async () => {
  vi.stubGlobal("$state", <T>(value: T) => value);
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    callback(0);
    return 0;
  });
  store = await import("./download-store.svelte");
});

afterEach(() => {
  store.syncQueueState([]);
});

afterAll(() => {
  vi.unstubAllGlobals();
});

describe("generic download progress", () => {
  it("does not recreate an item removed by an authoritative queue update", () => {
    const id = 901;
    store.syncQueueState([queueItem(id)]);
    store.syncQueueState([]);

    store.upsertGenericProgress(id, "Example video", "youtube", 50, 10, 50, 100, "downloading");

    expect(store.getDownloads().has(id)).toBe(false);
  });

  it("accepts progress again after the queue restores the same item", () => {
    const id = 902;
    store.syncQueueState([queueItem(id)]);
    store.syncQueueState([]);
    store.syncQueueState([queueItem(id)]);

    store.upsertGenericProgress(id, "Example video", "youtube", 50, 10, 50, 100, "downloading");

    expect(store.getDownloads().get(id)).toMatchObject({
      kind: "generic",
      percent: 50,
      downloadedBytes: 50,
      status: "downloading",
    });
  });
});

describe("finished ordering (completedAt)", () => {
  const completeItem = (id: number) => ({
    ...queueItem(id),
    status: { type: "Complete" as const },
    percent: 100,
  });

  it("stamps completedAt when an item turns complete", () => {
    const id = 910;
    store.syncQueueState([queueItem(id)]);
    store.syncQueueState([completeItem(id)]);

    const item = store.getDownloads().get(id);
    expect(item?.status).toBe("complete");
    expect(item?.completedAt).toEqual(expect.any(Number));
  });

  it("does not refresh completedAt on later syncs", () => {
    const id = 911;
    store.syncQueueState([queueItem(id)]);
    store.syncQueueState([completeItem(id)]);
    const first = store.getDownloads().get(id)?.completedAt;

    store.syncQueueState([completeItem(id)]);

    expect(store.getDownloads().get(id)?.completedAt).toBe(first);
  });

  it("leaves completedAt unset when first seen already complete", () => {
    const id = 912;
    store.syncQueueState([completeItem(id)]);

    expect(store.getDownloads().get(id)?.completedAt).toBeUndefined();
  });

  it("clears completedAt when a finished item runs again", () => {
    const id = 913;
    store.syncQueueState([queueItem(id)]);
    store.syncQueueState([completeItem(id)]);
    expect(store.getDownloads().get(id)?.completedAt).toEqual(expect.any(Number));

    store.syncQueueState([queueItem(id)]);

    expect(store.getDownloads().get(id)?.completedAt).toBeUndefined();
  });
});
