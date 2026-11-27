//! Thin layer over gallery-dl: JSON dump (`-j`) and download, plus
//! parsing of the messages it spits out.
//!
//! The `-j` returns an array of messages; each one is an array whose first
//! item is the type (2 = directory, 3 = file/URL, 6 = queue). What matters
//! for an index is message 3: `[3, "<file url>", { metadata }]`.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde_json::Value;

pub const KIND_DIRECTORY: u64 = 2;
pub const KIND_URL: u64 = 3;
pub const KIND_QUEUE: u64 = 6;

#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: u64,
    pub url: Option<String>,
    pub meta: Value,
}

impl Entry {
    pub fn is_url(&self) -> bool {
        self.kind == KIND_URL
    }

    /// Text field, accepting a dotted path (`author.username`).
    /// A number becomes text, so the CSV does not lose an id that came as an integer.
    pub fn text(&self, path: &str) -> String {
        match dig(&self.meta, path) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::Bool(b)) => b.to_string(),
            _ => String::new(),
        }
    }

    /// First path that exists and is not empty.
    pub fn first_text(&self, paths: &[&str]) -> String {
        for p in paths {
            let v = self.text(p);
            if !v.is_empty() {
                return v;
            }
        }
        String::new()
    }

    pub fn number(&self, path: &str) -> Option<i64> {
        match dig(&self.meta, path) {
            Some(Value::Number(n)) => n.as_i64(),
            Some(Value::String(s)) => s.parse::<i64>().ok(),
            _ => None,
        }
    }

    /// Tags: gallery-dl delivers a list of strings, a list of objects
    /// (`{"tag_name": …}`) or a single string, space-separated (Flickr).
    pub fn tags(&self, path: &str) -> Vec<String> {
        match dig(&self.meta, path) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|i| match i {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(_) => dig(i, "tag_name")
                        .or_else(|| dig(i, "name"))
                        .and_then(|v| v.as_str().map(|s| s.to_string())),
                    _ => None,
                })
                .filter(|s| !s.trim().is_empty())
                .collect(),
            Some(Value::String(s)) => s
                .split_whitespace()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Navigates a `Value` by dotted path.
pub fn dig<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = value;
    for part in path.split('.') {
        cur = cur.get(part)?;
    }
    if cur.is_null() {
        None
    } else {
        Some(cur)
    }
}

/// Reads the `gallery-dl -j` array. Tolerant: also accepts one message per
/// line (NDJSON), which is what comes out when the output is truncated by a pipe.
pub fn parse_dump(text: &str) -> Result<Vec<Entry>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(trimmed) {
        return Ok(items.iter().filter_map(entry_from).collect());
    }
    let mut out = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim().trim_end_matches(',');
        if line.is_empty() || line == "[" || line == "]" {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if let Some(e) = entry_from(&v) {
                out.push(e);
            }
        }
    }
    if out.is_empty() {
        return Err(anyhow!("the gallery-dl output is not a valid JSON dump"));
    }
    Ok(out)
}

fn entry_from(value: &Value) -> Option<Entry> {
    let items = value.as_array()?;
    let kind = items.first()?.as_u64()?;
    match items.len() {
        2 => Some(Entry {
            kind,
            url: None,
            meta: items[1].clone(),
        }),
        3.. => Some(Entry {
            kind,
            url: items[1].as_str().map(|s| s.to_string()),
            meta: items[2].clone(),
        }),
        _ => None,
    }
}

// ───────────────────────── cookies ─────────────────────────

