use super::types::{compute_mark_points, mm_to_mil_10, BoardBounds, MaskPaths, SilkscreenImage};
use xmlwriter::{Indent, Options, XmlWriter};

const SVG_HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#;

pub(crate) fn build_bottom_svg(
    bounds: &BoardBounds,
    image: &SilkscreenImage,
    mask_paths: &MaskPaths,
) -> String {
    const CLIP_MARGIN: f64 = 0.8374; // shrink (10 mil units)
    const EXPAND: f64 = 0.5; // expand (10 mil units)

    let min_x = mm_to_mil_10(bounds.min_x);
    let max_x = mm_to_mil_10(bounds.max_x);
    // Y axis inverted
    let min_y = -mm_to_mil_10(bounds.max_y);
    let max_y = -mm_to_mil_10(bounds.min_y);
    let w = mm_to_mil_10(bounds.width());
    let h = mm_to_mil_10(bounds.height());
    let center_x = min_x + w / 2.0;
    let image_w = image.width;
    let image_h = image.height;

    let mark_points = compute_mark_points(bounds)
        .iter()
        .flat_map(|(x, y)| [mm_to_mil_10(*x).to_string(), mm_to_mil_10(*y).to_string()])
        .collect::<Vec<_>>()
        .join(" ");

    let mut writer = create_writer();

    writer.start_element("svg");
    writer.write_attribute("width", &format!("{}mm", bounds.width()));
    writer.write_attribute("height", &format!("{}mm", bounds.height()));
    writer.write_attribute("boardBox", &format!("{min_x} {min_y} {w} {h}"));
    writer.write_attribute("viewBox", &format!("{min_x} {min_y} {w} {h}"));
    writer.write_attribute("version", "1.1");
    writer.write_attribute("eda-version", "1.6(2025-08-27)");
    writer.write_attribute("mark-points", &mark_points);
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

    // clipPath0 shrink
    writer.start_element("defs");
    writer.start_element("clipPath");
    writer.write_attribute("id", "clipPath0");
    writer.start_element("path");
    writer.write_attribute(
        "d",
        &format!(
            "M {} {} L {} {} {} {} {} {} {} {} ",
            max_x - CLIP_MARGIN,
            max_y - CLIP_MARGIN,
            min_x + CLIP_MARGIN,
            max_y - CLIP_MARGIN,
            min_x + CLIP_MARGIN,
            min_y + CLIP_MARGIN,
            max_x - CLIP_MARGIN,
            min_y + CLIP_MARGIN,
            max_x - CLIP_MARGIN,
            max_y - CLIP_MARGIN
        ),
    );
    writer.write_attribute("id", "outline0");
    writer.write_attribute("stroke", "none");
    writer.write_attribute("style", "fill-opacity:1;fill-rule:nonzero;fill:block;");
    writer.end_element();
    writer.end_element();
    writer.end_element();

    // clipPath1 expand + mask openings (evenodd to carve holes)
    writer.start_element("defs");
    writer.start_element("clipPath");
    writer.write_attribute("id", "clipPath1");
    writer.write_attribute("clip-path", "url(#clipPath0)");
    writer.start_element("path");
    let mut d_path = format!(
        "M {} {} L {} {} {} {} {} {} {} {} ",
        max_x + EXPAND,
        min_y - EXPAND,
        max_x + EXPAND,
        max_y + EXPAND,
        min_x - EXPAND,
        max_y + EXPAND,
        min_x - EXPAND,
        min_y - EXPAND,
        max_x + EXPAND,
        min_y - EXPAND
    );
    for mp in mask_paths {
        d_path.push_str(mp);
    }
    writer.write_attribute("d", &d_path);
    writer.write_attribute("id", "solder1");
    writer.write_attribute("stroke", "none");
    writer.write_attribute(
        "style",
        "fill-opacity:1;fill-rule:evenodd;clip-rule:evenodd;fill:block;",
    );
    writer.end_element();
    writer.end_element();
    writer.end_element();

    // main group (mirrored)
    writer.start_element("g");
    writer.write_attribute("clip-path", "url(#clipPath1)");
    writer.write_attribute(
        "transform",
        &format!("scale(-1 1) translate({} 0)", -2.0 * center_x),
    );

    writer.start_element("path");
    writer.write_attribute(
        "d",
        &format!(
            "M {} {} L {} {} {} {} {} {} {} {} ",
            min_x - EXPAND,
            max_y + EXPAND,
            min_x - EXPAND,
            min_y - EXPAND,
            max_x + EXPAND,
            min_y - EXPAND,
            max_x + EXPAND,
            max_y + EXPAND,
            min_x - EXPAND,
            max_y + EXPAND
        ),
    );
    writer.write_attribute("fill", "#FFFFFF");
    writer.write_attribute("stroke", "none");
    writer.write_attribute("stroke-width", "0");
    writer.write_attribute("id", "background");
    writer.end_element();

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
            max_x,
            min_y
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

pub(crate) fn build_top_svg(
    bounds: &BoardBounds,
    image: &SilkscreenImage,
    mask_paths: &MaskPaths,
) -> String {
    const CLIP_MARGIN: f64 = 0.8374; // in 10-mil units
    const EXPAND: f64 = 0.5; // in 10-mil units

    // Convert mm to 10-mil units (as used by sample SVGs)
    let min_x = mm_to_mil_10(bounds.min_x);
    let max_x = mm_to_mil_10(bounds.max_x);
    let min_y = -mm_to_mil_10(bounds.max_y);
    let max_y = -mm_to_mil_10(bounds.min_y);
    let w = mm_to_mil_10(bounds.width());
    let h = mm_to_mil_10(bounds.height());
    let image_w = image.width;
    let image_h = image.height;

    let mark_points = compute_mark_points(bounds)
        .iter()
        .flat_map(|(x, y)| [mm_to_mil_10(*x).to_string(), mm_to_mil_10(*y).to_string()])
        .collect::<Vec<_>>()
        .join(" ");

    let mut writer = create_writer();

    writer.start_element("svg");
    writer.write_attribute("width", &format!("{}mm", bounds.width()));
    writer.write_attribute("height", &format!("{}mm", bounds.height()));
    writer.write_attribute("boardBox", &format!("{min_x} {min_y} {w} {h}"));
    writer.write_attribute("viewBox", &format!("{min_x} {min_y} {w} {h}"));
    writer.write_attribute("version", "1.1");
    writer.write_attribute("eda-version", "1.6(2025-08-27)");
    writer.write_attribute("mark-points", &mark_points);
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

    writer.start_element("defs");
    writer.start_element("clipPath");
    writer.write_attribute("id", "clipPath0");
    writer.start_element("path");
    writer.write_attribute(
        "d",
        &format!(
            "M {} {} L {} {} {} {} {} {} {} {} ",
            max_x - CLIP_MARGIN,
            max_y - CLIP_MARGIN,
            min_x + CLIP_MARGIN,
            max_y - CLIP_MARGIN,
            min_x + CLIP_MARGIN,
            min_y + CLIP_MARGIN,
            max_x - CLIP_MARGIN,
            min_y + CLIP_MARGIN,
            max_x - CLIP_MARGIN,
            max_y - CLIP_MARGIN
        ),
    );
    writer.write_attribute("id", "outline0");
    writer.write_attribute("stroke", "none");
    writer.write_attribute("style", "fill-opacity:1;fill-rule:nonzero;fill:block;");
    writer.end_element(); // path
    writer.end_element(); // clipPath
    writer.end_element(); // defs

    writer.start_element("defs");
    writer.start_element("clipPath");
    writer.write_attribute("id", "clipPath1");
    writer.write_attribute("clip-path", "url(#clipPath0)");
    writer.start_element("path");
    let mut d_path = format!(
        "M {} {} L {} {} {} {} {} {} {} {} ",
        max_x + EXPAND,
        min_y - EXPAND,
        max_x + EXPAND,
        max_y + EXPAND,
        min_x - EXPAND,
        max_y + EXPAND,
        min_x - EXPAND,
        min_y - EXPAND,
        max_x + EXPAND,
        min_y - EXPAND
    );
    for mp in mask_paths {
        d_path.push_str(mp);
    }
    writer.write_attribute("d", &d_path);
    writer.write_attribute("id", "solder1");
    writer.write_attribute("stroke", "none");
    writer.write_attribute(
        "style",
        "fill-opacity:1;fill-rule:evenodd;clip-rule:evenodd;fill:block;",
    );
    writer.end_element(); // path
    writer.end_element(); // clipPath
    writer.end_element(); // defs

    writer.start_element("g");
    writer.write_attribute("clip-path", "url(#clipPath1)");
    writer.write_attribute("transform", "scale(1 1) translate(0 0)");

    writer.start_element("path");
    writer.write_attribute(
        "d",
        &format!(
            "M {} {} L {} {} {} {} {} {} {} {} ",
            min_x - EXPAND,
            max_y + EXPAND,
            min_x - EXPAND,
            min_y - EXPAND,
            max_x + EXPAND,
            min_y - EXPAND,
            max_x + EXPAND,
            max_y + EXPAND,
            min_x - EXPAND,
            max_y + EXPAND
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
            min_x,
            min_y
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

pub(crate) fn build_board_outline_svg(bounds: &BoardBounds) -> String {
    let (origin_x, origin_y) = bounds.origin();
    let ox = mm_to_mil_10(origin_x);
    let oy = mm_to_mil_10(origin_y);
    let w = mm_to_mil_10(bounds.width());
    let h = mm_to_mil_10(bounds.height());

    let mut writer = create_writer();
    writer.start_element("svg");
    writer.write_attribute("width", &format!("{}mm", bounds.width()));
    writer.write_attribute("height", &format!("{}mm", bounds.height()));
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

pub(crate) fn build_outline_mark_gerber(bounds: &BoardBounds, marks: &[(f64, f64)]) -> String {
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

fn create_writer() -> XmlWriter {
    XmlWriter::new(Options {
        use_single_quote: false,
        indent: Indent::Spaces(2),
        attributes_indent: Indent::Spaces(2),
    })
}
