use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{ConnectInfo, Query, State},
    http::{header, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::AsyncReadExt,
    net::{lookup_host, TcpListener},
    process::Command,
    sync::Mutex,
    time::timeout,
};
use tokio_util::io::ReaderStream;
use tower_http::cors::CorsLayer;
use url::Url;

const MAX_DOWNLOAD_BYTES: u64 = 1_000_000_000;

#[derive(Clone, Default)]
struct AppState(Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>);

#[derive(Deserialize)]
struct UrlRequest {
    url: String,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(ApiError {
            error: message.into(),
        }),
    )
        .into_response()
}

async fn limited(state: &AppState, ip: IpAddr) -> Result<(), Response> {
    let mut requests = state.0.lock().await;
    let now = Instant::now();
    let recent = requests.entry(ip).or_default();
    recent.retain(|time| now.duration_since(*time) < Duration::from_secs(60));
    if recent.len() >= 20 {
        return Err(error(StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded."));
    }
    recent.push(now);
    Ok(())
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.octets() == [255, 255, 255, 255])
        }
        IpAddr::V6(ip) => {
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80)
        }
    }
}

async fn validate(raw: &str) -> Result<Url, Response> {
    let url = Url::parse(raw).map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid URL."))?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Only credential-free HTTPS URLs are accepted.",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "URL host is missing."))?;
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        return Err(error(
            StatusCode::FORBIDDEN,
            "Local network hosts are blocked.",
        ));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !public_ip(ip) {
            return Err(error(
                StatusCode::FORBIDDEN,
                "Private network addresses are blocked.",
            ));
        }
    } else {
        let addresses = lookup_host((host, url.port_or_known_default().unwrap_or(443)))
            .await
            .map_err(|_| error(StatusCode::BAD_REQUEST, "Host could not be resolved."))?;
        let mut found = false;
        for address in addresses {
            found = true;
            if !public_ip(address.ip()) {
                return Err(error(
                    StatusCode::FORBIDDEN,
                    "Host resolves to a private network address.",
                ));
            }
        }
        if !found {
            return Err(error(StatusCode::BAD_REQUEST, "Host has no addresses."));
        }
    }
    Ok(url)
}

async fn inspect(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<UrlRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    limited(&state, peer.ip()).await?;
    let url = validate(&body.url).await?;
    let output = timeout(
        Duration::from_secs(45),
        Command::new("yt-dlp")
            .args([
                "--dump-single-json",
                "--skip-download",
                "--no-playlist",
                "--no-warnings",
                "--no-call-home",
                "--",
                url.as_str(),
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| error(StatusCode::GATEWAY_TIMEOUT, "Inspection timed out."))?
    .map_err(|_| error(StatusCode::BAD_GATEWAY, "yt-dlp could not start."))?;
    if !output.status.success() {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "Could not inspect this media URL.",
        ));
    }
    let data: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "Invalid metadata response."))?;
    Ok(Json(
        serde_json::json!({"id":data.get("id"),"title":data.get("title"),"duration":data.get("duration"),"thumbnail":data.get("thumbnail"),"extractor":data.get("extractor_key")}),
    ))
}

async fn download(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(query): Query<UrlRequest>,
) -> Result<Response, Response> {
    limited(&state, peer.ip()).await?;
    let url = validate(&query.url).await?;
    let mut child = Command::new("yt-dlp")
        .args([
            "--no-playlist",
            "--no-warnings",
            "--no-call-home",
            "--output",
            "-",
            "--",
            url.as_str(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "yt-dlp could not start."))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| error(StatusCode::BAD_GATEWAY, "Download stream unavailable."))?;
    let bounded = stdout.take(MAX_DOWNLOAD_BYTES);
    let body = Body::from_stream(ReaderStream::new(bounded));
    let mut response = Response::new(body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"gumballz-media\""),
    );
    Ok(response)
}

#[tokio::main]
async fn main() {
    let origin =
        std::env::var("CORS_ORIGIN").expect("Set CORS_ORIGIN to the exact web app origin.");
    let origin = HeaderValue::from_str(&origin).expect("CORS_ORIGIN must be a valid origin.");
    let cors = CorsLayer::new()
        .allow_origin(origin)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE]);
    let app = Router::new()
        .route("/api/inspect", post(inspect))
        .route("/api/download", get(download))
        .with_state(AppState::default())
        .layer(cors);
    let listener = TcpListener::bind("0.0.0.0:8787")
        .await
        .expect("Cannot bind port 8787");
    println!("GumballZ Get worker listening on :8787");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("Worker failed");
}
