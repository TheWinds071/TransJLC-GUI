//! Colorful silkscreen generation (FuckJLCColorfulSilkscreen port)
//!
//! Generates encrypted colorful silkscreen files for top/bottom layers
//! based on board outline, user-specified images, and solder mask openings.

use crate::patterns::LayerType;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

mod encrypt;
pub mod mask;
mod svg;
mod types;

pub use mask::parse_solder_mask;
use types::{compute_mark_points, load_image, MaskPaths};

const RSA_PUB_KEY: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAzPtuUqJecaR/wWtctGT8
QuVslmDH3Ut3s8c1Ls4A+M9rwpeLjgDUqfcrSrTHBrl5k/dOeJEWMeNF7STWS5jo
WZE0H60cvf2bhormC9S6CRwq4Lw0ua0YQMo66R/qCtLVa5w6WkaPCz4b0xaHWtej
JH49C0T67rU2DkepXuMPpwNCflMU+WgEQioZEldUTD6gYpu2U5GrW4AE0AQiIo+j
e7tgN8PlBMbMaEfu0LokZyth1ugfuLAgyogWnedAegQmPZzAUe36Sni94AsDlhxm
mjFl+WQZzD3MclbEY6KQB5XL8zCR/J6pCUUwfHantLxY/gQi0XJG5hWWtDyH/fR2
lwIDAQAB
-----END PUBLIC KEY-----"#;

/// Inputs for colorful silkscreen generation
#[derive(Debug, Clone)]
pub struct ColorfulOptions {
    pub top_image: Option<PathBuf>,
    pub bottom_image: Option<PathBuf>,
    pub top_solder_mask: Option<PathBuf>,
    pub bottom_solder_mask: Option<PathBuf>,
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
        let bounds = types::parse_outline_bounds(&outline_content)?;
        let mark_points = compute_mark_points(&bounds);

        fs::create_dir_all(output_dir)
            .with_context(|| format!("Create output dir {}", output_dir.display()))?;

        let key_material = encrypt::KeyMaterial::generate(RSA_PUB_KEY)?;
        let mut written: Vec<(LayerType, PathBuf)> = Vec::new();

        if let Some(top_path) = &self.options.top_image {
            let image = load_image(top_path)?;
            let mask = load_mask_paths(self.options.top_solder_mask.as_deref())?;
            let svg = svg::build_top_svg(&bounds, &image, &mask);
            let target = output_dir.join("Fabrication_ColorfulTopSilkscreen.FCTS");
            encrypt::encrypt_and_write(&svg, &key_material, &target)?;
            written.push((LayerType::ColorfulTopSilkscreen, target));
        }

        if let Some(bottom_path) = &self.options.bottom_image {
            let image = load_image(bottom_path)?;
            let mask = load_mask_paths(self.options.bottom_solder_mask.as_deref())?;
            let svg = svg::build_bottom_svg(&bounds, &image, &mask);
            let target = output_dir.join("Fabrication_ColorfulBottomSilkscreen.FCBS");
            encrypt::encrypt_and_write(&svg, &key_material, &target)?;
            written.push((LayerType::ColorfulBottomSilkscreen, target));
        }

        // Colorful board outline layer (encrypted SVG)
        let outline_svg = svg::build_board_outline_svg(&bounds);
        let outline_target = output_dir.join("Fabrication_ColorfulBoardOutlineLayer.FCBO");
        encrypt::encrypt_and_write(&outline_svg, &key_material, &outline_target)?;
        written.push((LayerType::ColorfulBoardOutline, outline_target));

        // Colorful board outline mark layer (plain Gerber)
        let mark_gerber = svg::build_outline_mark_gerber(&bounds, &mark_points);
        let mark_target = output_dir.join("Fabrication_ColorfulBoardOutlineMark.FCBM");
        fs::write(&mark_target, mark_gerber)
            .with_context(|| format!("Write {}", mark_target.display()))?;
        written.push((LayerType::ColorfulBoardOutlineMark, mark_target));

        Ok(written)
    }
}

fn load_mask_paths(path: Option<&Path>) -> Result<MaskPaths> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let content =
        fs::read_to_string(path).with_context(|| format!("Read solder mask {}", path.display()))?;
    mask::parse_solder_mask(&content)
}
