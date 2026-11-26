import test from "node:test";
import assert from "node:assert/strict";

import { resolveErrorPageContent } from "../pages/error-content.js";

test("uses localized page strings when locale messages are available", () => {
  const localized = {
    error_page_document_title: "GumballZ Get Companion",
    error_page_eyebrow: "GumballZ Get Companion",
    error_page_install_cta: "Install GumballZ Get",
    error_page_open_extensions: "Ouvrir les extensions Chrome",
    error_host_missing_title: "Titre localise",
    error_host_missing_body: "Corps localise",
    error_host_missing_detail: "Detail localise",
  };

  const content = resolveErrorPageContent({
    code: "HOST_MISSING",
    getMessage: (name) => localized[name] ?? "",
  });

  assert.equal(content.documentTitle, "GumballZ Get Companion");
  assert.equal(content.eyebrow, "GumballZ Get Companion");
  assert.equal(content.installLabel, "Install GumballZ Get");
  assert.equal(content.openExtensionsLabel, "Ouvrir les extensions Chrome");
  assert.equal(content.title, "Titre localise");
  assert.equal(content.body, "Corps localise");
  assert.equal(content.detail, "Detail localise");
});

test("falls back to English when locale messages are missing", () => {
  const content = resolveErrorPageContent({
    code: "INVALID_URL",
    getMessage: () => "",
  });

  assert.equal(content.documentTitle, "GumballZ Get Companion");
  assert.equal(content.title, "This page URL cannot be sent to GumballZ Get");
  assert.equal(
    content.detail,
    "Try again from a direct video, reel, post, playlist, or course page."
  );
});

test("prefers the query message for detail text when one is provided", () => {
  const content = resolveErrorPageContent({
    code: "LAUNCH_FAILED",
    message: "Desktop launch failed with exit code 5",
    getMessage: () => "",
  });

  assert.equal(content.detail, "Desktop launch failed with exit code 5");
});

test("falls back to LAUNCH_FAILED content for unknown error codes", () => {
  const content = resolveErrorPageContent({
    code: "SOMETHING_NEW",
    getMessage: () => "",
  });

  assert.equal(content.code, "LAUNCH_FAILED");
  assert.equal(content.title, "GumballZ Get could not be launched from your browser");
});

test("sanitizeInstallUrl rejects malicious and foreign URLs and allows official URLs", async () => {
  const { sanitizeInstallUrl, DEFAULT_INSTALL_URL } = await import("../pages/error-content.js");

  assert.equal(sanitizeInstallUrl(null), DEFAULT_INSTALL_URL);
  assert.equal(sanitizeInstallUrl(""), DEFAULT_INSTALL_URL);
  assert.equal(sanitizeInstallUrl("javascript:alert(1)"), DEFAULT_INSTALL_URL);
  assert.equal(sanitizeInstallUrl("data:text/html,evil"), DEFAULT_INSTALL_URL);
  assert.equal(sanitizeInstallUrl("http://github.com/nughnguyen/gumballz-get"), DEFAULT_INSTALL_URL);
  assert.equal(sanitizeInstallUrl("https://evil.com/fake-update"), DEFAULT_INSTALL_URL);
  assert.equal(
    sanitizeInstallUrl("https://github.com/nughnguyen/gumballz-get/releases/tag/v0.12.0"),
    "https://github.com/nughnguyen/gumballz-get/releases/tag/v0.12.0"
  );
  assert.equal(
    sanitizeInstallUrl("https://gumballz-get.org/download"),
    "https://gumballz-get.org/download"
  );
});

