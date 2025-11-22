//! Colorful silkscreen generation (FuckJLCColorfulSilkscreen port)
//!
//! Generates encrypted colorful silkscreen files for top/bottom layers
//! based on board outline and user-specified images.

use crate::patterns::LayerType;
use aes_gcm::aead::generic_array::{typenum::U16, GenericArray};
use aes_gcm::{
    aead::{Aead, KeyInit},
    aes::Aes128,
    AesGcm,
};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use image::ImageReader;
use rand::{rngs::OsRng, RngCore};
use regex::Regex;
use rsa::{pkcs8::DecodePublicKey, Oaep, RsaPublicKey};
use sha2::Sha256;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use xmlwriter::{Indent, Options, XmlWriter};

const RSA_PUB_KEY: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAzPtuUqJecaR/wWtctGT8
QuVslmDH3Ut3s8c1Ls4A+M9rwpeLjgDUqfcrSrTHBrl5k/dOeJEWMeNF7STWS5jo
WZE0H60cvf2bhormC9S6CRwq4Lw0ua0YQMo66R/qCtLVa5w6WkaPCz4b0xaHWtej
JH49C0T67rU2DkepXuMPpwNCflMU+WgEQioZEldUTD6gYpu2U5GrW4AE0AQiIo+j
e7tgN8PlBMbMaEfu0LokZyth1ugfuLAgyogWnedAegQmPZzAUe36Sni94AsDlhxm
mjFl+WQZzD3MclbEY6KQB5XL8zCR/J6pCUUwfHantLxY/gQi0XJG5hWWtDyH/fR2
lwIDAQAB
-----END PUBLIC KEY-----"#;

const SVG_HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#;

/// Inputs for colorful silkscreen generation
#[derive(Debug, Clone)]
pub struct ColorfulOptions {
    pub top_image: Option<PathBuf>,
    pub bottom_image: Option<PathBuf>,
}

/// Generate colorful silkscreen encrypted outputs
pub struct ColorfulSilkscreenGenerator {
    options: ColorfulOptions,
}

impl ColorfulSilkscreenGenerator {
    pub fn new(options: ColorfulOptions) -> Self {
        Self { options }
    }

    /// Generate colorful silkscreen files in the given output directory.
    /// Returns the written file paths with their logical layer types.
    pub fn generate(
        &self,
        outline_path: &Path,
        output_dir: &Path,
    ) -> Result<Vec<(LayerType, PathBuf)>> {
        if self.options.top_image.is_none() && self.options.bottom_image.is_none() {
            return Ok(Vec::new());
        }

        let outline_content = fs::read_to_string(outline_path)
            .with_context(|| format!("Read outline {}", outline_path.display()))?;
        let bounds = parse_outline_bounds(&outline_content)?;
        let mark_points = compute_mark_points(&bounds);

        fs::create_dir_all(output_dir)
            .with_context(|| format!("Create output dir {}", output_dir.display()))?;

        let key_material = KeyMaterial::generate()?;
        let mut written: Vec<(LayerType, PathBuf)> = Vec::new();

        if let Some(top_path) = &self.options.top_image {
            let image = load_image(top_path)?;
            let svg = build_top_svg(&bounds, &image);
            let target = output_dir.join("Fabrication_ColorfulTopSilkscreen.FCTS");
            encrypt_and_write(&svg, &key_material, &target)?;
            written.push((LayerType::ColorfulTopSilkscreen, target));
        }

        if let Some(bottom_path) = &self.options.bottom_image {
            let image = load_image(bottom_path)?;
            let svg = build_bottom_svg(&bounds, &image);
            let target = output_dir.join("Fabrication_ColorfulBottomSilkscreen.FCBS");
            encrypt_and_write(&svg, &key_material, &target)?;
            written.push((LayerType::ColorfulBottomSilkscreen, target));
        }

        // Colorful board outline layer (encrypted SVG)
        let outline_svg = build_board_outline_svg(&bounds);
        let outline_target = output_dir.join("Fabrication_ColorfulBoardOutlineLayer.FCBO");
        encrypt_and_write(&outline_svg, &key_material, &outline_target)?;
        written.push((LayerType::ColorfulBoardOutline, outline_target));

        // Colorful board outline mark layer (plain Gerber)
        let mark_gerber = build_outline_mark_gerber(&bounds, &mark_points);
        let mark_target = output_dir.join("Fabrication_ColorfulBoardOutlineMark.FCBM");
        fs::write(&mark_target, mark_gerber)
            .with_context(|| format!("Write {}", mark_target.display()))?;
        written.push((LayerType::ColorfulBoardOutlineMark, mark_target));

        Ok(written)
    }
}

