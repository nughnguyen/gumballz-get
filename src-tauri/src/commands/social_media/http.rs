#![allow(clippy::all)]
#![allow(warnings)]

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::Duration;
use tauri::command;

#[derive(Debug, Deserialize)]
pub struct HttpRequestOptions {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    pub cookie_path: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HttpResponsePayload {
    pub status: u16,
    pub ok: bool,
    pub headers: HashMap<String, String>,
    pub body: String,
}

pub fn parse_netscape_cookies(content: &str, filter_domain: Option<&str>) -> Vec<(String, String)> {
    let mut cookies = Vec::new();
    for raw_line in content.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(stripped) = line.strip_prefix("#HttpOnly_") {
            line = stripped;
        } else if line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 7 {
            let domain = parts[0].to_lowercase();
            let name = parts[5].to_string();
            let value = parts[6].to_string();

            if let Some(filter) = filter_domain {
                let clean_filter = filter.trim_start_matches('.').to_lowercase();
                let clean_domain = domain.trim_start_matches('.').to_lowercase();
                if !clean_domain.ends_with(&clean_filter) && !clean_filter.ends_with(&clean_domain)
                {
                    continue;
                }
            }

            if !name.is_empty() {
                cookies.push((name, value));
            }
        }
    }
    cookies
}

