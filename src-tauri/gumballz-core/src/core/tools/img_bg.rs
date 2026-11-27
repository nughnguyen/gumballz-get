//! Remove the background of an image (person, product, whatever), one or in batch.
//!
//! Runs **locally**: no image leaves the machine. Segmentation is a managed ONNX model
//! (`core/tools/onnx.rs`) running on the managed runtime
//! (`core/onnxrt.rs`). The pre- and post-processing follow what rembg
//! (danielgatis/rembg, MIT) does with the same weights — the same `resize` for the
//! model input, the same mean/std normalization, and the output mask
//! min-max normalized before becoming alpha.
//!
//! The output is an RGBA PNG with a transparent background, or a JPEG/PNG over a
//! solid colour chosen by the user. The mask alone can also be saved.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use image::{imageops::FilterType, DynamicImage, GrayImage, Rgba, RgbaImage};
use ndarray::Array4;
use serde::{Deserialize, Serialize};

use super::ProgressFn;

const TOOL_ID: &str = "img-bg";
const DEFAULT_SUFFIX: &str = "-nobg";

/// Pre-processing of each segmentation model. Comes from rembg: each weight
/// family was trained with its own input size and normalization,
/// and swapping that ruins the mask without any error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BgParams {
    pub model_id: &'static str,
    pub size: u32,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

const PARAMS: &[BgParams] = &[
    BgParams {
        model_id: "u2netp",
        size: 320,
        mean: IMAGENET_MEAN,
        std: IMAGENET_STD,
    },
    BgParams {
        model_id: "u2net",
        size: 320,
        mean: IMAGENET_MEAN,
        std: IMAGENET_STD,
    },
    BgParams {
        model_id: "silueta",
        size: 320,
        mean: IMAGENET_MEAN,
        std: IMAGENET_STD,
    },
    BgParams {
        model_id: "isnet-general-use",
        size: 1024,
        mean: [0.5, 0.5, 0.5],
        std: [1.0, 1.0, 1.0],
    },
];

pub fn params_for(model_id: &str) -> Option<&'static BgParams> {
    PARAMS.iter().find(|p| p.model_id == model_id)
}

/// Default model: the smallest of the family, so first use does not ask for 175 MB.
pub fn default_model() -> &'static str {
    "u2netp"
}

#[derive(Debug, Clone, Deserialize)]
pub struct BgOptions {
    /// Arquivos escolhidos um a um.
    #[serde(default)]
    pub inputs: Vec<String>,
    /// Whole folder (does not enter subfolders).
    #[serde(default)]
    pub input_dir: String,
    /// Id of the model in the ONNX catalog. Empty = the default.
    #[serde(default)]
    pub model: String,
    /// Empty = same folder as the input file.
    #[serde(default)]
    pub output_dir: String,
    /// Vazio = `-nobg`.
    #[serde(default)]
    pub suffix: String,
    /// Empty = transparent background. `#RRGGBB` or `#RRGGBBAA` = solid colour.
    #[serde(default)]
    pub background: String,
    /// "png" | "jpg"
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_quality")]
    pub quality: u8,
    /// Alpha below this becomes zero, to kill background dust. 0 = off.
    #[serde(default)]
    pub alpha_threshold: u8,
    /// Hard edge: alpha becomes 0 or 255. Good for logos, bad for hair.
    #[serde(default)]
    pub hard_edges: bool,
    /// Saves only the mask in greyscale, without cutting the image.
    #[serde(default)]
    pub mask_only: bool,
}

fn default_format() -> String {
    "png".into()
}
fn default_quality() -> u8 {
    92
}

