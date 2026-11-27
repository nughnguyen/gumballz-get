//! LinkedIn category: everything here runs on top of the official export the user
//! downloads in Settings -> Data privacy -> Get a copy of your data. It is their own data,
//! read from disk, with no network request at all and without touching the site
//! — that is why the category exists.

pub mod checklist;
pub mod connections;
pub mod csv;
pub mod messages;
pub mod overview;
pub mod source;

use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

/// Name/count pair, used in the rankings (companies, roles, contacts).
#[derive(Debug, Clone, Serialize)]
pub struct Count {
    pub name: String,
    pub count: usize,
}

/// Count in a period (`2021-08` or `2021`).
#[derive(Debug, Clone, Serialize)]
pub struct Bucket {
    pub period: String,
    pub count: usize,
}

/// Descending ranking, ties resolved by name so the output is stable.
pub fn top(map: &HashMap<String, usize>, limit: usize) -> Vec<Count> {
    let mut v: Vec<Count> = map
        .iter()
        .map(|(name, count)| Count {
            name: name.clone(),
            count: *count,
        })
        .collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    v.truncate(limit);
    v
}

/// Time series in chronological order.
pub fn series(map: &HashMap<String, usize>) -> Vec<Bucket> {
    let mut v: Vec<Bucket> = map
        .iter()
        .map(|(period, count)| Bucket {
            period: period.clone(),
            count: *count,
        })
        .collect();
    v.sort_by(|a, b| a.period.cmp(&b.period));
    v
}

pub fn bump(map: &mut HashMap<String, usize>, key: &str) {
    if key.is_empty() {
        return;
    }
    *map.entry(key.to_string()).or_insert(0) += 1;
}

/// Output folder for the exported files. Without `out_dir` it falls back to
/// `<app_data>/tools/linkedin`.
pub fn out_dir(opt: Option<&str>) -> std::path::PathBuf {
    let dir = match opt {
        Some(d) if !d.trim().is_empty() => std::path::PathBuf::from(d),
        _ => super::tools_dir()
            .unwrap_or_else(super::temp_dir)
            .join("linkedin"),
    };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Writes an output file and returns the path as a string.
pub fn write_out(dir: &std::path::Path, name: &str, body: &str) -> Result<String> {
    let path = dir.join(super::sanitize_name(name));
    std::fs::write(&path, body)?;
    Ok(path.to_string_lossy().to_string())
}

/// Escapes text for HTML (the exports become a local page).
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// JSON embedded inside `<script>`: `<` becomes `<` so it does not close the tag.
pub fn json_for_script(v: &serde_json::Value) -> String {
    serde_json::to_string(v)
        .unwrap_or_else(|_| "null".to_string())
        .replace('<', "\\u003c")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking_e_estavel_no_empate() {
        let mut m = HashMap::new();
        bump(&mut m, "Beta");
        bump(&mut m, "Acme");
        bump(&mut m, "Acme");
        bump(&mut m, "Ceta");
        bump(&mut m, "");
        let t = top(&m, 3);
        assert_eq!(t[0].name, "Acme");
        assert_eq!(t[0].count, 2);
        assert_eq!(t[1].name, "Beta");
        assert_eq!(t[2].name, "Ceta");
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn serie_sai_em_ordem() {
        let mut m = HashMap::new();
        bump(&mut m, "2022-01");
        bump(&mut m, "2021-08");
        let s = series(&m);
        assert_eq!(s[0].period, "2021-08");
        assert_eq!(s[1].period, "2022-01");
    }

    #[test]
    fn escapa_html_e_script() {
        assert_eq!(
            html_escape("<b>&\"x\"</b>"),
            "&lt;b&gt;&amp;&quot;x&quot;&lt;/b&gt;"
        );
        let v = serde_json::json!({ "a": "</script>" });
        assert!(!json_for_script(&v).contains("</script>"));
    }
}
