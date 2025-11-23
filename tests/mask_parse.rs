use std::fs;
use std::path::Path;

use TransJLC::colorful::parse_solder_mask;

#[test]
fn parse_sample_solder_mask_generates_paths() {
    let path = Path::new("tests/data/mask/Gerber_TopSolderMaskLayer.GTS");
    let txt = fs::read_to_string(path).expect("read sample mask");
    let paths = parse_solder_mask(&txt).expect("parse mask");
    assert!(!paths.is_empty(), "expected parsed solder mask paths");
}
