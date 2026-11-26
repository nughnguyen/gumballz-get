# GumballZ Get Agent Instructions

GumballZ Get is a Tauri v2 desktop downloader under GPL-3.0. Maintained by Nguyen Quoc Hung (GumballZ).

## Architecture

- Tauri v2 and Rust backend: `src-tauri/`.
- Svelte 5 frontend: `src/`.
- Browser companion: `browser-extension/`.
- Product identity and donate settings are maintained in the corresponding frontend and Tauri config.

## Security

Follow `.agents/skills/tauri-security-guard/SKILL.md` for backend and webview changes. Preserve command signatures and apply `.agents/skills/gumballz-brutalist-ui/SKILL.md` for UI work. Plugin contributions must follow `.agents/skills/gumballz-plugin-sdk/SKILL.md`.

## Verification

Use the stage-specific gates in the project task plan. UI changes follow `.agents/skills/ui-lint/SKILL.md`; run `pnpm lint:ui <target>` for changed routes. Format Rust with `pnpm fmt:rs`; check frontend with `pnpm check` and translations with `pnpm check:i18n`.

## Standing rules — read before any feature or fix

### Version — bump only when Nguyễn says so
- The version lives in `package.json`, `src-tauri/Cargo.toml` (+ `Cargo.lock`), `src-tauri/tauri.conf.json`, `browser-extension/chrome/manifest.json`, `browser-extension/firefox/manifest.json`, `src/lib/stores/changelog-store.svelte.ts` (`currentVersion` fallback), `src/routes/about/project/+page.svelte` (`APP_VERSION`), `SECURITY.md`.
- RULE (changed 2026-11-26): do NOT bump the version on every commit — not every commit is a new version. Bump only when Nguyễn explicitly says to raise the version ("nâng ver", "lên 1.0.2"...). Push fixes to main first so he can pull and test; tag + release only after he approves.
- When he does ask for a bump: `node scripts/bump-version.js patch` (or minor/major) updates all the files above in one go; the pre-commit hook does NOT bump anymore. Commit the bump together with the change it releases.
- Activate hooks once: `git config core.hooksPath .githooks` (or `pnpm install`, which runs the `prepare` script). The hook files must stay executable in git (mode 100755). In the agent VM, the `git` wrapper forces `core.hooksPath=/dev/null`, so hooks never run from plain `git` — commit with `git -c core.hooksPath=.githooks commit ...` (the trailing `-c` overrides the wrapper).
- `src/lib/releases/releases.ts` FALLBACK_DOWNLOADS is NOT bumped by the script: it always names the last RELEASED version, never the version being built — bump it only after a release is live (the web deploys on push, the release only exists after the tag's build).
- Only a tag (`vX.Y.Z`) creates a release. An ordinary push to main redeploys the web but does not release.

### Changelog — every release says what changed
- `CHANGELOG.md` is the source of truth: one `## [X.Y.Z] - YYYY-MM-DD` section per version, in Vietnamese, short bullets. When adding a feature or fixing a bug, add its bullet to that version's section in the same commit.
- The GitHub release body = that version's CHANGELOG section + the download instructions. The app's "Nhật ký thay đổi" dialog/page reads the release body via the GitHub API (`fetchChangelog()`), so a release with an empty body shows an empty changelog.
- `node scripts/changelog-section.mjs <version>` prints one version's section (used when editing a release body: `gh release edit vX.Y.Z --notes-file <section>`).
- The release workflow's `releaseBody` is static text; the changelog section has to be added to the body after the release is published (or by a workflow step — workflow files can only be edited through the GitHub web UI, see below).

### History — one clean commit per version
- Keep history to one clean squashed commit per version: orphan branch → `git add -A` → verify the staged diff vs origin/main → commit → force-push → reset local main → delete the branch. ALWAYS `git fetch` + compare origin/main before any force-push.
- NEVER include `.github/workflows/*` in a pushed diff: the token has no `workflow` scope, so such pushes are rejected (403). Workflow edits go through the GitHub web UI.
- Tag only after the build verdict (CI green). Move a tag only by deleting it (remote + local) and re-creating it.

### Attribution (GPL) — never remove
- This app derives from LibreGet (GPL-3.0): keep `LICENSE` + the LibreGet credit line in `src/components/web/WebDownloadHub.svelte`. The Flaticon 'Download' icon by Magnific needs its attribution line too. Never erase upstream attribution to claim exclusive authorship.

### Download formats follow the platform
- The download options shown for a link must match what that platform can actually produce: image / audio / video formats only for media links (YouTube, TikTok, Instagram...); document links (Scribd, Studocu, Google Docs...) offer only PDF / DOCX. Never show video/audio choices for a document link.
- YouTube downloads are split into three modes: current clip, playlist, mix (mixes can exceed 1000 clips — no 50-item cap on the mix mode).