#[derive(Debug, Clone, Serialize)]
pub struct BgItem {
    pub input: String,
    pub output: Option<String>,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BgResult {
    pub model: String,
    pub items: Vec<BgItem>,
    pub done: usize,
    pub failed: usize,
}

// ── Pure pieces (tested without model or runtime) ──────────────────────

/// `#RGB`, `#RRGGBB` or `#RRGGBBAA`. Empty returns `None` (= transparent).
pub fn parse_hex_color(raw: &str) -> anyhow::Result<Option<Rgba<u8>>> {
    let s = raw.trim().trim_start_matches('#');
    if s.is_empty() {
        return Ok(None);
    }
    let byte = |i: usize| -> anyhow::Result<u8> {
        u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| anyhow!("invalid color: {raw}"))
    };
    let px = match s.len() {
        3 => {
            let mut v = [0u8; 3];
            for (i, c) in s.chars().enumerate() {
                let d = c
                    .to_digit(16)
                    .ok_or_else(|| anyhow!("invalid color: {raw}"))? as u8;
                v[i] = d * 17;
            }
            Rgba([v[0], v[1], v[2], 255])
        }
        6 => Rgba([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Rgba([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => return Err(anyhow!("invalid color: {raw}")),
    };
    Ok(Some(px))
}

/// Image → normalized `NCHW` tensor, the way the weights expect.
///
/// rembg divides the array by the **largest value of the array itself** and only then
/// applies mean/std; keeping that is what makes the mask come out like its own.
pub fn normalize_input(img: &DynamicImage, p: &BgParams) -> Array4<f32> {
    let size = p.size as usize;
    let rgb = image::imageops::resize(&img.to_rgb8(), p.size, p.size, FilterType::Lanczos3);
    let max = rgb.iter().copied().max().unwrap_or(0) as f32;
    let scale = 1.0 / max.max(1e-6);
    let mut t = Array4::<f32>::zeros((1, 3, size, size));
    for y in 0..size {
        for x in 0..size {
            let px = rgb.get_pixel(x as u32, y as u32).0;
            for c in 0..3 {
                t[[0, c, y, x]] = (px[c] as f32 * scale - p.mean[c]) / p.std[c];
            }
        }
    }
    t
}

/// Raw model output → greyscale mask, min-max normalized.
/// Constant output (image all background or all object) becomes a zeroed mask
/// instead of dividing by zero.
pub fn mask_from_raw(raw: &[f32], w: u32, h: u32) -> anyhow::Result<GrayImage> {
    let n = (w as usize) * (h as usize);
    if raw.len() < n {
        return Err(anyhow!(
            "the model output has {} values, expected {}",
            raw.len(),
            n
        ));
    }
    let slice = &raw[..n];
    let mut mi = f32::INFINITY;
    let mut ma = f32::NEG_INFINITY;
    for v in slice {
        if v.is_finite() {
            mi = mi.min(*v);
            ma = ma.max(*v);
        }
    }
    let span = ma - mi;
    let buf: Vec<u8> = if !span.is_finite() || span <= f32::EPSILON {
        vec![0u8; n]
    } else {
        slice
            .iter()
            .map(|v| (((v - mi) / span) * 255.0).round().clamp(0.0, 255.0) as u8)
            .collect()
    };
    GrayImage::from_raw(w, h, buf).ok_or_else(|| anyhow!("mask with inconsistent size"))
}

/// Optional mask cleanup: weak-alpha cut and/or hard edge.
pub fn clean_mask(mask: &GrayImage, alpha_threshold: u8, hard_edges: bool) -> GrayImage {
    let mut out = mask.clone();
    for px in out.pixels_mut() {
        let mut v = px.0[0];
        if alpha_threshold > 0 && v < alpha_threshold {
            v = 0;
        }
        if hard_edges {
            v = if v >= 128 { 255 } else { 0 };
        }
        px.0[0] = v;
    }
    out
}

/// Applies the mask as alpha and, if there is a colour, composites over it.
///
/// The original image's alpha is preserved (multiplied by the mask), so
/// a PNG that already had transparency does not lose it.
pub fn compose(base: &RgbaImage, mask: &GrayImage, background: Option<Rgba<u8>>) -> RgbaImage {
    let (w, h) = base.dimensions();
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let src = base.get_pixel(x, y).0;
            let m = mask.get_pixel(x, y).0[0] as u32;
            let a = ((src[3] as u32 * m) / 255) as u8;
            out.put_pixel(x, y, Rgba([src[0], src[1], src[2], a]));
        }
    }
    match background {
        None => out,
        Some(bg) => {
            let mut flat = RgbaImage::from_pixel(w, h, bg);
            for y in 0..h {
                for x in 0..w {
                    let fg = out.get_pixel(x, y).0;
                    let a = fg[3] as u32;
                    let dst = flat.get_pixel(x, y).0;
                    let mix = |f: u8, b: u8| ((f as u32 * a + b as u32 * (255 - a)) / 255) as u8;
                    let alpha = (a + dst[3] as u32 * (255 - a) / 255).min(255) as u8;
                    flat.put_pixel(
                        x,
                        y,
                        Rgba([
                            mix(fg[0], dst[0]),
                            mix(fg[1], dst[1]),
                            mix(fg[2], dst[2]),
                            alpha,
                        ]),
                    );
                }
            }
            flat
        }
    }
}

fn is_image_file(path: &Path) -> bool {
    const EXT: [&str; 8] = ["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Joins chosen files and folder, without repeats and in stable order.
pub fn collect_inputs(inputs: &[String], input_dir: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for raw in inputs {
        let p = PathBuf::from(raw.trim());
        if p.is_file() {
            out.push(p);
        }
    }
    let dir = input_dir.trim();
    if !dir.is_empty() {
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut found: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && is_image_file(p))
                .collect();
            found.sort();
            out.extend(found);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Output path of a file, respecting folder, suffix and format.
pub fn output_path(input: &Path, output_dir: &str, suffix: &str, ext: &str) -> PathBuf {
    let dir = if output_dir.trim().is_empty() {
        input.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    } else {
        PathBuf::from(output_dir.trim())
    };
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".into());
    let suffix = if suffix.is_empty() {
        DEFAULT_SUFFIX
    } else {
        suffix
    };
    dir.join(format!("{stem}{suffix}.{ext}"))
}

/// "jpg"/"jpeg" become JPEG; anything else becomes PNG. JPEG does not keep
/// alpha, so without a background colour the cut would come out over black — in that case the
/// choice is ignored and the file comes out as PNG.
pub fn resolve_format(format: &str, has_background: bool, mask_only: bool) -> &'static str {
    let f = format.trim().to_ascii_lowercase();
    let wants_jpeg = f == "jpg" || f == "jpeg";
    if wants_jpeg && (has_background || mask_only) {
        "jpg"
    } else {
        "png"
    }
}

fn save_image(img: &DynamicImage, dest: &Path, ext: &str, quality: u8) -> anyhow::Result<u64> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("criando {}", parent.display()))?;
    }
    if ext == "jpg" {
        let file =
            std::fs::File::create(dest).with_context(|| format!("criando {}", dest.display()))?;
        let mut w = std::io::BufWriter::new(file);
        let mut enc =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, quality.clamp(1, 100));
        enc.encode_image(&img.to_rgb8())
            .map_err(|e| anyhow!("could not save the JPEG {}: {}", dest.display(), e))?;
    } else {
        img.save_with_format(dest, image::ImageFormat::Png)
            .map_err(|e| anyhow!("could not save the PNG {}: {}", dest.display(), e))?;
    }
    Ok(std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0))
}

