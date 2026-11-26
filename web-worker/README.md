# GumballZ Get Web Worker

Optional Axum service for browser quick-grab. Requires `yt-dlp` and `ffmpeg` on `PATH`.

```powershell
$env:CORS_ORIGIN = "https://downloads.example.com"
cargo run --manifest-path web-worker/Cargo.toml --release
```

The worker listens on port `8787`. Set `VITE_WORKER_API_URL` to its HTTPS origin when building the web app. It exposes `POST /api/inspect` with `{"url":"https://…"}` and `GET /api/download?url=…`. Put it behind HTTPS and a reverse proxy with request and connection limits. The worker requires HTTPS input, rejects local/private DNS results, uses discrete process arguments, limits each peer to 20 requests per minute, and caps streamed downloads at 1 GB. Use an outbound firewall as an additional DNS-rebinding boundary.
