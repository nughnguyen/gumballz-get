//! View and clean image metadata (EXIF/XMP/IPTC), focusing on what leaks:
//! GPS, camera serial number, owner name, date and software.
//!
//! Reading uses `kamadak-exif` (pure Rust). Cleaning does NOT re-encode anything:
//! it rewrites the container without the metadata segments, so the pixels come out
//! byte-for-byte identical to the original — the opposite of what a "save as" does.

use std::path::{Path, PathBuf};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ExifField {
    pub tag: String,
    pub label: String,
    pub group: String,
    pub value: String,
    /// Field that identifies a person, place or piece of equipment.
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExifReport {
    pub path: String,
    pub bytes: u64,
    pub has_exif: bool,
    pub fields: Vec<ExifField>,
    /// (latitude, longitude) in decimal degrees, when GPS exists.
    pub gps: Option<(f64, f64)>,
    pub camera: Option<String>,
    pub taken: Option<String>,
    pub software: Option<String>,
    /// How many sensitive fields were found (GPS counts as one).
    pub sensitive_count: usize,
    pub error: Option<String>,
}

const SENSITIVE_PREFIXES: &[&str] = &["GPS"];
const SENSITIVE_TAGS: &[&str] = &[
    "BodySerialNumber",
    "CameraOwnerName",
    "LensSerialNumber",
    "SerialNumber",
    "Artist",
    "Copyright",
    "ImageUniqueID",
    "OwnerName",
];

fn is_sensitive(tag: &str) -> bool {
    SENSITIVE_PREFIXES.iter().any(|p| tag.starts_with(p)) || SENSITIVE_TAGS.contains(&tag)
}

/// Degrees/minutes/seconds + hemisphere → decimal degrees.
pub fn dms_to_deg(dms: &[f64], reference: &str) -> Option<f64> {
    if dms.is_empty() {
        return None;
    }
    let d = dms.first().copied().unwrap_or(0.0);
    let m = dms.get(1).copied().unwrap_or(0.0);
    let s = dms.get(2).copied().unwrap_or(0.0);
    let deg = d + m / 60.0 + s / 3600.0;
    let r = reference.trim().to_uppercase();
    Some(if r.starts_with('S') || r.starts_with('W') {
        -deg
    } else {
        deg
    })
}

fn rationals(field: &exif::Field) -> Vec<f64> {
    match &field.value {
        exif::Value::Rational(v) => v.iter().map(|r| r.to_f64()).collect(),
        exif::Value::SRational(v) => v.iter().map(|r| r.to_f64()).collect(),
        _ => Vec::new(),
    }
}

fn text_of(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    exif.get_field(tag, exif::In::PRIMARY).map(|f| {
        f.display_value()
            .to_string()
            .trim_matches('"')
            .trim()
            .to_string()
    })
}

pub fn read(path: &str) -> ExifReport {
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut report = ExifReport {
        path: path.to_string(),
        bytes,
        has_exif: false,
        fields: Vec::new(),
        gps: None,
        camera: None,
        taken: None,
        software: None,
        sensitive_count: 0,
        error: None,
    };
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            report.error = Some(e.to_string());
            return report;
        }
    };
    let mut reader = std::io::BufReader::new(&file);
    let exif = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(e) => e,
        Err(e) => {
            // "no EXIF" is a legitimate answer, not a failure.
            if !matches!(e, exif::Error::NotFound(_)) {
                report.error = Some(e.to_string());
            }
            return report;
        }
    };

    for f in exif.fields() {
        let tag = f.tag.to_string();
        let sensitive = is_sensitive(&tag);
        if sensitive {
            report.sensitive_count += 1;
        }
        report.fields.push(ExifField {
            label: f.tag.description().unwrap_or("").to_string(),
            group: if tag.starts_with("GPS") {
                "GPS".into()
            } else if f.ifd_num == exif::In::THUMBNAIL {
                "Thumbnail".into()
            } else {
                "Image".into()
            },
            value: f.display_value().with_unit(&exif).to_string(),
            tag,
            sensitive,
        });
    }
    report.has_exif = !report.fields.is_empty();

    let lat = exif.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY);
    let lon = exif.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY);
    if let (Some(lat), Some(lon)) = (lat, lon) {
        let lat_ref = text_of(&exif, exif::Tag::GPSLatitudeRef).unwrap_or_else(|| "N".into());
        let lon_ref = text_of(&exif, exif::Tag::GPSLongitudeRef).unwrap_or_else(|| "E".into());
        if let (Some(a), Some(b)) = (
            dms_to_deg(&rationals(lat), &lat_ref),
            dms_to_deg(&rationals(lon), &lon_ref),
        ) {
            report.gps = Some((a, b));
        }
    }

    let make = text_of(&exif, exif::Tag::Make);
    let model = text_of(&exif, exif::Tag::Model);
    report.camera = match (make, model) {
        (Some(a), Some(b)) if b.starts_with(&a) => Some(b),
        (Some(a), Some(b)) => Some(format!("{} {}", a, b)),
        (a, b) => a.or(b),
    };
    report.taken =
        text_of(&exif, exif::Tag::DateTimeOriginal).or_else(|| text_of(&exif, exif::Tag::DateTime));
    report.software = text_of(&exif, exif::Tag::Software);
    report
}