/// Board outline bounds expressed in millimeters.
#[derive(Debug, Clone, Copy)]
struct BoardBounds {
    origin_x: f64,
    origin_y: f64,
    width: f64,
    height: f64,
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

#[derive(Debug, Clone, Copy)]
enum Units {
    Millimeter,
    Inch,
}

fn parse_outline_bounds(content: &str) -> Result<BoardBounds> {
    let units = detect_units(content);
    let (x_decimals, y_decimals) = detect_format_decimals(content);

    let re_x = Regex::new(r"X([+-]?\d+(?:\.\d+)?)")?;
    let re_y = Regex::new(r"Y([+-]?\d+(?:\.\d+)?)")?;

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    let mut last_x: Option<f64> = None;
    let mut last_y: Option<f64> = None;

    for line in content.lines() {
        if let Some(caps) = re_x.captures(line) {
            if let Some(val) = parse_coord(&caps[1], x_decimals, units) {
                last_x = Some(val);
            }
        }

        if let Some(caps) = re_y.captures(line) {
            if let Some(val) = parse_coord(&caps[1], y_decimals, units) {
                last_y = Some(val);
            }
        }

        if let (Some(x), Some(y)) = (last_x, last_y) {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }

    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        bail!("Failed to parse board outline bounds");
    }

    let width = max_x - min_x;
    let height = max_y - min_y;

    Ok(BoardBounds {
        origin_x: min_x,
        origin_y: -max_y, // align with original script convention (Y flipped)
        width,
        height,
        min_x,
        max_x,
        min_y,
        max_y,
    })
}

fn detect_units(content: &str) -> Units {
    for line in content.lines() {
        let l = line.trim().to_uppercase();
        if l.contains("%MOIN") {
            return Units::Inch;
        }
        if l.contains("%MOMM") {
            return Units::Millimeter;
        }
    }
    Units::Millimeter
}

fn detect_format_decimals(content: &str) -> (usize, usize) {
    let re = Regex::new(r"(?i)%FSLAX(\d)(\d)Y(\d)(\d)\*%").ok();
    if let Some(re) = re {
        for line in content.lines() {
            if let Some(caps) = re.captures(line) {
                let x_dec = caps
                    .get(2)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(5);
                let y_dec = caps
                    .get(4)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(5);
                return (x_dec, y_dec);
            }
        }
    }
    (5, 5)
}

fn parse_coord(raw: &str, decimals: usize, units: Units) -> Option<f64> {
    let sign = if raw.starts_with('-') { -1.0 } else { 1.0 };
    let trimmed = raw.trim_start_matches(['+', '-']);

    let value = if trimmed.contains('.') {
        trimmed.parse::<f64>().ok()?
    } else {
        let scale = 10f64.powi(decimals as i32);
        trimmed.parse::<f64>().ok()? / scale
    };

    let mm_value = match units {
        Units::Millimeter => value,
        Units::Inch => value * 25.4,
    };

    Some(sign * mm_value)
}

/// Loaded image metadata and base64 data URI.
struct SilkscreenImage {
    width: u32,
    height: u32,
    data_uri: String,
}

fn load_image(path: &Path) -> Result<SilkscreenImage> {
    let bytes = fs::read(path).with_context(|| format!("Read image {}", path.display()))?;
    let reader = ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .context("Guess image format")?;
    let (width, height) = reader.into_dimensions().context("Read image dimensions")?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "png".to_string());
    let encoded = general_purpose::STANDARD.encode(bytes);
    let data_uri = format!("data:image/{};base64,{}", ext, encoded);