/// Keeps in the file only the cookies of the requested domains (and subdomains).
/// The extension session may have come from a shared bucket; the
/// gallery-dl does not need to see another site's cookies.
pub fn filter_netscape(content: &str, domains: &[&str]) -> String {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for raw in content.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        let bare = line.trim();
        if bare.is_empty() {
            continue;
        }
        let probe = bare.strip_prefix("#HttpOnly_").unwrap_or(bare);
        if probe.starts_with('#') {
            continue;
        }
        let Some(first) = probe.split('\t').next() else {
            continue;
        };
        if probe.split('\t').count() < 7 {
            continue;
        }
        let host = first.trim_start_matches('.').to_lowercase();
        let ok = domains
            .iter()
            .any(|d| host == *d || host.ends_with(&format!(".{}", d)));
        if ok {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Joins several sessions (one per domain) into a single Netscape file, with one
/// single header.
pub fn merge_netscape(parts: &[String]) -> String {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for part in parts {
        for raw in part.lines() {
            let line = raw.trim_end_matches(['\r', '\n']);
            let probe = line.trim();
            if probe.is_empty() {
                continue;
            }
            let bare = probe.strip_prefix("#HttpOnly_").unwrap_or(probe);
            if bare.starts_with('#') {
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Writes the filtered session to a temporary file for `--cookies`.
/// Returns `None` when no cookie is left.
pub struct CookieFile {
    pub path: PathBuf,
}

impl Drop for CookieFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn write_cookies(netscape: &str, domains: &[&str]) -> Result<Option<CookieFile>> {
    let filtered = filter_netscape(netscape, domains);
    if filtered.lines().filter(|l| !l.starts_with('#')).count() == 0 {
        return Ok(None);
    }
    let dir = super::super::temp_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("gdl-cookies-{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&path, filtered)?;
    Ok(Some(CookieFile { path }))
}

// ───────────────────────── processo ─────────────────────────

fn range_arg(limit: Option<u64>) -> Option<String> {
    limit.filter(|n| *n > 0).map(|n| format!("1-{}", n))
}

async fn binary() -> Result<PathBuf> {
    crate::core::dependencies::ensure_gallerydl()
        .await
        .ok_or_else(|| anyhow!("gallery-dl is not installed and could not be downloaded"))
}

/// `gallery-dl -j <url>`: only lists, downloads nothing.
pub async fn dump(
    url: &str,
    cookies: Option<&Path>,
    limit: Option<u64>,
    extra: &[String],
    progress: &super::super::ProgressFn,
    id: &str,
) -> Result<Vec<Entry>> {
    use tokio::io::AsyncReadExt;

    let bin = binary().await?;
    let mut cmd = crate::core::process::command(&bin);
    cmd.arg("-j");
    if let Some(r) = range_arg(limit) {
        cmd.args(["--range", &r]);
    }
    if let Some(c) = cookies {
        cmd.arg("--cookies").arg(c);
    }
    for a in extra {
        cmd.arg(a);
    }
    cmd.arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow!("could not start gallery-dl: {}", e))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("gallery-dl did not open stdout"))?;
    let stderr = child.stderr.take();

    let err_task = tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut tail = String::new();
        if let Some(e) = stderr {
            let mut lines = BufReader::new(e).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    tail = line;
                }
            }
        }
        tail
    });

    super::super::report(progress, id, "started", 0, None, None);
    let mut text = String::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut last = std::time::Instant::now();
    loop {
        let n = stdout.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        text.push_str(&String::from_utf8_lossy(&buf[..n]));
        if last.elapsed() > std::time::Duration::from_millis(300) {
            super::super::report(progress, id, "progress", text.len() as u64, None, None);
            last = std::time::Instant::now();
        }
    }
    let status = child.wait().await?;
    let tail = err_task.await.unwrap_or_default();
    if !status.success() && text.trim().is_empty() {
        return Err(anyhow!(
            "gallery-dl could not read this page: {}",
            if tail.is_empty() {
                "no detail".to_string()
            } else {
                tail
            }
        ));
    }
    parse_dump(&text)
}

#[derive(Debug, Clone, Default)]
pub struct Downloaded {
    pub files: Vec<String>,
    pub log_tail: String,
}

/// `gallery-dl -d <dest> --write-metadata <url>`: downloads the media and writes the
/// `.json` of each file next to it, which is what matches file to post.
pub async fn download(
    url: &str,
    dest: &Path,
    cookies: Option<&Path>,
    limit: Option<u64>,
    extra: &[String],
    progress: &super::super::ProgressFn,
    id: &str,
) -> Result<Downloaded> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let bin = binary().await?;
    std::fs::create_dir_all(dest)?;
    let mut cmd = crate::core::process::command(&bin);
    cmd.arg("-d").arg(dest).arg("--write-metadata");
    if let Some(r) = range_arg(limit) {
        cmd.args(["--range", &r]);
    }
    if let Some(c) = cookies {
        cmd.arg("--cookies").arg(c);
    }
    for a in extra {
        cmd.arg(a);
    }
    cmd.arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow!("could not start gallery-dl: {}", e))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let p2 = progress.clone();
    let id2 = id.to_string();
    let out_task = tokio::spawn(async move {
        let mut files: Vec<String> = Vec::new();
        if let Some(o) = stdout {
            let mut lines = BufReader::new(o).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let l = line.trim().trim_start_matches("# ").to_string();
                if l.is_empty() {
                    continue;
                }
                files.push(l.clone());
                super::super::report(&p2, &id2, "progress", files.len() as u64, None, Some(l));
            }
        }
        files
    });
    let err_task = tokio::spawn(async move {
        let mut tail = String::new();
        if let Some(e) = stderr {
            let mut lines = BufReader::new(e).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    tail = line;
                }
            }
        }
        tail
    });
    let status = child.wait().await?;
    let files = out_task.await.unwrap_or_default();
    let log_tail = err_task.await.unwrap_or_default();
    if !status.success() && files.is_empty() {
        return Err(anyhow!("o gallery-dl falhou: {}", log_tail));
    }
    Ok(Downloaded { files, log_tail })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMP: &str = r#"[
      [2, {"category": "tumblr", "blog_name": "estudio"}],
      [3, "https://64.media.tumblr.com/abc/foto_1280.jpg",
        {"id": 700111222, "blog_name": "estudio", "type": "photo",
         "tags": ["arte", "aquarela"], "date": "2024-03-02 10:00:00",
         "post_url": "https://estudio.tumblr.com/post/700111222"}],
      [6, "https://outro.tumblr.com/post/1", {"category": "tumblr"}]
    ]"#;

    #[test]
    fn le_o_array_do_dump_json() {
        let entries = parse_dump(DUMP).expect("valid dump");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].kind, KIND_DIRECTORY);
        assert!(entries[0].url.is_none());
        assert!(entries[1].is_url());
        assert_eq!(
            entries[1].url.as_deref(),
            Some("https://64.media.tumblr.com/abc/foto_1280.jpg")
        );
        assert_eq!(entries[2].kind, KIND_QUEUE);
    }

    #[test]
    fn campos_aceitam_caminho_com_ponto_e_numero_vira_texto() {
        let entries = parse_dump(DUMP).expect("valid dump");
        let e = &entries[1];
        assert_eq!(e.text("id"), "700111222");
        assert_eq!(e.text("blog_name"), "estudio");
        assert_eq!(e.text("nao.existe"), "");
        assert_eq!(e.number("id"), Some(700111222));
        assert_eq!(e.first_text(&["vazio", "type"]), "photo");
    }

    #[test]
    fn tags_aceitam_lista_objeto_e_string() {
        let lista: Value = serde_json::json!({"tags": ["a", "b"]});
        let objeto: Value = serde_json::json!({"tags": [{"tag_name": "c"}, {"name": "d"}]});
        let texto: Value = serde_json::json!({"tags": "e f  g"});
        let mk = |m: Value| Entry {
            kind: KIND_URL,
            url: None,
            meta: m,
        };
        assert_eq!(mk(lista).tags("tags"), vec!["a", "b"]);
        assert_eq!(mk(objeto).tags("tags"), vec!["c", "d"]);
        assert_eq!(mk(texto).tags("tags"), vec!["e", "f", "g"]);
    }

    #[test]
    fn dump_vazio_nao_e_erro() {
        assert!(parse_dump("[]").expect("array vazio").is_empty());
        assert!(parse_dump("   ").expect("nada").is_empty());
        assert!(parse_dump("isso não é json").is_err());
    }

    #[test]
    fn ndjson_tambem_e_aceito() {
        let nd = "[3, \"https://a/1.jpg\", {\"id\": 1}]\n[3, \"https://a/2.jpg\", {\"id\": 2}]";
        let entries = parse_dump(nd).expect("ndjson");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].text("id"), "2");
    }

    #[test]
    fn so_o_cookie_do_dominio_certo_entra_no_arquivo() {
        let content = "# Netscape HTTP Cookie File\n\
            .tumblr.com\tTRUE\t/\tTRUE\t2000000000\tsid\tvaleu\n\
            www.tumblr.com\tFALSE\t/\tTRUE\t2000000000\tpfg\txyz\n\
            .facebook.com\tTRUE\t/\tTRUE\t2000000000\tc_user\t123\n\
            evil-tumblr.com.br\tTRUE\t/\tTRUE\t2000000000\tx\ty\n\
            linha quebrada sem tabs\n";
        let out = filter_netscape(content, &["tumblr.com"]);
        assert!(out.contains("sid\tvaleu"));
        assert!(out.contains("pfg\txyz"));
        assert!(
            !out.contains("c_user"),
            "cookie from another site cannot leak"
        );
        assert!(
            !out.contains("evil-tumblr.com.br"),
            "a similar suffix is not the same domain"
        );
        assert_eq!(out.lines().filter(|l| !l.starts_with('#')).count(), 2);
    }

    #[test]
    fn merge_junta_sessoes_com_um_cabecalho_so() {
        let a = "# Netscape HTTP Cookie File\n.deviantart.com\tTRUE\t/\tTRUE\t1\tauth\tA\n";
        let b = "# Netscape HTTP Cookie File\n.flickr.com\tTRUE\t/\tTRUE\t1\tcookie\tB\n";
        let out = merge_netscape(&[a.to_string(), b.to_string()]);
        assert_eq!(out.lines().filter(|l| l.starts_with('#')).count(), 1);
        assert_eq!(out.lines().filter(|l| !l.starts_with('#')).count(), 2);
        assert!(out.contains("deviantart") && out.contains("flickr"));
    }

    #[test]
    fn limite_vira_range_do_gallery_dl() {
        assert_eq!(range_arg(Some(120)).as_deref(), Some("1-120"));
        assert_eq!(range_arg(Some(0)), None);
        assert_eq!(range_arg(None), None);
    }
}
