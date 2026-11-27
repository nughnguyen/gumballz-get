//! Compress an image until it fits a size, without guessing quality.
//!
//! `img-resize` changes dimensions and takes a fixed quality; here the target is the
//! **file size** — "this photo has to fit in 500 KB". The quality comes from
//! an in-memory binary search, and if it does not fit even at the minimum, the image
//! shrinks a little and the search restarts. No external process.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, DynamicImage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct CompressOptions {
    pub inputs: Vec<String>,
    /// Target in KB per file; 0 = use the fixed quality.
    #[serde(default)]
    pub target_kb: u64,
    /// Fixed quality (1-100), used when there is no size target.
    #[serde(default = "default_quality")]
    pub quality: u8,
    /// Longest side allowed; 0 = keep the dimensions.
    #[serde(default)]
    pub max_side: u32,
    /// Flattens transparency onto this colour when turning JPEG.
    #[serde(default = "default_bg")]
    pub background: String,
    /// Only reports what would happen.
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub output_dir: String,
    #[serde(default)]
    pub suffix: String,
}

fn default_quality() -> u8 {
    82
}
fn default_bg() -> String {
    "#FFFFFF".into()
}

#[derive(Debug, Clone, Serialize)]
pub struct CompressItem {
    pub input: String,
    pub output: Option<String>,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub quality: u8,
    pub width: u32,
    pub height: u32,
    pub scaled: bool,
    pub hit_target: bool,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompressResult {
    pub items: Vec<CompressItem>,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

pub fn encode_jpeg(img: &DynamicImage, quality: u8) -> anyhow::Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut enc = JpegEncoder::new_with_quality(&mut buf, quality.clamp(1, 100));
    enc.encode_image(&img.to_rgb8())
        .map_err(|e| anyhow!("could not encode the JPEG: {}", e))?;
    Ok(buf.into_inner())
}

/// Binary search for the quality that fits the target. Returns `(bytes, quality)`
/// with the **highest** quality that still fits; if not even the minimum fits, returns
/// the minimum's so the caller can decide whether to shrink the image.
pub fn fit_quality(
    img: &DynamicImage,
    target_bytes: u64,
    min_quality: u8,
    max_quality: u8,
) -> anyhow::Result<(Vec<u8>, u8, bool)> {
    let (mut lo, mut hi) = (min_quality.max(1), max_quality.min(100));
    let mut best: Option<(Vec<u8>, u8)> = None;
    // 7 steps cover the 1-100 range with room to spare.
    for _ in 0..7 {
        if lo > hi {
            break;
        }
        let mid = lo + (hi - lo) / 2;
        let bytes = encode_jpeg(img, mid)?;
        if bytes.len() as u64 <= target_bytes {
            best = Some((bytes, mid));
            lo = mid + 1;
        } else {
            if mid == 0 {
                break;
            }
            hi = mid - 1;
        }
    }
    match best {
        Some((bytes, q)) => Ok((bytes, q, true)),
        None => {
            let bytes = encode_jpeg(img, min_quality.max(1))?;
            let fits = bytes.len() as u64 <= target_bytes;
            Ok((bytes, min_quality.max(1), fits))
        }
    }
}

fn resize_to_side(img: &DynamicImage, max_side: u32) -> DynamicImage {
    if max_side == 0 || (img.width().max(img.height()) <= max_side) {
        return img.clone();
    }
    let scale = max_side as f32 / img.width().max(img.height()) as f32;
    img.resize(
        ((img.width() as f32 * scale).round() as u32).max(1),
        ((img.height() as f32 * scale).round() as u32).max(1),
        FilterType::Lanczos3,
    )
}

fn flatten(img: &DynamicImage, hex: &str) -> DynamicImage {
    let bg = super::icon_pack::parse_hex(hex);
    let mut canvas = image::RgbaImage::from_pixel(img.width(), img.height(), bg);
    image::imageops::overlay(&mut canvas, &img.to_rgba8(), 0, 0);
    DynamicImage::ImageRgba8(canvas)
}

fn compress_one(opts: &CompressOptions, input: &str) -> anyhow::Result<CompressItem> {
    let inp = Path::new(input);
    let bytes_before = std::fs::metadata(inp).map(|m| m.len()).unwrap_or(0);
    let mut img = image::open(inp).map_err(|e| anyhow!("could not open the image: {}", e))?;
    if img.color().has_alpha() {
        img = flatten(&img, &opts.background);
    }
    img = resize_to_side(&img, opts.max_side);
    let mut scaled = opts.max_side > 0 && img.width().max(img.height()) == opts.max_side;

    let (data, quality, hit) = if opts.target_kb > 0 {
        let target = opts.target_kb * 1024;
        let (mut data, mut quality, mut fits) = fit_quality(&img, target, 40, 95)?;
        // It did not fit even in the best case: give up quality, then shrink.
        // A noisy photo needs many rounds; stopping at three left the
        // user with a file above the target and no explanation.
        let mut tries = 0;
        while !fits && tries < 8 {
            let floor = if tries < 2 { 30 } else { 20 };
            let side = (img.width().max(img.height()) as f32 * 0.8).round() as u32;
            if side < 200 {
                break;
            }
            img = resize_to_side(&img, side);
            scaled = true;
            let r = fit_quality(&img, target, floor, 95)?;
            data = r.0;
            quality = r.1;
            fits = r.2;
            tries += 1;
        }
        (data, quality, fits)
    } else {
        let data = encode_jpeg(&img, opts.quality)?;
        (data, opts.quality, true)
    };

    let out_dir = if opts.output_dir.trim().is_empty() {
        inp.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    } else {
        PathBuf::from(opts.output_dir.trim())
    };
    let stem = inp
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".into());
    let suffix = if opts.suffix.is_empty() {
        "-compressed"
    } else {
        opts.suffix.as_str()
    };
    let output = out_dir.join(format!("{}{}.jpg", stem, suffix));
    if !opts.dry_run {
        std::fs::create_dir_all(&out_dir)?;
        std::fs::write(&output, &data)?;
    }

    Ok(CompressItem {
        input: input.to_string(),
        output: (!opts.dry_run).then(|| output.to_string_lossy().to_string()),
        bytes_before,
        bytes_after: data.len() as u64,
        quality,
        width: img.width(),
        height: img.height(),
        scaled,
        hit_target: hit,
        ok: true,
        error: None,
    })
}

pub fn run(opts: &CompressOptions, progress: &super::ProgressFn) -> CompressResult {
    let total = opts.inputs.len() as u64;
    let mut items = Vec::new();
    for (i, input) in opts.inputs.iter().enumerate() {
        super::report(
            progress,
            "img-compress",
            "progress",
            i as u64,
            Some(total),
            Some(input.clone()),
        );
        items.push(compress_one(opts, input).unwrap_or_else(|e| {
            tracing::warn!("[img-compress] {}: {}", input, e);
            CompressItem {
                input: input.clone(),
                output: None,
                bytes_before: std::fs::metadata(input).map(|m| m.len()).unwrap_or(0),
                bytes_after: 0,
                quality: 0,
                width: 0,
                height: 0,
                scaled: false,
                hit_target: false,
                ok: false,
                error: Some(e.to_string()),
            }
        }));
    }
    super::report(progress, "img-compress", "done", total, Some(total), None);
    CompressResult {
        bytes_before: items.iter().map(|i| i.bytes_before).sum(),
        bytes_after: items.iter().filter(|i| i.ok).map(|i| i.bytes_after).sum(),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Photo-like image: gradient with a few shapes. Pure noise is the
    /// worst case for JPEG and does not represent what the user compresses.
    fn photo(w: u32, h: u32) -> DynamicImage {
        let mut img = image::RgbImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;
            let blob = if ((fx - 0.3).powi(2) + (fy - 0.4).powi(2)) < 0.02 {
                60.0
            } else {
                0.0
            };
            *p = image::Rgb([
                (fx * 200.0 + blob) as u8,
                (fy * 180.0 + 40.0) as u8,
                ((1.0 - fx) * 160.0 + blob / 2.0) as u8,
            ]);
        }
        DynamicImage::ImageRgb8(img)
    }

    /// Colour noise: compresses badly on purpose, so the target really bites.
    fn noisy(w: u32, h: u32) -> DynamicImage {
        let mut img = image::RgbImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let n = (x * 7919 + y * 104_729) % 251;
            *p = image::Rgb([n as u8, ((n * 3) % 251) as u8, ((n * 7) % 251) as u8]);
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn quality_changes_the_size_monotonically() {
        let img = noisy(400, 300);
        let low = encode_jpeg(&img, 30).unwrap().len();
        let high = encode_jpeg(&img, 95).unwrap().len();
        assert!(
            high > low,
            "higher quality must weigh more: {} vs {}",
            high,
            low
        );
    }

    #[test]
    fn search_lands_under_the_target() {
        let img = photo(600, 400);
        let target = 40 * 1024;
        let (bytes, q, fits) = fit_quality(&img, target, 30, 95).unwrap();
        assert!(fits, "did not fit the target");
        assert!(
            bytes.len() as u64 <= target,
            "{} bytes > {}",
            bytes.len(),
            target
        );
        assert!((30..=95).contains(&q));
        let _ = &bytes;
        // And it is the best quality possible: one step up already overflows.
        if q < 95 {
            let bigger = encode_jpeg(&img, q + 4).unwrap();
            assert!(
                bigger.len() as u64 > target,
                "a higher quality could have been used ({} still fit)",
                q + 4
            );
        }
    }

    #[test]
    fn an_impossible_target_is_reported_not_faked() {
        let img = noisy(1200, 900);
        let (bytes, q, fits) = fit_quality(&img, 1024, 30, 95).unwrap();
        assert!(!fits, "1 KB does not fit, it had to warn");
        assert_eq!(q, 30, "returns the minimum so the caller can shrink");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn resize_only_shrinks() {
        use image::GenericImageView;
        let img = noisy(100, 50);
        assert_eq!(resize_to_side(&img, 0).dimensions(), (100, 50));
        assert_eq!(
            resize_to_side(&img, 500).dimensions(),
            (100, 50),
            "does not upscale"
        );
        assert_eq!(resize_to_side(&img, 50).dimensions(), (50, 25));
    }

    #[test]
    fn transparency_is_flattened_before_the_jpeg() {
        let mut rgba = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 0]));
        rgba.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let flat = flatten(&DynamicImage::ImageRgba8(rgba), "#FFFFFF").to_rgba8();
        assert_eq!(flat.get_pixel(3, 3), &image::Rgba([255, 255, 255, 255]));
        assert_eq!(flat.get_pixel(0, 0), &image::Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn end_to_end_hits_the_target_and_shrinks_when_it_must() {
        let dir = std::env::temp_dir().join("gumballz-get-imgcompress-test");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("grande.png");
        photo(1400, 1000).save(&src).unwrap();
        let before = std::fs::metadata(&src).unwrap().len();
        // Target relative to the source file: a gradient PNG is already small,
        // and a fixed target could be larger than it.
        let target_kb = (before / 1024 / 2).max(8);

        let opts = CompressOptions {
            inputs: vec![src.to_string_lossy().to_string()],
            target_kb,
            quality: 82,
            max_side: 0,
            background: "#FFFFFF".into(),
            dry_run: false,
            output_dir: dir.to_string_lossy().to_string(),
            suffix: "-out".into(),
        };
        let res = run(&opts, &crate::core::tools::noop_progress());
        let item = &res.items[0];
        assert!(item.ok, "{:?}", item.error);
        assert!(item.hit_target, "did not reach the target");
        assert!(
            item.bytes_after <= target_kb * 1024,
            "{} bytes above the target of {} KB",
            item.bytes_after,
            target_kb
        );
        assert!(item.bytes_after < before, "did not end up smaller than the original");
        let on_disk = std::fs::metadata(item.output.as_ref().unwrap())
            .unwrap()
            .len();
        assert_eq!(
            on_disk, item.bytes_after,
            "the reported size must be the written size"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_impossible_target_is_admitted_end_to_end() {
        let dir = std::env::temp_dir().join("gumballz-get-imgcompress-hard");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("ruido.png");
        noisy(1600, 1200).save(&src).unwrap();
        let res = run(
            &CompressOptions {
                inputs: vec![src.to_string_lossy().to_string()],
                target_kb: 2,
                quality: 82,
                max_side: 0,
                background: "#FFFFFF".into(),
                dry_run: false,
                output_dir: dir.to_string_lossy().to_string(),
                suffix: "-out".into(),
            },
            &crate::core::tools::noop_progress(),
        );
        let item = &res.items[0];
        assert!(item.ok, "must deliver best effort, not fail");
        assert!(!item.hit_target, "2 KB is impossible and the UI needs to know");
        assert!(item.scaled, "it should have shrunk while trying");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dry_run_writes_nothing() {
        let dir = std::env::temp_dir().join("gumballz-get-imgcompress-dry");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("teste.png");
        photo(200, 200).save(&src).unwrap();
        let res = run(
            &CompressOptions {
                inputs: vec![src.to_string_lossy().to_string()],
                target_kb: 0,
                quality: 70,
                max_side: 0,
                background: "#FFFFFF".into(),
                dry_run: true,
                output_dir: dir.to_string_lossy().to_string(),
                suffix: "-out".into(),
            },
            &crate::core::tools::noop_progress(),
        );
        assert!(res.items[0].ok);
        assert!(res.items[0].output.is_none());
        assert!(!dir.join("teste-out.jpg").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
