# GumballZ Get Companion Extension

Browser extension for **GumballZ Get** (Chrome, Edge, Brave, and Firefox).

Detect media streams, cookies, and video links directly from your browser and download them standalone or route them to **GumballZ Get Desktop**.

---

## Installation Guide

### Chrome, Edge, Brave, Opera (Chromium)

1. Clone or download this repository.
2. Open your browser and navigate to:
   - Chrome: `chrome://extensions`
   - Edge: `edge://extensions`
   - Brave: `brave://extensions`
3. Enable **Developer mode** (toggle in the top-right corner).
4. Click **Load unpacked** (or *Tải tiện ích đã giải nén*).
5. Select the [`browser-extension/chrome`](./chrome) directory in this repository.
6. The GumballZ Get Companion icon will appear in your browser extension toolbar!

### Firefox

1. Open Firefox and go to `about:debugging#/runtime/this-firefox`.
2. Click **Load Temporary Add-on**.
3. Select the `manifest.json` file inside [`browser-extension/firefox`](./firefox).

---

## Build Extension Archive (.zip)

To create a packaged `.zip` ready for distribution:

```bash
pnpm build:extension
# Output will be located in dist/
```

---

## Pairing with Desktop App

1. Launch **GumballZ Get Desktop**.
2. Go to **Settings → Network → Browser Extension**.
3. Click **Pair extension** to connect the extension with your local desktop engine.