    Ok(SilkscreenImage {
        width,
        height,
        data_uri,
    })
}

fn mm_to_mil_10(val: f64) -> f64 {
    val / 0.254
}

fn compute_mark_points(bounds: &BoardBounds) -> Vec<(f64, f64)> {
    // Inset by approximately 3 mil to mirror the original script behaviour.
    const INSET_MM: f64 = 0.0762;
    let min_x = bounds.min_x + INSET_MM;
    let max_x = bounds.max_x - INSET_MM;
    let min_y = bounds.min_y + INSET_MM;
    let max_y = bounds.max_y - INSET_MM;

    vec![(min_x, min_y), (min_x, max_y), (max_x, max_y)]
}

fn build_top_svg(bounds: &BoardBounds, image: &SilkscreenImage) -> String {
    let ox = mm_to_mil_10(bounds.origin_x);
    let oy = mm_to_mil_10(bounds.origin_y);
    let w = mm_to_mil_10(bounds.width);
    let h = mm_to_mil_10(bounds.height);
    let image_w = image.width;
    let image_h = image.height;

    let mut writer = create_writer();

    writer.start_element("svg");
    writer.write_attribute("width", &format!("{}mm", bounds.width));
    writer.write_attribute("height", &format!("{}mm", bounds.height));
    writer.write_attribute("boardBox", &format!("{ox} {oy} {w} {h}"));
    writer.write_attribute("viewBox", &format!("{ox} {oy} {w} {h}"));
    writer.write_attribute("version", "1.1");
    writer.write_attribute(
        "xmlns:inkscape",
        "http://www.inkscape.org/namespaces/inkscape",
    );
    writer.write_attribute(
        "xmlns:sodipodi",
        "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd",
    );
    writer.write_attribute("xmlns:xlink", "http://www.w3.org/1999/xlink");
    writer.write_attribute("xmlns", "http://www.w3.org/2000/svg");
    writer.write_attribute("xmlns:svg", "http://www.w3.org/2000/svg");

    // defs/clipPath 0
    writer.start_element("defs");
    writer.start_element("clipPath");
    writer.write_attribute("id", "clipPath0");
    writer.start_element("path");
    writer.write_attribute(
        "d",
        &format!(
            "M {} {} L {} {} {} {} {} {} {} {} ",
            ox + 0.05,
            oy + h - 0.05,
            ox + 0.05,
            oy + 0.05,
            ox + w - 0.05,
            oy + 0.05,
            ox + w - 0.05,
            oy + h - 0.05,
            ox + 0.05,
            oy + h - 0.05
        ),
    );
    writer.write_attribute("id", "outline0");
    writer.write_attribute("stroke", "none");
    writer.write_attribute("style", "fill-opacity:1;fill-rule:nonzero;fill:block;");
    writer.end_element(); // path
    writer.end_element(); // clipPath
    writer.end_element(); // defs

    // defs/clipPath 1
    writer.start_element("defs");
    writer.start_element("clipPath");
    writer.write_attribute("id", "clipPath1");
    writer.write_attribute("clip-path", "url(#clipPath0)");
    writer.start_element("path");
    writer.write_attribute(
        "d",
        &format!(
            "M {} {} L {} {} {} {} {} {} {} {} ",
            ox - 0.05,
            oy + h + 0.05,
            ox - 0.05,
            oy - 0.05,
            ox + w + 0.05,
            oy - 0.05,
            ox + w + 0.05,
            oy + h + 0.05,
            ox - 0.05,
            oy + h + 0.05
        ),
    );
    writer.write_attribute("id", "solder1");
    writer.write_attribute("stroke", "none");
    writer.write_attribute("style", "fill-opacity:1;fill-rule:nonzero;fill:block;");
    writer.end_element(); // path
    writer.end_element(); // clipPath
    writer.end_element(); // defs

    // main group
    writer.start_element("g");
    writer.write_attribute("clip-path", "url(#clipPath1)");
    writer.write_attribute("transform", "scale(1 1) translate(0 0)");

    writer.start_element("path");
    writer.write_attribute(
        "d",
        &format!(
            "M {} {} L {} {} {} {} {} {} {} {} ",
            ox - 0.05,
            oy + h + 0.05,
            ox + w + 0.05,
            oy + h + 0.05,
            ox + w + 0.05,
            oy - 0.05,
            ox - 0.05,
            oy - 0.05,
            ox + w + 0.05,
            oy + h + 0.05
        ),
    );
    writer.write_attribute("fill", "#FFFFFF");
    writer.write_attribute("stroke", "none");
    writer.write_attribute("stroke-width", "0");
    writer.write_attribute("id", "background");
    writer.end_element(); // path

    writer.start_element("image");
    writer.write_attribute("width", &image_w.to_string());
    writer.write_attribute("height", &image_h.to_string());
    writer.write_attribute("preserveAspectRatio", "none");
    writer.write_attribute("xlink:href", &image.data_uri);
    writer.write_attribute(
        "transform",
        &format!(
            "matrix({} 0 0 {} {} {})",
            w / image_w as f64,
            h / image_h as f64,
            ox,
            oy
        ),
    );
    writer.end_element(); // image

    writer.end_element(); // g
    writer.end_element(); // svg

    let mut result = SVG_HEADER.to_string();
    result.push('\n');
    result.push_str(&writer.end_document());
    result
}