pub fn read_many(inputs: &[String]) -> Vec<ExifReport> {
    inputs.iter().map(|p| read(p)).collect()
}

// ── Cleaning ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Jpeg,
    Png,
    Webp,
}

pub fn kind_of(data: &[u8]) -> Option<Kind> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Kind::Jpeg)
    } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Kind::Png)
    } else if data.len() > 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        Some(Kind::Webp)
    } else {
        None
    }
}

/// JPEG markers thrown away: APP1 (EXIF and XMP), APP13 (IPTC/Photoshop) and
/// the comment. APP0 (JFIF), APP2 (ICC profile) and APP14 (Adobe) stay — they are
/// what keeps the colour right.
fn strip_jpeg(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&data[0..2]);
    let mut i = 2usize;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            // Out of sync: copy the rest as-is instead of corrupting it.
            out.extend_from_slice(&data[i..]);
            return Ok(out);
        }
        let marker = data[i + 1];
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 || marker == 0xFF {
            out.extend_from_slice(&data[i..i + 2]);
            i += 2;
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            out.extend_from_slice(&data[i..]);
            return Ok(out);
        }
        if i + 3 >= data.len() {
            out.extend_from_slice(&data[i..]);
            return Ok(out);
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        let end = (i + 2 + len).min(data.len());
        let drop = marker == 0xE1 || marker == 0xED || marker == 0xFE;
        if !drop {
            out.extend_from_slice(&data[i..end]);
        }
        i = end;
    }
    Ok(out)
}

const PNG_DROP: &[&[u8; 4]] = &[b"tEXt", b"zTXt", b"iTXt", b"eXIf", b"tIME", b"dSIG"];

fn strip_png(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&data[0..8]);
    let mut i = 8usize;
    while i + 8 <= data.len() {
        let len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let ctype: [u8; 4] = [data[i + 4], data[i + 5], data[i + 6], data[i + 7]];
        let end = i
            .checked_add(12)
            .and_then(|v| v.checked_add(len))
            .ok_or_else(|| anyhow!("PNG with invalid chunk size"))?;
        if end > data.len() {
            return Err(anyhow!("truncated PNG"));
        }
        if !PNG_DROP.iter().any(|d| d.as_slice() == ctype) {
            out.extend_from_slice(&data[i..end]);
        }
        i = end;
        if &ctype == b"IEND" {
            break;
        }
    }
    Ok(out)
}

