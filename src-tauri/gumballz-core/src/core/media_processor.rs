use crate::core::hls_downloader::HlsDownloadResult;
use tokio_util::sync::CancellationToken;

pub struct MediaProcessor;

impl MediaProcessor {
    #[allow(clippy::too_many_arguments)]
    pub async fn download_hls(
        m3u8_url: &str,
        output: &str,
        referer: &str,
        bytes_tx: Option<tokio::sync::mpsc::UnboundedSender<u64>>,
        cancel_token: CancellationToken,
        max_concurrent: u32,
        max_retries: u32,
        client: Option<reqwest::Client>,
    ) -> anyhow::Result<HlsDownloadResult> {
        let downloader = match client {
            Some(c) => crate::core::hls_downloader::HlsDownloader::with_client(c),
            None => crate::core::hls_downloader::HlsDownloader::new(),
        };
        downloader
            .download(
                m3u8_url,
                output,
                referer,
                bytes_tx,
                cancel_token,
                max_concurrent,
                max_retries,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn download_hls_with_quality(
        m3u8_url: &str,
        output: &str,
        referer: &str,
        bytes_tx: Option<tokio::sync::mpsc::UnboundedSender<u64>>,
        cancel_token: CancellationToken,
        max_concurrent: u32,
        max_retries: u32,
        client: Option<reqwest::Client>,
        max_height: Option<u32>,
    ) -> anyhow::Result<HlsDownloadResult> {
        let downloader = match client {
            Some(c) => crate::core::hls_downloader::HlsDownloader::with_client(c),
            None => crate::core::hls_downloader::HlsDownloader::new(),
        };
        downloader
            .download_with_quality(
                m3u8_url,
                output,
                referer,
                bytes_tx,
                cancel_token,
                max_concurrent,
                max_retries,
                max_height,
            )
            .await
    }

    pub async fn remux(input: &str, output: &str) -> anyhow::Result<()> {
        let status = crate::core::process::command("ffmpeg")
            .args(["-y", "-i", input, "-c", "copy", output])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("FFmpeg remux falhou com status {}", status);
        }

        Ok(())
    }

    pub async fn merge_audio_video(video: &str, audio: &str, output: &str) -> anyhow::Result<()> {
        let status = crate::core::process::command("ffmpeg")
            .args([
                "-y", "-i", video, "-i", audio, "-map", "0:v", "-map", "1:a", "-c", "copy", output,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("FFmpeg merge falhou com status {}", status);
        }

        Ok(())
    }

    /// Build an MP4 slideshow from still images + optional background music,
    /// like TikTok plays a photo post: each photo shows for an equal slice
    /// of the music (3s each when there is no music), scaled/padded to the
    /// given canvas and encoded with libx264 + AAC.
    pub async fn slideshow_from_images(
        images: &[std::path::PathBuf],
        music: Option<&std::path::Path>,
        width: u32,
        height: u32,
        output: &str,
    ) -> anyhow::Result<()> {
        if images.is_empty() {
            anyhow::bail!("No images for slideshow");
        }

        let per_image_secs = match music {
            Some(m) => match crate::core::ffmpeg::probe(m).await {
                Ok(info) => {
                    let d = info.duration_seconds / images.len() as f64;
                    if d > 0.0 && d.is_finite() {
                        d
                    } else {
                        3.0
                    }
                }
                Err(_) => 3.0,
            },
            None => 3.0,
        };

        // libx264 needs even dimensions.
        let w = (width.max(2)) & !1;
        let h = (height.max(2)) & !1;

        let list_path = std::env::temp_dir().join(format!(
            "gumballz-slideshow-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let mut list = String::new();
        for img in images {
            list.push_str(&format!(
                "file '{}'\nduration {:.3}\n",
                img.to_string_lossy().replace('\'', "'\\''"),
                per_image_secs
            ));
        }
        // The concat demuxer needs the last file repeated without a duration.
        list.push_str(&format!(
            "file '{}'\n",
            images
                .last()
                .unwrap()
                .to_string_lossy()
                .replace('\'', "'\\''")
        ));
        tokio::fs::write(&list_path, &list).await?;

        let vf = format!(
            "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black,format=yuv420p",
            w, h, w, h
        );
        let mut args: Vec<String> = vec![
            "-y".into(),
            "-f".into(),
            "concat".into(),
            "-safe".into(),
            "0".into(),
            "-i".into(),
            list_path.to_string_lossy().into_owned(),
        ];
        if let Some(m) = music {
            args.extend(["-i".into(), m.to_string_lossy().into_owned()]);
        }
        args.extend([
            "-vf".into(),
            vf,
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "veryfast".into(),
            "-crf".into(),
            "23".into(),
        ]);
        if music.is_some() {
            args.extend(["-c:a".into(), "aac".into(), "-shortest".into()]);
        }
        args.extend(["-movflags".into(), "+faststart".into(), output.into()]);

        let status = crate::core::process::command("ffmpeg")
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()
            .await?;
        let _ = tokio::fs::remove_file(&list_path).await;

        if !status.success() {
            anyhow::bail!("FFmpeg slideshow falhou com status {}", status);
        }

        Ok(())
    }

    pub async fn download_direct(
        url: &str,
        output: &str,
        headers: &[(&str, &str)],
    ) -> anyhow::Result<()> {
        let mut args = vec!["-y".to_string()];

        if !headers.is_empty() {
            let header_str: String = headers
                .iter()
                .map(|(k, v)| format!("{}: {}\r\n", k, v))
                .collect();
            args.extend(["-headers".to_string(), header_str]);
        }

        args.extend([
            "-i".to_string(),
            url.to_string(),
            "-c".to_string(),
            "copy".to_string(),
            output.to_string(),
        ]);

        let status = crate::core::process::command("ffmpeg")
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("FFmpeg download_direct falhou com status {}", status);
        }

        Ok(())
    }
}

pub fn check_ffmpeg() -> bool {
    crate::core::process::std_command("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn check_ytdlp() -> bool {
    crate::core::process::std_command("yt-dlp")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn check_dependencies() -> Vec<String> {
    let mut missing = Vec::new();
    if !check_ytdlp() {
        missing.push("yt-dlp".into());
    }
    if !check_ffmpeg() {
        missing.push("ffmpeg".into());
    }
    missing
}