fn build_board_outline_svg(bounds: &BoardBounds) -> String {
    let ox = mm_to_mil_10(bounds.origin_x);
    let oy = mm_to_mil_10(bounds.origin_y);
    let w = mm_to_mil_10(bounds.width);
    let h = mm_to_mil_10(bounds.height);

    let mut writer = create_writer();
    writer.start_element("svg");
    writer.write_attribute("width", &format!("{}mm", bounds.width));
    writer.write_attribute("height", &format!("{}mm", bounds.height));
    writer.write_attribute("viewBox", &format!("{ox} {oy} {w} {h}"));
    writer.write_attribute("version", "1.1");
    writer.write_attribute("xmlns", "http://www.w3.org/2000/svg");
    writer.write_attribute("xmlns:svg", "http://www.w3.org/2000/svg");

    writer.start_element("rect");
    writer.write_attribute("x", &ox.to_string());
    writer.write_attribute("y", &oy.to_string());
    writer.write_attribute("width", &w.to_string());
    writer.write_attribute("height", &h.to_string());
    writer.write_attribute("fill", "none");
    writer.write_attribute("stroke", "#00AA00");
    writer.write_attribute("stroke-width", "1");
    writer.end_element(); // rect

    writer.end_element(); // svg

    let mut result = SVG_HEADER.to_string();
    result.push('\n');
    result.push_str(&writer.end_document());
    result
}

fn build_outline_mark_gerber(bounds: &BoardBounds, marks: &[(f64, f64)]) -> String {
    // Use mm units with 2 integer, 5 decimal places.
    let mut out = String::new();
    out.push_str("G04 Fabrication_ColorfulBoardOutlineMark*\n");
    out.push_str("%FSLAX25Y25*%\n");
    out.push_str("%MOMM*%\n");
    out.push_str("%LPD*%\n");

    // Outline aperture and drawing (simple rectangle)
    out.push_str("%ADD10C,0.150*%\n");
    out.push_str("D10*\n");
    let start = format_coord(bounds.min_x, bounds.min_y);
    out.push_str(&format!("X{}Y{}D02*\n", start.0, start.1));
    let outline_pts = [
        (bounds.max_x, bounds.min_y),
        (bounds.max_x, bounds.max_y),
        (bounds.min_x, bounds.max_y),
        (bounds.min_x, bounds.min_y),
    ];
    for (x, y) in outline_pts {
        let c = format_coord(x, y);
        out.push_str(&format!("X{}Y{}D01*\n", c.0, c.1));
    }

    // Mark pads (flash)
    out.push_str("%ADD11C,1.000*%\n");
    out.push_str("D11*\n");
    for (x, y) in marks {
        let c = format_coord(*x, *y);
        out.push_str(&format!("X{}Y{}D03*\n", c.0, c.1));
    }

    out.push_str("M02*\n");
    out
}