// ── Execution ───────────────────────────────────────────────────────────

/// Runs inference on an already-open file and returns the mask at the image's
/// original size.
fn mask_for_image(
    session: &mut ort::session::Session,
    img: &DynamicImage,
    p: &BgParams,
) -> anyhow::Result<GrayImage> {
    let side = p.size as i64;
    // `ort` brings its own `ndarray` (0.17) and the crate uses 0.16, so the
    // tensor crosses the boundary as shape + contiguous data, which is what
    // `Tensor::from_array` accepts without depending on any crate version.
    let (data, _) = normalize_input(img, p).into_raw_vec_and_offset();
    let tensor = ort::value::Tensor::from_array((vec![1, 3, side, side], data))
        .map_err(|e| anyhow!("could not build the input tensor: {e}"))?;
    let outputs = session
        .run(ort::inputs![tensor])
        .map_err(|e| anyhow!("inference failed: {e}"))?;
    let (shape, raw) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| anyhow!("could not read the model output: {e}"))?;
    if shape.len() < 2 {
        return Err(anyhow!(
            "model output with unexpected format: {:?}",
            &shape[..]
        ));
    }
    let h = shape[shape.len() - 2].max(0) as u32;
    let w = shape[shape.len() - 1].max(0) as u32;
    let small = mask_from_raw(raw, w, h)?;
    let (ow, oh) = (img.width(), img.height());
    Ok(image::imageops::resize(
        &small,
        ow,
        oh,
        FilterType::Lanczos3,
    ))
}

