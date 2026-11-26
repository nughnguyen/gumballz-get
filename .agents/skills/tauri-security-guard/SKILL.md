# Tauri Security Guard

- Grant Tauri v2 capabilities with least privilege. Limit filesystem, shell, dialog, HTTP, and opener scopes to required app data, downloads, and named media sidecars.
- Keep CSP strict: no `unsafe-eval`; allow `https://img.vietqr.io` explicitly in `img-src` only.
- Pass process arguments as discrete `.arg(...)` values. Never interpolate user input into shell command strings.
- Sanitize output names and reject traversal components before filesystem writes.
- Bind local bridges only to `127.0.0.1`; compare pairing secrets in constant time and validate `Origin`.
- Redact cookies and credentials in logs. Sanitize untrusted HTML with DOMPurify before rendering.
- Preserve SHA-256 integrity checks for downloaded media tools.