fn format_coord(x_mm: f64, y_mm: f64) -> (String, String) {
    // 2 integer + 5 decimal -> scale by 1e5
    let scale = 100_000.0;
    let xi = (x_mm * scale).round() as i64;
    let yi = (y_mm * scale).round() as i64;
    (format!("{:+08}", xi), format!("{:+08}", yi))
}

fn build_bottom_svg(bounds: &BoardBounds, image: &SilkscreenImage) -> String {
    let ox = mm_to_mil_10(bounds.origin_x);
    let oy = mm_to_mil_10(bounds.origin_y);
    let w = mm_to_mil_10(bounds.width);
    let h = mm_to_mil_10(bounds.height);
    let image_w = image.width;
    let image_h = image.height;

    let mut writer = create_writer();

    writer.start_element("svg");
    writer.write_attribute("width", &format!("{}mm", bounds.width));
    writer.write_attribute("height", &format!("{}mm", bounds.height));
    writer.write_attribute("boardBox", &format!("{ox} {oy} {w} {h}"));
    writer.write_attribute("viewBox", &format!("{ox} {oy} {w} {h}"));
    writer.write_attribute("version", "1.1");
    writer.write_attribute(
        "xmlns:inkscape",
        "http://www.inkscape.org/namespaces/inkscape",
    );
    writer.write_attribute(
        "xmlns:sodipodi",
        "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd",
    );
    writer.write_attribute("xmlns:xlink", "http://www.w3.org/1999/xlink");
    writer.write_attribute("xmlns", "http://www.w3.org/2000/svg");
    writer.write_attribute("xmlns:svg", "http://www.w3.org/2000/svg");

    // defs/clipPath 0
    writer.start_element("defs");
    writer.start_element("clipPath");
    writer.write_attribute("id", "clipPath0");
    writer.start_element("path");
    writer.write_attribute(
        "d",
        &format!(
            "M {} {} L {} {} {} {} {} {} {} {} ",
            oy + h - 0.05,
            ox + 0.05,
            ox + w - 0.05,
            oy + h - 0.05,
            ox + 0.05,
            oy + h - 0.05,
            ox + 0.05,
            oy + 0.05,
            ox + w - 0.05,
            oy + 0.05
        ),
    );
    writer.write_attribute("id", "outline0");
    writer.write_attribute("stroke", "none");
    writer.write_attribute("style", "fill-opacity:1;fill-rule:nonzero;fill:block;");
    writer.end_element(); // path
    writer.end_element(); // clipPath
    writer.end_element(); // defs

    // defs/clipPath 1
    writer.start_element("defs");
    writer.start_element("clipPath");
    writer.write_attribute("id", "clipPath1");
    writer.write_attribute("clip-path", "url(#clipPath0)");
    writer.start_element("path");
    writer.write_attribute(
        "d",
        &format!(
            "M {} {} L {} {} {} {} {} {} {} {} ",
            ox - 0.05,
            oy + h + 0.05,
            ox - 0.05,
            oy - 0.05,
            ox + w - 0.05,
            oy - 0.05,
            ox + w - 0.05,
            oy + h + 0.05,
            ox - 0.05,
            oy + h + 0.05
        ),
    );
    writer.write_attribute("id", "solder1");
    writer.write_attribute("stroke", "none");
    writer.write_attribute("style", "fill-opacity:1;fill-rule:nonzero;fill:block");
    writer.end_element(); // path
    writer.end_element(); // clipPath
    writer.end_element(); // defs

    // main group
    writer.start_element("g");
    writer.write_attribute("clip-path", "url(#clipPath1)");
    writer.write_attribute(
        "transform",
        &format!("scale(-1 1) translate(-{} 0)", 2.0 * ox + w),
    );

    writer.start_element("path");
    writer.write_attribute(
        "d",
        &format!(
            "M {} {} L {} {} {} {} {} {} {} {} ",
            ox - 0.05,
            oy + h + 0.05,
            ox + w + 0.05,
            oy + h + 0.05,
            ox + w + 0.05,
            oy - 0.05,
            ox - 0.05,
            oy - 0.05,
            ox - 0.05,
            oy + h + 0.05
        ),
    );
    writer.write_attribute("fill", "#FFFFFF");
    writer.write_attribute("stroke", "none");
    writer.write_attribute("stroke-width", "0");
    writer.write_attribute("id", "background");
    writer.end_element(); // path

    writer.start_element("image");
    writer.write_attribute("width", &image_w.to_string());
    writer.write_attribute("height", &image_h.to_string());
    writer.write_attribute("preserveAspectRatio", "none");
    writer.write_attribute("xlink:href", &image.data_uri);
    writer.write_attribute(
        "transform",
        &format!(
            "matrix({} 0 0 {} {} {})",
            -(w / image_w as f64),
            h / image_h as f64,
            ox + w,
            oy
        ),
    );
    writer.end_element(); // image

    writer.end_element(); // g
    writer.end_element(); // svg

    let mut result = SVG_HEADER.to_string();
    result.push('\n');
    result.push_str(&writer.end_document());
    result
}

