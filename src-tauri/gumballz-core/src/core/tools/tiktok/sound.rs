//! `tt-sound`: download just the audio — the "sound" — **with attribution**.
//!
//! Extracting audio from a video is the easy part. What makes this tool worth it is the
//! credit: the sound's name, who created it and the sound page's link, written into the
//! file's tags and a text sidecar next to it. Without that it is just any audio
//! extractor.
//!
//! Where each piece comes from:
//! - the sound's **name** and **author** come from yt-dlp's `-J` (`track`, `artist`);
//! - the sound's **id** — and therefore the `tiktok.com/music/<slug>-<id>` link — only
//!   exists in the video page's HTML, in the `music` object. That is why the tool does
//!   a GET on that page; if it fails, the credit comes out without the link instead of the
//!   whole tool failing.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::{expand_inputs, short_reason, sound_url, Pacer, TempCookies};
use crate::core::tools::{report, sanitize_name, ProgressFn};

const ID: &str = "tt-sound";

fn def_true() -> bool {
    true
}
fn def_format() -> String {
    "mp3".to_string()
}
fn def_delay() -> u64 {
    1200
}

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    /// Pasted list: one URL per line.
    #[serde(default)]
    pub urls: String,
    /// `.txt` file with one URL per line.
    #[serde(default)]
    pub list_file: Option<String>,
    pub dest: String,
    /// "mp3" | "m4a" | "best".
    #[serde(default = "def_format")]
    pub format: String,
    /// Write the credit into the file's tags.
    #[serde(default = "def_true")]
    pub tag_file: bool,
    /// Write the credit into a `.txt` next to it.
    #[serde(default = "def_true")]
    pub write_sidecar: bool,
    /// Skip what is already in the destination.
    #[serde(default = "def_true")]
    pub skip_existing: bool,
    #[serde(default = "def_delay")]
    pub delay_ms: u64,
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub account_slug: Option<String>,
    #[serde(skip)]
    pub session_netscape: Option<String>,
}

/// The credit of a sound. It is the heart of the tool.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Attribution {
    /// Sound name ("original sound", "Beat do X"…).
    pub sound: String,
    /// Who signs the sound (not always who posted the video).
    pub sound_author: String,
    /// `tiktok.com/music/<slug>-<id>`, when the id was found.
    pub sound_url: String,
    pub sound_id: String,
    /// Whether the sound is original to the video or taken from another.
    pub original: bool,
    /// Who posted the video.
    pub video_author: String,
    pub video_title: String,
    pub video_url: String,
    pub video_id: String,
    pub created: String,
    pub duration: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SoundItem {
    pub url: String,
    /// "ok" | "skipped" | "failed"
    pub status: String,
    pub reason: String,
    pub file: String,
    pub sidecar: String,
    pub tagged: bool,
    pub attribution: Attribution,
}

#[derive(Debug, Clone, Serialize)]
pub struct SoundResult {
    pub items: Vec<SoundItem>,
    pub ok: usize,
    pub failed: usize,
    pub skipped: usize,
    pub dest: String,
    pub used_session: bool,
}

// ───────────────────────── crédito ─────────────────────────