#[command]
pub async fn social_media_http_fetch(options: HttpRequestOptions) -> Result<HttpResponsePayload, String> {
    let timeout = Duration::from_millis(options.timeout_ms.unwrap_or(15_000));
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let method_str = options
        .method
        .unwrap_or_else(|| "GET".to_string())
        .to_uppercase();
    let method = match method_str.as_str() {
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "HEAD" => reqwest::Method::HEAD,
        _ => reqwest::Method::GET,
    };

    let mut request_builder = client.request(method, &options.url);

    let mut header_map = HeaderMap::new();
    if let Some(custom_headers) = options.headers {
        for (k, v) in custom_headers {
            if let (Ok(hn), Ok(hv)) = (HeaderName::from_str(&k), HeaderValue::from_str(&v)) {
                header_map.insert(hn, hv);
            }
        }
    }

    if let Some(cookie_path) = options.cookie_path {
        if let Ok(cookie_content) = fs::read_to_string(&cookie_path) {
            let cookies = parse_netscape_cookies(&cookie_content, None);
            if !cookies.is_empty() {
                let formatted = cookies
                    .into_iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join("; ");
                if let Ok(hv) = HeaderValue::from_str(&formatted) {
                    header_map.insert(reqwest::header::COOKIE, hv);
                }
            }
        }
    }

    request_builder = request_builder.headers(header_map);

    if let Some(body_text) = options.body {
        request_builder = request_builder.body(body_text);
    }

    let response = request_builder
        .send()
        .await
        .map_err(|e| format!("Network request failed: {}", e))?;

    let status = response.status().as_u16();
    let ok = response.status().is_success();

    let mut response_headers = HashMap::new();
    for (name, value) in response.headers() {
        if let Ok(val_str) = value.to_str() {
            response_headers.insert(name.as_str().to_lowercase(), val_str.to_string());
        }
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    Ok(HttpResponsePayload {
        status,
        ok,
        headers: response_headers,
        body,
    })
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedAvatar {
    pub file_path: String,
    pub data_url: String,
}

#[command]
pub async fn social_media_cache_avatar(url: String) -> Result<CachedAvatar, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Avatar URL cannot be empty".to_string());
    }

    if trimmed.starts_with("data:") {
        return Ok(CachedAvatar {
            file_path: String::new(),
            data_url: trimmed.to_string(),
        });
    }

    if trimmed.starts_with("asset://") || trimmed.starts_with("http://asset.localhost") {
        return Ok(CachedAvatar {
            file_path: trimmed.to_string(),
            data_url: trimmed.to_string(),
        });
    }

    let social_media_dir = crate::commands::social_media::gumballz_get_social_media_dir()?;
    let avatars_dir = social_media_dir.join("avatars");
    fs::create_dir_all(&avatars_dir)
        .map_err(|e| format!("Failed to create avatars directory: {}", e))?;

    use base64::Engine;
    use sha2::{Digest, Sha256};

    let ext_to_mime = |extension: &str| -> &'static str {
        match extension {
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            _ => "image/jpeg",
        }
    };

    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    if let Ok(entries) = fs::read_dir(&avatars_dir) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with(&hash) {
                            let path = entry.path();
                            if let Ok(bytes) = fs::read(&path) {
                                if !bytes.is_empty() {
                                    let ext =
                                        path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
                                    let mime = ext_to_mime(ext);
                                    let b64 =
                                        base64::engine::general_purpose::STANDARD.encode(&bytes);
                                    let data_url = format!("data:{};base64,{}", mime, b64);
                                    return Ok(CachedAvatar {
                                        file_path: path.to_string_lossy().to_string(),
                                        data_url,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let client = crate::core::http_client::apply_global_proxy(reqwest::Client::builder())
        .timeout(Duration::from_millis(12_000))
        .build()
        .map_err(|e| format!("Failed to build HTTP client for avatar: {}", e))?;

    let user_agent = if trimmed.contains("fbsbx.com")
        || trimmed.contains("facebook.com")
        || trimmed.contains("instagram.com")
    {
        "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"
    } else {
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
    };

    let resp = client
        .get(trimmed)
        .header(reqwest::header::USER_AGENT, user_agent)
        .header(
            reqwest::header::ACCEPT,
            "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8",
        )
        .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| format!("Network request failed for avatar: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Avatar download failed with status {}",
            resp.status()
        ));
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    if content_type.starts_with("text/html") {
        return Err("Avatar request returned HTML instead of an image".to_string());
    }

    let ext = if content_type.contains("png") {
        "png"
    } else if content_type.contains("webp") {
        "webp"
    } else if content_type.contains("gif") {
        "gif"
    } else if content_type.contains("svg") {
        "svg"
    } else {
        "jpg"
    };

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read avatar bytes: {}", e))?;

    if bytes.is_empty() {
        return Err("Received empty avatar response".to_string());
    }

    let target_path = avatars_dir.join(format!("{}.{}", hash, ext));
    let temp_path = avatars_dir.join(format!("{}.tmp.{}", hash, std::process::id()));

    fs::write(&temp_path, &bytes)
        .map_err(|e| format!("Failed to write temporary avatar file: {}", e))?;
    fs::rename(&temp_path, &target_path)
        .map_err(|e| format!("Failed to persist avatar file: {}", e))?;

    let mime = ext_to_mime(ext);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let data_url = format!("data:{};base64,{}", mime, b64);

    Ok(CachedAvatar {
        file_path: target_path.to_string_lossy().to_string(),
        data_url,
    })
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResolvedProfileAvatar {
    pub data_url: String,
    pub name: Option<String>,
}

#[command]
pub async fn social_media_resolve_facebook_avatar(
    url: String,
) -> Result<Option<ResolvedProfileAvatar>, String> {
    let clean = url.trim().trim_end_matches('/');
    if clean.is_empty() {
        return Ok(None);
    }

    let target_url = if clean.ends_with("/avatar") {
        clean.to_string()
    } else {
        format!("{}/avatar", clean)
    };

    let app_dir = crate::commands::social_media::gumballz_get_social_media_dir().unwrap_or_else(|_| PathBuf::from("."));
    let gallery_bin = crate::commands::social_media::fetcher::resolve_gallery_dl(&app_dir);

    if let Some(bin) = gallery_bin {
        let mut cmd = Command::new(&bin);
        cmd.args(["-j", &target_url, "--range", "1"]);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }

        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&stdout_str) {
                    let mut author_name: Option<String> = None;
                    let mut direct_image_url: Option<String> = None;

                    if let Some(arr) = json_val.as_array() {
                        for item in arr {
                            if let Some(sub_arr) = item.as_array() {
                                if sub_arr.len() >= 2 && sub_arr[0] == 2 {
                                    if let Some(obj) = sub_arr[1].as_object() {
                                        if let Some(u) =
                                            obj.get("username").and_then(|v| v.as_str())
                                        {
                                            if !u.is_empty() {
                                                author_name = Some(u.to_string());
                                            }
                                        }
                                    }
                                } else if sub_arr.len() >= 3 && sub_arr[0] == 3 {
                                    if let Some(img_url) = sub_arr[1].as_str() {
                                        if img_url.starts_with("http") {
                                            direct_image_url = Some(img_url.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(img_url) = direct_image_url {
                        if let Ok(cached) = social_media_cache_avatar(img_url).await {
                            return Ok(Some(ResolvedProfileAvatar {
                                data_url: cached.data_url,
                                name: author_name,
                            }));
                        }
                    }
                }
            }
        }
    }

    // Fallback: Check Graph API for public Pages
    let clean_user = clean.rsplit('/').next().unwrap_or(clean);
    let graph_url = format!(
        "https://graph.facebook.com/{}/picture?type=large",
        clean_user
    );
    if let Ok(cached) = social_media_cache_avatar(graph_url).await {
        return Ok(Some(ResolvedProfileAvatar {
            data_url: cached.data_url,
            name: None,
        }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_netscape_cookies_rust() {
        let raw = ".instagram.com\tTRUE\t/\tTRUE\t1799999999\tsessionid\t12345%3Aabc\n#HttpOnly_.instagram.com\tTRUE\t/\tTRUE\t1799999999\tds_user_id\t98765\n";
        let cookies = parse_netscape_cookies(raw, Some("instagram.com"));
        assert_eq!(cookies.len(), 2);
        assert_eq!(
            cookies[0],
            ("sessionid".to_string(), "12345%3Aabc".to_string())
        );
        assert_eq!(cookies[1], ("ds_user_id".to_string(), "98765".to_string()));
    }
}