fn create_writer() -> XmlWriter {
    XmlWriter::new(Options {
        use_single_quote: false,
        indent: Indent::None,
        attributes_indent: Indent::None,
    })
}

struct KeyMaterial {
    aes_key: [u8; 16],
    aes_iv: [u8; 16],
    enc_key: Vec<u8>,
    enc_iv: Vec<u8>,
}

impl KeyMaterial {
    fn generate() -> Result<Self> {
        let mut aes_key = [0u8; 16];
        let mut aes_iv = [0u8; 16];
        OsRng.fill_bytes(&mut aes_key);
        OsRng.fill_bytes(&mut aes_iv);

        let public_key = RsaPublicKey::from_public_key_pem(RSA_PUB_KEY)
            .context("Invalid embedded RSA public key")?;
        let enc_key = public_key
            .encrypt(&mut OsRng, Oaep::new::<Sha256>(), &aes_key)
            .context("Encrypt AES key")?;
        let enc_iv = public_key
            .encrypt(&mut OsRng, Oaep::new::<Sha256>(), &aes_iv)
            .context("Encrypt AES IV")?;

        Ok(Self {
            aes_key,
            aes_iv,
            enc_key,
            enc_iv,
        })
    }
}

fn encrypt_and_write(svg: &str, key_material: &KeyMaterial, output: &Path) -> Result<()> {
    // AES-128-GCM with 16-byte nonce to mirror the original script (WebCrypto)
    type Aes128Gcm16 = AesGcm<Aes128, U16>;
    let cipher = Aes128Gcm16::new_from_slice(&key_material.aes_key).context("Create AES cipher")?;
    let nonce = GenericArray::<u8, U16>::from_slice(&key_material.aes_iv);
    let ciphertext = cipher
        .encrypt(nonce, svg.as_bytes())
        .map_err(|e| anyhow!("Encrypt silkscreen SVG: {:?}", e))?;

    let mut file = File::create(output).with_context(|| format!("Create {}", output.display()))?;
    file.write_all(&key_material.enc_key)
        .with_context(|| format!("Write AES key for {}", output.display()))?;
    file.write_all(&key_material.enc_iv)
        .with_context(|| format!("Write AES IV for {}", output.display()))?;
    file.write_all(&ciphertext)
        .with_context(|| format!("Write ciphertext for {}", output.display()))?;

    Ok(())
}