fn s(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Builds the credit with what yt-dlp returned and, when it exists, with the
/// reading of the page's `music` object (which is the one that has the sound id).
pub fn attribution_from(meta: &serde_json::Value, music: Option<&super::Music>) -> Attribution {
    let track = s(meta, "track");
    let artist = meta
        .get("artists")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .and_then(|x| x.as_str())
        .map(|x| x.to_string())
        .unwrap_or_else(|| s(meta, "artist"));
    let ts = meta.get("timestamp").and_then(|x| x.as_i64()).unwrap_or(0);
    let sound = music
        .map(|m| m.title.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or(track);
    let sound_author = music
        .map(|m| m.author.clone())
        .filter(|a| !a.is_empty())
        .unwrap_or(artist);
    let sound_id = music.map(|m| m.id.clone()).unwrap_or_default();
    Attribution {
        sound_url: sound_url(&sound, &sound_id).unwrap_or_default(),
        sound,
        sound_author,
        sound_id,
        original: music.map(|m| m.original).unwrap_or(false),
        video_author: s(meta, "uploader"),
        video_title: s(meta, "title"),
        video_url: s(meta, "webpage_url"),
        video_id: s(meta, "id"),
        created: if ts > 0 {
            super::fmt_utc(ts)
        } else {
            String::new()
        },
        duration: meta.get("duration").and_then(|x| x.as_u64()).unwrap_or(0),
    }
}

/// The text sidecar: the credit written for a person to read and copy.
pub fn sidecar_text(a: &Attribution) -> String {
    let mut out = String::new();
    let linha = |out: &mut String, rotulo: &str, valor: &str| {
        if !valor.trim().is_empty() {
            out.push_str(rotulo);
            out.push_str(": ");
            out.push_str(valor.trim());
            out.push('\n');
        }
    };
    out.push_str("Sound credit\n");
    out.push_str("==============\n");
    linha(&mut out, "Sound", &a.sound);
    linha(&mut out, "Sound author", &a.sound_author);
    linha(&mut out, "Sound link", &a.sound_url);
    linha(
        &mut out,
        "Type",
        if a.original {
            "original sound of the video"
        } else {
            "sound taken from another video"
        },
    );
    out.push('\n');
    linha(&mut out, "Video", &a.video_title);
    linha(&mut out, "Posted by", &a.video_author);
    linha(&mut out, "Video link", &a.video_url);
    linha(&mut out, "Date", &a.created);
    if a.duration > 0 {
        out.push_str(&format!("Duration: {}s\n", a.duration));
    }
    out.push_str("\nDownloaded with GumballZ Get. Give credit when using.\n");
    out
}

/// The same thing in one line, to fit in the tag's comment field.
pub fn comment_line(a: &Attribution) -> String {
    let mut partes: Vec<String> = Vec::new();
    if !a.sound.is_empty() {
        partes.push(format!("Sound: {}", a.sound));
    }
    if !a.sound_author.is_empty() {
        partes.push(format!("by {}", a.sound_author));
    }
    if !a.sound_url.is_empty() {
        partes.push(a.sound_url.clone());
    }
    if !a.video_url.is_empty() {
        partes.push(format!("video: {}", a.video_url));
    }
    partes.join(" · ")
}

/// Sound file name: `<author>-<id>-<sound>`.
pub fn sound_base(a: &Attribution) -> String {
    let autor = if a.video_author.is_empty() {
        "tiktok".to_string()
    } else {
        a.video_author.clone()
    };
    let som = if a.sound.is_empty() {
        "sound".to_string()
    } else {
        a.sound.clone()
    };
    let som: String = som.chars().take(60).collect();
    sanitize_name(&format!("{}-{}-{}", autor, a.video_id, som.trim()))
}

/// Writes the credit into the audio file's tags.
fn tag_audio(path: &Path, a: &Attribution) -> Result<()> {
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::prelude::{Accessor, ItemKey};
    use lofty::tag::Tag;

    let mut tagged = lofty::read_from_path(path).map_err(|e| anyhow!(e.to_string()))?;
    let tag_type = tagged.primary_tag_type();
    if tagged.primary_tag_mut().is_none() {
        tagged.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged
        .primary_tag_mut()
        .ok_or_else(|| anyhow!("the format does not accept tag {:?}", tag_type))?;

    if !a.sound.is_empty() {
        tag.set_title(a.sound.clone());
    }
    if !a.sound_author.is_empty() {
        tag.set_artist(a.sound_author.clone());
    }
    if !a.video_author.is_empty() {
        let _ = tag.insert_text(ItemKey::AlbumArtist, a.video_author.clone());
    }
    tag.set_album("TikTok".to_string());
    tag.set_genre("TikTok".to_string());
    let comentario = comment_line(a);
    if !comentario.is_empty() {
        tag.set_comment(comentario);
    }
    let fonte = if a.sound_url.is_empty() {
        a.video_url.clone()
    } else {
        a.sound_url.clone()
    };
    if !fonte.is_empty() {
        let _ = tag.insert_text(ItemKey::AudioFileUrl, fonte);
    }
    tagged
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(())
}

// ───────────────────────── execução ─────────────────────────

/// Fetches the `music` object on the video page. Best effort: any
/// stumble returns `None` and the credit continues with what yt-dlp gave.
async fn fetch_music(client: &reqwest::Client, url: &str, pacer: &Pacer) -> Option<super::Music> {
    pacer.wait().await;
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let html = resp.text().await.ok()?;
    super::parse_music(&html)
}

pub async fn run(opts: &Options, progress: ProgressFn) -> Result<SoundResult> {
    if opts.dest.trim().is_empty() {
        return Err(anyhow!("choose the destination folder"));
    }
    let dest = PathBuf::from(&opts.dest);
    std::fs::create_dir_all(&dest)?;

    let session = TempCookies::new(opts.session_netscape.as_deref());
    let used_session = session.is_some();
    let cookies: Option<PathBuf> = opts
        .cookies
        .as_deref()
        .filter(|c| !c.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| session.path().map(|p| p.to_path_buf()));

    let list_text = match opts.list_file.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(path) => std::fs::read_to_string(path)
            .map_err(|e| anyhow!("could not read the list {}: {}", path, e))?,
        None => String::new(),
    };
    let mut queue = expand_inputs(&opts.urls);
    for url in expand_inputs(&list_text) {
        if !queue.contains(&url) {
            queue.push(url);
        }
    }
    if queue.is_empty() {
        return Err(anyhow!("no TikTok video link in the input"));
    }

    let audio_format = match opts.format.as_str() {
        "m4a" => "m4a",
        "best" => "best",
        _ => "mp3",
    };
    let ffmpeg = crate::core::dependencies::find_tool("ffmpeg").await;
    let (client, _) = super::cookie_client(opts.session_netscape.as_deref())?;
    let pacer = Pacer::new(opts.delay_ms);
    let total = queue.len() as u64;
    let mut items: Vec<SoundItem> = Vec::with_capacity(queue.len());

    for (i, url) in queue.iter().enumerate() {
        report(
            &progress,
            ID,
            "progress",
            i as u64,
            Some(total),
            Some(url.clone()),
        );
        let falha = |motivo: String| SoundItem {
            url: url.clone(),
            status: "failed".to_string(),
            reason: motivo,
            file: String::new(),
            sidecar: String::new(),
            tagged: false,
            attribution: Attribution::default(),
        };

        // 1. metadados (nome do som, autor, data)
        pacer.wait().await;
        let mut meta_args = vec![
            "--ignore-config".to_string(),
            "--no-warnings".to_string(),
            "--no-playlist".to_string(),
            "--skip-download".to_string(),
            "-J".to_string(),
        ];
        if let Some(c) = cookies.as_deref() {
            meta_args.push("--cookies".to_string());
            meta_args.push(c.to_string_lossy().to_string());
        }
        meta_args.push(url.clone());
        let meta = match super::ytdlp_json(&meta_args).await {
            Ok(v) => v,
            Err(e) => {
                items.push(falha(e.to_string()));
                continue;
            }
        };

        // 2. the sound id, which only the page has
        let pagina = match meta.get("webpage_url").and_then(|x| x.as_str()) {
            Some(u) if u.starts_with("http") => u.to_string(),
            _ => url.clone(),
        };
        let music = fetch_music(&client, &pagina, &pacer).await;
        let attribution = attribution_from(&meta, music.as_ref());
        let base = sound_base(&attribution);

        if opts.skip_existing && super::has_stem(&dest, &base) {
            items.push(SoundItem {
                url: url.clone(),
                status: "skipped".to_string(),
                reason: "already in the folder".to_string(),
                file: String::new(),
                sidecar: String::new(),
                tagged: false,
                attribution,
            });
            continue;
        }

        // 3. the audio
        pacer.wait().await;
        let args = super::ytdlp_args(
            url,
            &dest,
            &base,
            super::Mode::Audio {
                format: audio_format,
            },
            false,
            ffmpeg.as_deref(),
            cookies.as_deref(),
        );
        let (files, tail) = match super::run_ytdlp(&args, ID, &progress).await {
            Ok(v) => v,
            Err(e) => {
                items.push(falha(e.to_string()));
                continue;
            }
        };
        let Some(file) = files.first().cloned() else {
            items.push(falha(short_reason(&tail)));
            continue;
        };

        // 4. the credit
        let mut tagged = false;
        let mut aviso = String::new();
        if opts.tag_file {
            match tag_audio(Path::new(&file), &attribution) {
                Ok(()) => tagged = true,
                Err(e) => aviso = format!("the audio came out, but the tags did not: {}", e),
            }
        }
        let mut sidecar = String::new();
        if opts.write_sidecar {
            let path = dest.join(format!("{}.credito.txt", base));
            match std::fs::write(&path, sidecar_text(&attribution)) {
                Ok(()) => sidecar = path.to_string_lossy().to_string(),
                Err(e) => aviso = format!("the credit sidecar was not written: {}", e),
            }
        }
        items.push(SoundItem {
            url: url.clone(),
            status: "ok".to_string(),
            reason: aviso,
            file,
            sidecar,
            tagged,
            attribution,
        });
    }

    let ok = items.iter().filter(|i| i.status == "ok").count();
    let failed = items.iter().filter(|i| i.status == "failed").count();
    let skipped = items.iter().filter(|i| i.status == "skipped").count();
    report(&progress, ID, "done", total, Some(total), None);
    Ok(SoundResult {
        items,
        ok,
        failed,
        skipped,
        dest: dest.to_string_lossy().to_string(),
        used_session,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn meta() -> serde_json::Value {
        json!({
            "id": "7683195368279985438",
            "title": "hear how @Palina spins",
            "uploader": "tiktok",
            "track": "original sound",
            "artists": ["TikTok"],
            "timestamp": 1788883336,
            "duration": 68,
            "webpage_url": "https://www.tiktok.com/@tiktok/video/7683195368279985438"
        })
    }

    fn music() -> super::super::Music {
        super::super::Music {
            id: "7683266637168610079".to_string(),
            title: "original sound".to_string(),
            author: "TikTok".to_string(),
            original: true,
            duration: 68,
            play_url: "https://v58.tiktokcdn.com/x".to_string(),
        }
    }

    #[test]
    fn credito_completo_quando_a_pagina_deu_o_id_do_som() {
        let a = attribution_from(&meta(), Some(&music()));
        assert_eq!(a.sound, "original sound");
        assert_eq!(a.sound_author, "TikTok");
        assert_eq!(a.sound_id, "7683266637168610079");
        assert_eq!(
            a.sound_url,
            "https://www.tiktok.com/music/original-sound-7683266637168610079"
        );
        assert!(a.original);
        assert_eq!(a.video_author, "tiktok");
        assert_eq!(a.created, "2026-09-08 16:02 UTC");
        assert_eq!(a.duration, 68);
    }

    #[test]
    fn sem_a_pagina_o_credito_sai_sem_link_mas_com_nome_e_autor() {
        let a = attribution_from(&meta(), None);
        assert_eq!(a.sound, "original sound");
        assert_eq!(a.sound_author, "TikTok");
        assert_eq!(a.sound_url, "");
        assert_eq!(a.sound_id, "");
        assert!(!a.original);
    }

    #[test]
    fn artista_cai_no_campo_singular_quando_nao_ha_lista() {
        let m = json!({"id":"1","track":"beat","artist":"Fulano"});
        let a = attribution_from(&m, None);
        assert_eq!(a.sound_author, "Fulano");
    }

    #[test]
    fn sidecar_traz_som_autor_e_link() {
        let texto = sidecar_text(&attribution_from(&meta(), Some(&music())));
        assert!(texto.starts_with("Sound credit"));
        assert!(texto.contains("Sound: original sound"));
        assert!(texto.contains("Sound author: TikTok"));
        assert!(texto.contains(
            "Sound link: https://www.tiktok.com/music/original-sound-7683266637168610079"
        ));
        assert!(texto.contains("Type: original sound of the video"));
        assert!(texto
            .contains("Video link: https://www.tiktok.com/@tiktok/video/7683195368279985438"));
        assert!(texto.contains("Duration: 68s"));
    }

    #[test]
    fn sidecar_pula_o_que_esta_vazio() {
        let texto = sidecar_text(&Attribution {
            sound: "x".to_string(),
            ..Default::default()
        });
        assert!(texto.contains("Sound: x"));
        assert!(!texto.contains("Sound link"));
        assert!(!texto.contains("Sound author"));
        assert!(!texto.contains("Duration"));
    }

    #[test]
    fn comentario_de_tag_cabe_em_uma_linha() {
        let c = comment_line(&attribution_from(&meta(), Some(&music())));
        assert_eq!(
            c,
            "Sound: original sound · by TikTok · \
             https://www.tiktok.com/music/original-sound-7683266637168610079 · \
             video: https://www.tiktok.com/@tiktok/video/7683195368279985438"
        );
        assert!(!c.contains('\n'));
        assert_eq!(comment_line(&Attribution::default()), "");
    }

    #[test]
    fn nome_do_arquivo_junta_autor_id_e_som() {
        let a = attribution_from(&meta(), Some(&music()));
        assert_eq!(sound_base(&a), "tiktok-7683195368279985438-original sound");
        let vazio = sound_base(&Attribution::default());
        assert_eq!(vazio, "tiktok--sound");
        let longo = sound_base(&Attribution {
            video_author: "a".to_string(),
            video_id: "1".to_string(),
            sound: "s".repeat(200),
            ..Default::default()
        });
        assert!(
            longo.len() <= 70,
            "the sound name is cut: {}",
            longo.len()
        );
    }

    #[tokio::test]
    #[ignore = "network: downloads the sound of a public TikTok video and checks the attribution"]
    async fn ao_vivo_baixa_o_som_com_credito() {
        let dir = crate::core::tools::temp_dir().join("tt-live-sound");
        let _ = std::fs::create_dir_all(&dir);
        let opts = Options {
            urls: "https://www.tiktok.com/@tiktok/video/7683195368279985438".to_string(),
            list_file: None,
            dest: dir.to_string_lossy().to_string(),
            format: "mp3".to_string(),
            tag_file: true,
            write_sidecar: true,
            skip_existing: false,
            delay_ms: 1200,
            cookies: None,
            account_slug: None,
            session_netscape: None,
        };
        let r = run(&opts, crate::core::tools::noop_progress())
            .await
            .expect("the live download failed");
        assert_eq!(r.failed, 0, "{:?}", r.items);
        let item = r.items.first().cloned().unwrap_or_else(|| SoundItem {
            url: String::new(),
            status: "failed".to_string(),
            reason: "no item".to_string(),
            file: String::new(),
            sidecar: String::new(),
            tagged: false,
            attribution: Attribution::default(),
        });
        assert!(!item.attribution.sound.is_empty());
        assert!(
            !item.attribution.sound_url.is_empty(),
            "the sound link is the point of the tool"
        );
        assert!(item.tagged);
        assert!(Path::new(&item.sidecar).exists());
    }
}