fn run_blocking(
    opts: &BgOptions,
    model_id: &str,
    progress: &ProgressFn,
) -> anyhow::Result<BgResult> {
    let params = *params_for(model_id)
        .ok_or_else(|| anyhow!("model {model_id} cannot remove background"))?;
    let files = collect_inputs(&opts.inputs, &opts.input_dir);
    if files.is_empty() {
        return Err(anyhow!("choose at least one image"));
    }
    let background = parse_hex_color(&opts.background)?;
    let ext = resolve_format(&opts.format, background.is_some(), opts.mask_only);

    let mut session = super::onnx::session_for(model_id)?;
    let total = files.len() as u64;
    let mut items: Vec<BgItem> = Vec::with_capacity(files.len());
    let mut failed = 0usize;

    for (i, path) in files.iter().enumerate() {
        super::report(
            progress,
            TOOL_ID,
            "progress",
            i as u64,
            Some(total),
            Some(path.to_string_lossy().to_string()),
        );
        let one = (|| -> anyhow::Result<(PathBuf, u32, u32, u64)> {
            let img = image::open(path).with_context(|| format!("abrindo {}", path.display()))?;
            let mask = mask_for_image(&mut session, &img, &params)?;
            let mask = clean_mask(&mask, opts.alpha_threshold, opts.hard_edges);
            let dest = output_path(path, &opts.output_dir, &opts.suffix, ext);
            let out = if opts.mask_only {
                DynamicImage::ImageLuma8(mask)
            } else {
                DynamicImage::ImageRgba8(compose(&img.to_rgba8(), &mask, background))
            };
            let bytes = save_image(&out, &dest, ext, opts.quality)?;
            Ok((dest, out.width(), out.height(), bytes))
        })();
        match one {
            Ok((dest, w, h, bytes)) => items.push(BgItem {
                input: path.to_string_lossy().to_string(),
                output: Some(dest.to_string_lossy().to_string()),
                width: w,
                height: h,
                bytes,
                ok: true,
                error: None,
            }),
            Err(e) => {
                failed += 1;
                tracing::warn!("[img-bg] {} failed: {}", path.display(), e);
                items.push(BgItem {
                    input: path.to_string_lossy().to_string(),
                    output: None,
                    width: 0,
                    height: 0,
                    bytes: 0,
                    ok: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    super::report(progress, TOOL_ID, "done", total, Some(total), None);
    Ok(BgResult {
        model: model_id.to_string(),
        done: items.len() - failed,
        failed,
        items,
    })
}

/// Tool entry point. Guarantees runtime and model before any
/// inference, and only then throws the heavy work onto a blocking thread.
pub async fn run(opts: BgOptions, progress: ProgressFn) -> anyhow::Result<BgResult> {
    let model_id = if opts.model.trim().is_empty() {
        default_model().to_string()
    } else {
        opts.model.trim().to_string()
    };
    if params_for(&model_id).is_none() {
        return Err(anyhow!("model {model_id} cannot remove background"));
    }
    // Actionable error before downloading 180 MB for nothing.
    crate::core::onnxrt::init()?;
    super::onnx::ensure_model(&model_id, &progress).await?;

    let p = progress.clone();
    tokio::task::spawn_blocking(move || run_blocking(&opts, &model_id, &p))
        .await
        .map_err(|e| anyhow!("background removal task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(w: u32, h: u32, v: u8) -> GrayImage {
        GrayImage::from_pixel(w, h, image::Luma([v]))
    }

    #[test]
    fn cada_modelo_do_catalogo_rembg_tem_pre_processamento() {
        for m in super::super::onnx::family("rembg") {
            let p = params_for(m.id)
                .unwrap_or_else(|| panic!("model {} without input parameters", m.id));
            assert!(p.size >= 320);
            assert!(p.std.iter().all(|s| *s > 0.0), "desvio zero em {}", m.id);
        }
    }

    #[test]
    fn o_modelo_padrao_existe_no_catalogo_e_tem_parametros() {
        assert!(super::super::onnx::find(default_model()).is_some());
        assert!(params_for(default_model()).is_some());
        assert!(params_for("nao-existe").is_none());
    }

    #[test]
    fn hex_colors_with_three_six_and_eight_digits() {
        assert_eq!(parse_hex_color("").expect("empty"), None);
        assert_eq!(parse_hex_color("   ").expect("whitespace"), None);
        assert_eq!(
            parse_hex_color("#FF8800").expect("6"),
            Some(Rgba([255, 136, 0, 255]))
        );
        assert_eq!(
            parse_hex_color("f80").expect("3"),
            Some(Rgba([255, 136, 0, 255]))
        );
        assert_eq!(
            parse_hex_color("#0080FF40").expect("8"),
            Some(Rgba([0, 128, 255, 64]))
        );
        assert!(parse_hex_color("#12345").is_err());
        assert!(parse_hex_color("#GGGGGG").is_err());
    }

    #[test]
    fn a_normalizacao_da_o_formato_nchw_e_desfaz_media_e_desvio() {
        let p = BgParams {
            model_id: "teste",
            size: 4,
            mean: [0.5, 0.5, 0.5],
            std: [1.0, 1.0, 1.0],
        };
        // White image: the largest value is 255, so everything becomes 1.0 before the mean.
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            8,
            8,
            image::Rgb([255, 255, 255]),
        ));
        let t = normalize_input(&img, &p);
        assert_eq!(t.shape(), &[1, 3, 4, 4]);
        for v in t.iter() {
            assert!((v - 0.5).abs() < 1e-5, "expected 0.5, got {v}");
        }
    }

    #[test]
    fn a_normalizacao_separa_os_canais_na_ordem_certa() {
        let p = BgParams {
            model_id: "teste",
            size: 2,
            mean: [0.0, 0.0, 0.0],
            std: [1.0, 1.0, 1.0],
        };
        let img =
            DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 4, image::Rgb([255, 0, 0])));
        let t = normalize_input(&img, &p);
        assert!((t[[0, 0, 0, 0]] - 1.0).abs() < 1e-5, "canal R");
        assert!(t[[0, 1, 0, 0]].abs() < 1e-5, "canal G");
        assert!(t[[0, 2, 0, 0]].abs() < 1e-5, "canal B");
    }

    #[test]
    fn a_mascara_normaliza_por_min_e_max() {
        let raw = vec![-1.0f32, 0.0, 1.0, 3.0];
        let m = mask_from_raw(&raw, 2, 2).expect("mask");
        assert_eq!(m.get_pixel(0, 0).0[0], 0);
        assert_eq!(m.get_pixel(1, 1).0[0], 255);
        // 0.0 is 1/4 of the way between -1 and 3.
        assert_eq!(m.get_pixel(1, 0).0[0], 64);
    }

    #[test]
    fn saida_constante_vira_mascara_zerada_em_vez_de_dividir_por_zero() {
        let m = mask_from_raw(&[0.7f32; 4], 2, 2).expect("mask");
        assert!(m.pixels().all(|p| p.0[0] == 0));
    }

    #[test]
    fn saida_curta_demais_da_erro() {
        assert!(mask_from_raw(&[0.1f32, 0.2], 2, 2).is_err());
    }

    #[test]
    fn a_mascara_ignora_valores_a_mais_do_batch() {
        // (1, 2, 2, 2): only the first channel matters.
        let raw = vec![0.0f32, 1.0, 0.0, 1.0, 9.0, 9.0, 9.0, 9.0];
        let m = mask_from_raw(&raw, 2, 2).expect("mask");
        assert_eq!(m.get_pixel(0, 0).0[0], 0);
        assert_eq!(m.get_pixel(1, 0).0[0], 255);
    }

    #[test]
    fn corte_de_alfa_fraco_e_borda_dura() {
        let mut m = gray(3, 1, 0);
        m.put_pixel(0, 0, image::Luma([10]));
        m.put_pixel(1, 0, image::Luma([130]));
        m.put_pixel(2, 0, image::Luma([250]));

        let corte = clean_mask(&m, 32, false);
        assert_eq!(corte.get_pixel(0, 0).0[0], 0);
        assert_eq!(corte.get_pixel(1, 0).0[0], 130);

        let dura = clean_mask(&m, 0, true);
        assert_eq!(dura.get_pixel(0, 0).0[0], 0);
        assert_eq!(dura.get_pixel(1, 0).0[0], 255);
        assert_eq!(dura.get_pixel(2, 0).0[0], 255);
    }

    #[test]
    fn a_composicao_transparente_apaga_o_fundo_e_mantem_o_objeto() {
        let base = RgbaImage::from_pixel(2, 1, Rgba([10, 20, 30, 255]));
        let mut mask = gray(2, 1, 255);
        mask.put_pixel(1, 0, image::Luma([0]));
        let out = compose(&base, &mask, None);
        assert_eq!(out.get_pixel(0, 0).0, [10, 20, 30, 255]);
        assert_eq!(
            out.get_pixel(1, 0).0[3],
            0,
            "background had to become invisible"
        );
    }

    #[test]
    fn a_composicao_respeita_o_alfa_que_a_imagem_ja_tinha() {
        let base = RgbaImage::from_pixel(1, 1, Rgba([10, 20, 30, 128]));
        let out = compose(&base, &gray(1, 1, 255), None);
        assert_eq!(out.get_pixel(0, 0).0[3], 128);
    }

    #[test]
    fn com_cor_de_fundo_o_resultado_fica_opaco_e_meio_a_meio_na_borda() {
        let base = RgbaImage::from_pixel(3, 1, Rgba([0, 0, 0, 255]));
        let mut mask = gray(3, 1, 255);
        mask.put_pixel(1, 0, image::Luma([128]));
        mask.put_pixel(2, 0, image::Luma([0]));
        let out = compose(&base, &mask, Some(Rgba([255, 255, 255, 255])));
        assert_eq!(out.get_pixel(0, 0).0, [0, 0, 0, 255], "objeto puro");
        assert_eq!(out.get_pixel(2, 0).0, [255, 255, 255, 255], "fundo puro");
        let meio = out.get_pixel(1, 0).0;
        assert_eq!(meio[3], 255);
        assert!(
            (100..=160).contains(&meio[0]),
            "the edge should have blended, got {meio:?}"
        );
    }

    #[test]
    fn jpeg_so_quando_nao_precisa_de_alfa() {
        assert_eq!(resolve_format("jpg", true, false), "jpg");
        assert_eq!(resolve_format("jpeg", false, true), "jpg");
        assert_eq!(resolve_format("jpg", false, false), "png");
        assert_eq!(resolve_format("png", true, false), "png");
        assert_eq!(resolve_format("", false, false), "png");
    }

    #[test]
    fn o_nome_de_saida_usa_sufixo_pasta_e_extensao() {
        let inp = PathBuf::from("/tmp/fotos/gato.jpeg");
        let a = output_path(&inp, "", "", "png");
        assert_eq!(a, PathBuf::from("/tmp/fotos/gato-nobg.png"));
        let b = output_path(&inp, "/saida", "_corte", "jpg");
        assert_eq!(b, PathBuf::from("/saida/gato_corte.jpg"));
    }

    #[test]
    fn input_list_joins_folder_and_files_without_repeats() {
        let dir = std::env::temp_dir().join(format!("gumballz-get-imgbg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test folder");
        let a = dir.join("a.png");
        let b = dir.join("b.jpg");
        let txt = dir.join("leia.txt");
        image::RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]))
            .save(&a)
            .expect("test png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
            .save(&b)
            .expect("test jpg");
        std::fs::write(&txt, "nada").expect("test txt");

        let got = collect_inputs(&[a.to_string_lossy().to_string()], &dir.to_string_lossy());
        assert_eq!(got, vec![a.clone(), b.clone()], "without repeats and without the .txt");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sem_entrada_nenhuma_a_lista_fica_vazia() {
        assert!(collect_inputs(&[], "").is_empty());
        assert!(collect_inputs(&["/nao/existe.png".into()], "/nao/existe/").is_empty());
    }

    /// Needs network and the installed runtime: downloads u2netp, runs on a
    /// synthetic image (light square in the middle of a dark background) and checks that
    /// a right-sized RGBA PNG came out.
    #[test]
    #[ignore = "network + runtime: downloads u2netp and removes the background of a synthetic image"]
    fn removes_the_background_for_real_with_u2netp() {
        let dir = std::env::temp_dir().join("gumballz-get-imgbg-live");
        std::fs::create_dir_all(&dir).expect("folder");
        let src = dir.join("alvo.png");
        let mut img = image::RgbImage::from_pixel(256, 256, image::Rgb([12, 14, 18]));
        for y in 80..176 {
            for x in 80..176 {
                img.put_pixel(x, y, image::Rgb([240, 230, 210]));
            }
        }
        img.save(&src).expect("test image");

        let rt = tokio::runtime::Runtime::new().expect("runtime tokio");
        let opts = BgOptions {
            inputs: vec![src.to_string_lossy().to_string()],
            input_dir: String::new(),
            model: "u2netp".into(),
            output_dir: dir.to_string_lossy().to_string(),
            suffix: String::new(),
            background: String::new(),
            format: "png".into(),
            quality: 92,
            alpha_threshold: 0,
            hard_edges: false,
            mask_only: false,
        };
        let res = rt
            .block_on(run(opts, super::super::noop_progress()))
            .expect("background removal");
        assert_eq!(res.failed, 0, "itens: {:?}", res.items);
        let out = res.items[0].output.clone().expect("output");
        let saved = image::open(&out).expect("open output").to_rgba8();
        assert_eq!(saved.dimensions(), (256, 256));
        // The light square in the middle is the object; the corner is background. If
        // normalization had the axes or channels swapped, the mask
        // would come out inverted or scrambled and one of these two would fail.
        assert!(
            saved.get_pixel(128, 128).0[3] > 200,
            "the middle object should have stayed opaque"
        );
        assert!(
            saved.get_pixel(4, 4).0[3] < 64,
            "the corner was background and should have stayed transparent"
        );
    }
}
