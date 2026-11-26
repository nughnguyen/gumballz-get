import test from "node:test";
import assert from "node:assert/strict";

import { buildGumballzSchemeUrl, openGumballzScheme } from "../src/send-via-scheme.js";

test("buildGumballzSchemeUrl strips https prefix and preserves http protocol in gumballzget://", () => {
  assert.equal(
    buildGumballzSchemeUrl("https://www.youtube.com/watch?v=abc"),
    "gumballzget://www.youtube.com/watch?v=abc"
  );
  assert.equal(
    buildGumballzSchemeUrl("http://example.com/v"),
    "gumballzget://http://example.com/v"
  );
});

test("buildGumballzSchemeUrl preserves an already-gumballz-get URL", () => {
  assert.equal(
    buildGumballzSchemeUrl("gumballzget://example.com/v"),
    "gumballzget://example.com/v"
  );
});

test("buildGumballzSchemeUrl wraps magnet/p2p URLs without an extra slash", () => {
  assert.equal(
    buildGumballzSchemeUrl("magnet:?xt=urn:btih:abc"),
    "gumballzget:magnet:?xt=urn:btih:abc"
  );
  assert.equal(
    buildGumballzSchemeUrl("p2p:foo"),
    "gumballzget:p2p:foo"
  );
});

test("buildGumballzSchemeUrl returns null for empty / non-string input", () => {
  assert.equal(buildGumballzSchemeUrl(""), null);
  assert.equal(buildGumballzSchemeUrl("   "), null);
  assert.equal(buildGumballzSchemeUrl(null), null);
  assert.equal(buildGumballzSchemeUrl(undefined), null);
  assert.equal(buildGumballzSchemeUrl(42), null);
});

test("openGumballzScheme creates a hidden tab and schedules its removal", async () => {
  const events = [];
  const scheduled = [];

  const tabs = {
    create: async (options) => {
      events.push(["create", options]);
      return { id: 1234 };
    },
    remove: (id) => {
      events.push(["remove", id]);
      return Promise.resolve();
    },
  };

  const schedule = (fn, ms) => {
    scheduled.push({ fn, ms });
    return null;
  };

  const result = await openGumballzScheme("https://example.com/v", {
    tabs,
    schedule,
    tabLifetimeMs: 500,
  });

  assert.deepEqual(result, { ok: true, schemeUrl: "gumballzget://example.com/v" });
  assert.deepEqual(events, [
    ["create", { url: "gumballzget://example.com/v", active: false }],
  ]);
  assert.equal(scheduled.length, 1);
  assert.equal(scheduled[0].ms, 500);

  // Run the scheduled cleanup: the throwaway tab should be removed.
  scheduled[0].fn();
  assert.deepEqual(events[1], ["remove", 1234]);
});

test("openGumballzScheme does not navigate the user's active tab", async () => {
  let updateCalled = false;
  const tabs = {
    create: async () => ({ id: 99 }),
    update: async () => {
      updateCalled = true;
    },
    remove: () => Promise.resolve(),
  };

  const result = await openGumballzScheme("https://example.com/v", {
    tabs,
    schedule: () => {},
  });

  assert.equal(result.ok, true);
  assert.equal(updateCalled, false);
});

test("openGumballzScheme returns invalid-url for empty input", async () => {
  const result = await openGumballzScheme("", {
    tabs: { create: async () => ({ id: 1 }), remove: () => Promise.resolve() },
    schedule: () => {},
  });
  assert.deepEqual(result, { ok: false, reason: "invalid-url" });
});

test("openGumballzScheme returns tabs-api-unavailable when tabs.create is missing", async () => {
  const result = await openGumballzScheme("https://example.com/v", {
    tabs: {},
    schedule: () => {},
  });
  assert.deepEqual(result, { ok: false, reason: "tabs-api-unavailable" });
});

test("openGumballzScheme reports tabs-create-failed when tabs.create throws", async () => {
  const tabs = {
    create: async () => {
      throw new Error("permission denied");
    },
    remove: () => Promise.resolve(),
  };

  const result = await openGumballzScheme("https://example.com/v", {
    tabs,
    schedule: () => {},
  });

  assert.deepEqual(result, {
    ok: false,
    reason: "tabs-create-failed",
    message: "permission denied",
  });
});

test("openGumballzScheme tolerates a sync remove() that throws", async () => {
  const tabs = {
    create: async () => ({ id: 7 }),
    remove: () => {
      throw new Error("tab is gone");
    },
  };

  const calls = [];
  const schedule = (fn) => {
    calls.push(fn);
  };

  const result = await openGumballzScheme("https://example.com/v", {
    tabs,
    schedule,
  });

  assert.equal(result.ok, true);
  assert.doesNotThrow(() => calls[0]());
});