fn strip_webp(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut body: Vec<u8> = Vec::with_capacity(data.len());
    body.extend_from_slice(b"WEBP");
    let mut i = 12usize;
    while i + 8 <= data.len() {
        let fourcc: [u8; 4] = [data[i], data[i + 1], data[i + 2], data[i + 3]];
        let len = u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]) as usize;
        let padded = len + (len & 1);
        let end = (i + 8 + padded).min(data.len());
        let drop = &fourcc == b"EXIF" || &fourcc == b"XMP ";
        if !drop {
            let start = body.len();
            body.extend_from_slice(&data[i..end]);
            // VP8X stores in flags which chunks exist; without clearing the bits the
            // decoder looks for an EXIF that is no longer there.
            if &fourcc == b"VP8X" && body.len() > start + 8 {
                body[start + 8] &= !(0x08 | 0x04);
            }
        }
        i = end;
    }
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn strip_bytes(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    match kind_of(data) {
        Some(Kind::Jpeg) => strip_jpeg(data),
        Some(Kind::Png) => strip_png(data),
        Some(Kind::Webp) => strip_webp(data),
        None => Err(anyhow!(
            "no lossless cleanup for this format here — only JPEG, PNG and WebP"
        )),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StripOptions {
    pub inputs: Vec<String>,
    /// Overwrites the original file instead of creating a clean copy.
    #[serde(default)]
    pub in_place: bool,
    #[serde(default)]
    pub output_dir: String,
    #[serde(default)]
    pub suffix: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StripItem {
    pub input: String,
    pub output: Option<String>,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StripResult {
    pub items: Vec<StripItem>,
}

pub fn strip(opts: &StripOptions, progress: &super::ProgressFn) -> StripResult {
    let total = opts.inputs.len() as u64;
    let mut items = Vec::new();
    for (i, input) in opts.inputs.iter().enumerate() {
        super::report(
            progress,
            "img-exif",
            "progress",
            i as u64,
            Some(total),
            Some(input.clone()),
        );
        items.push(strip_one(opts, input));
    }
    super::report(progress, "img-exif", "done", total, Some(total), None);
    StripResult { items }
}

fn strip_one(opts: &StripOptions, input: &str) -> StripItem {
    let inp = Path::new(input);
    let before = std::fs::metadata(inp).map(|m| m.len()).unwrap_or(0);
    let fail = |e: String| StripItem {
        input: input.to_string(),
        output: None,
        bytes_before: before,
        bytes_after: 0,
        ok: false,
        error: Some(e),
    };
    let data = match std::fs::read(inp) {
        Ok(d) => d,
        Err(e) => return fail(e.to_string()),
    };
    let cleaned = match strip_bytes(&data) {
        Ok(c) => c,
        Err(e) => return fail(e.to_string()),
    };
    let out = if opts.in_place {
        inp.to_path_buf()
    } else {
        let dir = if opts.output_dir.trim().is_empty() {
            inp.parent().map(|p| p.to_path_buf()).unwrap_or_default()
        } else {
            PathBuf::from(opts.output_dir.trim())
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return fail(e.to_string());
        }
        let stem = inp
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "imagem".into());
        let ext = inp
            .extension()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "jpg".into());
        let suffix = if opts.suffix.is_empty() {
            "-clean"
        } else {
            opts.suffix.as_str()
        };
        dir.join(format!("{}{}.{}", stem, suffix, ext))
    };
    if let Err(e) = std::fs::write(&out, &cleaned) {
        return fail(e.to_string());
    }
    StripItem {
        input: input.to_string(),
        output: Some(out.to_string_lossy().to_string()),
        bytes_before: before,
        bytes_after: cleaned.len() as u64,
        ok: true,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dms_handles_hemispheres() {
        let sp = dms_to_deg(&[23.0, 33.0, 0.0], "S").unwrap();
        assert!((sp + 23.55).abs() < 1e-9);
        assert!(dms_to_deg(&[10.0, 30.0, 0.0], "E").unwrap() > 0.0);
        assert!(dms_to_deg(&[], "N").is_none());
    }

    #[test]
    fn sensitive_covers_gps_and_serials() {
        assert!(is_sensitive("GPSLatitude"));
        assert!(is_sensitive("BodySerialNumber"));
        assert!(!is_sensitive("ExposureTime"));
    }

    fn jpeg_with_exif() -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8];
        // APP0 (JFIF) stays.
        v.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00]);
        // APP1 (Exif) goes.
        v.extend_from_slice(&[0xFF, 0xE1, 0x00, 0x08]);
        v.extend_from_slice(b"Exif\0\0");
        // Comment goes.
        v.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x05, b'o', b'i', b'!']);
        // SOS and "data".
        v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x03, 0x00, 0xAA, 0xBB, 0xFF, 0xD9]);
        v
    }

    #[test]
    fn jpeg_loses_exif_and_keeps_pixels() {
        let src = jpeg_with_exif();
        let out = strip_bytes(&src).unwrap();
        assert_eq!(kind_of(&out), Some(Kind::Jpeg));
        assert!(!out.windows(4).any(|w| w == b"Exif"));
        assert!(!out.windows(2).any(|w| w == [0xFF, 0xFE]));
        // APP0 preserved and the scan intact up to the EOI.
        assert!(out.windows(2).any(|w| w == [0xFF, 0xE0]));
        assert!(out.ends_with(&[0xAA, 0xBB, 0xFF, 0xD9]));
        // Running it again changes nothing more.
        assert_eq!(strip_bytes(&out).unwrap(), out);
    }

    fn png_chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = (payload.len() as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(payload);
        v.extend_from_slice(&[0, 0, 0, 0]);
        v
    }

    #[test]
    fn png_loses_text_and_keeps_idat() {
        let mut src = b"\x89PNG\r\n\x1a\n".to_vec();
        src.extend(png_chunk(b"IHDR", &[0; 13]));
        src.extend(png_chunk(b"tEXt", b"Comment\0segredo"));
        src.extend(png_chunk(b"eXIf", b"II*\0"));
        src.extend(png_chunk(b"IDAT", &[1, 2, 3]));
        src.extend(png_chunk(b"IEND", b""));
        let out = strip_bytes(&src).unwrap();
        assert!(!out.windows(4).any(|w| w == b"tEXt"));
        assert!(!out.windows(4).any(|w| w == b"eXIf"));
        assert!(out.windows(4).any(|w| w == b"IDAT"));
        assert!(out.windows(4).any(|w| w == b"IHDR"));
        assert!(out.ends_with(b"IEND\0\0\0\0"));
    }

    fn riff_chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = kind.to_vec();
        v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        v.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            v.push(0);
        }
        v
    }

    #[test]
    fn webp_drops_exif_and_clears_vp8x_flag() {
        let mut body = b"WEBP".to_vec();
        // VP8X with the EXIF bit (0x08) on.
        body.extend(riff_chunk(b"VP8X", &[0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        body.extend(riff_chunk(b"VP8 ", &[9, 9, 9]));
        body.extend(riff_chunk(b"EXIF", b"II*\0segredo"));
        let mut src = b"RIFF".to_vec();
        src.extend_from_slice(&(body.len() as u32).to_le_bytes());
        src.extend_from_slice(&body);

        let out = strip_bytes(&src).unwrap();
        assert_eq!(kind_of(&out), Some(Kind::Webp));
        assert!(!out.windows(4).any(|w| w == b"EXIF"));
        assert!(out.windows(4).any(|w| w == b"VP8 "));
        let size = u32::from_le_bytes([out[4], out[5], out[6], out[7]]) as usize;
        assert_eq!(size, out.len() - 8, "RIFF size must match");
        let vp8x = out.windows(4).position(|w| w == b"VP8X").unwrap();
        assert_eq!(out[vp8x + 8] & 0x08, 0, "the EXIF bit stayed on");
    }

    /// Builds a JPEG with a real APP1/EXIF: IFD0 with Make and Model, and
    /// a GPS IFD with south latitude and west longitude (São Paulo).
    fn jpeg_with_gps() -> Vec<u8> {
        fn entry(out: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: [u8; 4]) {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&value);
        }
        // Offset map (relative to the start of the TIFF header).
        const MAKE_AT: u32 = 50;
        const MODEL_AT: u32 = 56;
        const GPS_IFD_AT: u32 = 64;
        const LAT_AT: u32 = 118;
        const LON_AT: u32 = 142;

        let mut t: Vec<u8> = Vec::new();
        t.extend_from_slice(b"II*\0");
        t.extend_from_slice(&8u32.to_le_bytes());
        t.extend_from_slice(&3u16.to_le_bytes());
        entry(&mut t, 0x010F, 2, 6, MAKE_AT.to_le_bytes());
        entry(&mut t, 0x0110, 2, 5, MODEL_AT.to_le_bytes());
        entry(&mut t, 0x8825, 4, 1, GPS_IFD_AT.to_le_bytes());
        t.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(t.len(), MAKE_AT as usize);
        t.extend_from_slice(b"LibrG\0");
        t.extend_from_slice(b"Test\0");
        while t.len() < GPS_IFD_AT as usize {
            t.push(0);
        }

        t.extend_from_slice(&4u16.to_le_bytes());
        entry(&mut t, 0x0001, 2, 2, *b"S\0\0\0");
        entry(&mut t, 0x0002, 5, 3, LAT_AT.to_le_bytes());
        entry(&mut t, 0x0003, 2, 2, *b"W\0\0\0");
        entry(&mut t, 0x0004, 5, 3, LON_AT.to_le_bytes());
        t.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(t.len(), LAT_AT as usize);
        for (num, den) in [(23u32, 1u32), (33, 1), (0, 1)] {
            t.extend_from_slice(&num.to_le_bytes());
            t.extend_from_slice(&den.to_le_bytes());
        }
        assert_eq!(t.len(), LON_AT as usize);
        for (num, den) in [(46u32, 1u32), (38, 1), (0, 1)] {
            t.extend_from_slice(&num.to_le_bytes());
            t.extend_from_slice(&den.to_le_bytes());
        }

        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(&t);
        let mut jpeg = vec![0xFF, 0xD8];
        jpeg.extend_from_slice(&[0xFF, 0xE1]);
        jpeg.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        jpeg.extend_from_slice(&app1);
        jpeg.extend_from_slice(&[0xFF, 0xD9]);
        jpeg
    }

    #[test]
    fn reads_gps_and_camera_then_strips_them() {
        let dir = std::env::temp_dir().join("gumballz-get-exif-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("com-gps.jpg");
        std::fs::write(&path, jpeg_with_gps()).unwrap();
        let p = path.to_string_lossy().to_string();

        let r = read(&p);
        assert!(r.error.is_none(), "erro na leitura: {:?}", r.error);
        assert!(r.has_exif);
        assert_eq!(r.camera.as_deref(), Some("LibrG Test"));
        let (lat, lon) = r.gps.expect("did not find the GPS");
        assert!((lat + 23.55).abs() < 1e-6, "latitude: {}", lat);
        assert!((lon + 46.6333333).abs() < 1e-5, "longitude: {}", lon);
        assert!(
            r.sensitive_count >= 4,
            "GPS fields not marked as sensitive"
        );

        let opts = StripOptions {
            inputs: vec![p.clone()],
            in_place: false,
            output_dir: String::new(),
            suffix: String::new(),
        };
        let res = strip(&opts, &crate::core::tools::noop_progress());
        let item = &res.items[0];
        assert!(item.ok, "falhou: {:?}", item.error);
        let clean = read(item.output.as_ref().unwrap());
        assert!(!clean.has_exif, "the EXIF survived the cleanup");
        assert!(clean.gps.is_none());
        assert!(item.bytes_after < item.bytes_before);
    }

    #[test]
    fn unknown_container_is_refused() {
        assert!(strip_bytes(b"not an image at all").is_err());
    }
}
