//! Text as vector outlines, using the Segoe UI fonts that ship with Windows.
//!
//! Text is turned into a path, so it can be rotated, scaled and shadowed like any other shape.

use ab_glyph::{Font, FontVec, GlyphId, OutlineCurve};
use std::sync::OnceLock;
use tiny_skia::{Path, PathBuilder};

/// Horizontal anchor of the text relative to the x position (Canvas `textAlign`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    /// The text starts at x.
    Left,
    /// The text is centred on x.
    Center,
}

/// Vertical anchor of the text relative to the y position (Canvas `textBaseline`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextBaseline {
    /// y is the alphabetic baseline.
    Alphabetic,
    /// y is the middle of the em box.
    Middle,
}

/// Font size and weight (Canvas `font`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontSpec {
    /// Size in pixels (the em size).
    pub size: f32,
    /// CSS weight: 400 regular, 700 bold, 800 extra bold.
    pub weight: u16,
}

impl FontSpec {
    /// Creates a font description.
    ///
    /// @param weight CSS weight (400 regular, 600+ bold).
    /// @param size Size in pixels.
    /// @return The font description.
    pub fn new(weight: u16, size: f32) -> FontSpec {
        FontSpec { size, weight }
    }
}

impl Default for FontSpec {
    /// Canvas default: 10px regular.
    ///
    /// @return The default font.
    fn default() -> FontSpec {
        FontSpec { size: 10.0, weight: 400 }
    }
}

/// The loaded font files. Fonts that are missing on the system are simply skipped.
struct FontSet {
    regular: Option<FontVec>,
    bold: Option<FontVec>,
    black: Option<FontVec>,
    symbol: Option<FontVec>,
}

static FONTS: OnceLock<FontSet> = OnceLock::new();

/// Loads a font file from the Windows fonts folder.
///
/// @param file_name The file name, e.g. "segoeuib.ttf".
/// @return The font, or None when the file is missing or invalid.
fn load_system_font(file_name: &str) -> Option<FontVec> {
    let windows_dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let path = std::path::Path::new(&windows_dir).join("Fonts").join(file_name);
    let bytes = std::fs::read(path).ok()?;
    FontVec::try_from_vec(bytes).ok()
}

/// Returns the loaded fonts, loading them on first use.
///
/// @return The font set.
fn fonts() -> &'static FontSet {
    FONTS.get_or_init(|| FontSet {
        regular: load_system_font("segoeui.ttf"),
        bold: load_system_font("segoeuib.ttf"),
        black: load_system_font("seguibl.ttf"),
        symbol: load_system_font("seguisym.ttf"),
    })
}

/// Returns the fonts to try for a weight, best match first.
///
/// @param weight CSS weight.
/// @return Fonts to try in order.
fn fonts_for_weight(weight: u16) -> Vec<&'static FontVec> {
    let set = fonts();
    let ordered = if weight >= 800 {
        [&set.bold, &set.black, &set.regular, &set.symbol]
    } else if weight >= 600 {
        [&set.bold, &set.regular, &set.black, &set.symbol]
    } else {
        [&set.regular, &set.bold, &set.black, &set.symbol]
    };
    ordered.into_iter().filter_map(|font| font.as_ref()).collect()
}

/// Finds the first font that has a glyph for a character.
///
/// @param candidates Fonts to try in order.
/// @param character The character.
/// @return The font and glyph id, or None when no font has the character.
fn find_glyph(candidates: &[&'static FontVec], character: char) -> Option<(&'static FontVec, GlyphId)> {
    for font in candidates {
        let id = font.glyph_id(character);
        if id.0 != 0 {
            return Some((font, id));
        }
    }
    None
}

/// Returns the scale from font units to pixels for a font.
///
/// @param font The font.
/// @param size Font size in pixels.
/// @return Pixels per font unit.
fn units_to_pixels(font: &FontVec, size: f32) -> f32 {
    let units_per_em = font.units_per_em().unwrap_or(2048.0);
    size / units_per_em
}

/// Measures the width of a text (Canvas `measureText().width`).
///
/// @param text The text.
/// @param font Size and weight.
/// @return The advance width in pixels.
pub fn measure_text(text: &str, font: FontSpec) -> f32 {
    let candidates = fonts_for_weight(font.weight);
    let mut width = 0.0;
    for character in text.chars() {
        if let Some((glyph_font, id)) = find_glyph(&candidates, character) {
            width += glyph_font.h_advance_unscaled(id) * units_to_pixels(glyph_font, font.size);
        } else {
            width += font.size * 0.5;
        }
    }
    width
}

/// Builds the outline path of a text, in pixels, positioned like Canvas `fillText(text, x, y)`.
///
/// @param text The text.
/// @param x Anchor x.
/// @param y Anchor y.
/// @param font Size and weight.
/// @param align Horizontal anchor.
/// @param baseline Vertical anchor.
/// @return The text outline, or None for empty or blank text.
pub fn text_path(
    text: &str,
    x: f32,
    y: f32,
    font: FontSpec,
    align: TextAlign,
    baseline: TextBaseline,
) -> Option<Path> {
    let candidates = fonts_for_weight(font.weight);
    let primary = *candidates.first()?;
    let primary_scale = units_to_pixels(primary, font.size);
    let ascent = primary.ascent_unscaled() * primary_scale;
    let descent = primary.descent_unscaled() * primary_scale;

    let total_width = measure_text(text, font);
    let start_x = match align {
        TextAlign::Left => x,
        TextAlign::Center => x - total_width / 2.0,
    };
    // Canvas positions "middle" relative to the em box.
    let em_top = ascent - (ascent - descent - font.size) / 2.0;
    let baseline_y = match baseline {
        TextBaseline::Alphabetic => y,
        TextBaseline::Middle => y + em_top - font.size / 2.0,
    };

    let mut builder = PathBuilder::new();
    let mut pen_x = start_x;
    for character in text.chars() {
        let Some((glyph_font, id)) = find_glyph(&candidates, character) else {
            pen_x += font.size * 0.5;
            continue;
        };
        let scale = units_to_pixels(glyph_font, font.size);
        if let Some(outline) = glyph_font.outline(id) {
            append_outline(&mut builder, &outline.curves, pen_x, baseline_y, scale);
        }
        pen_x += glyph_font.h_advance_unscaled(id) * scale;
    }
    builder.finish()
}

/// Adds one glyph outline to a path. Font units have y pointing up; the path has y pointing down.
///
/// @param builder The path being built.
/// @param curves The glyph's outline curves.
/// @param origin_x Pen x in pixels.
/// @param baseline_y Baseline y in pixels.
/// @param scale Pixels per font unit.
fn append_outline(builder: &mut PathBuilder, curves: &[OutlineCurve], origin_x: f32, baseline_y: f32, scale: f32) {
    let to_x = |value: f32| origin_x + value * scale;
    let to_y = |value: f32| baseline_y - value * scale;
    let mut last_end: Option<(f32, f32)> = None;
    for curve in curves {
        let start = match curve {
            OutlineCurve::Line(p0, _) => *p0,
            OutlineCurve::Quad(p0, _, _) => *p0,
            OutlineCurve::Cubic(p0, _, _, _) => *p0,
        };
        let start_point = (to_x(start.x), to_y(start.y));
        let continues = match last_end {
            Some(end) => (end.0 - start_point.0).abs() < 0.001 && (end.1 - start_point.1).abs() < 0.001,
            None => false,
        };
        if !continues {
            if last_end.is_some() {
                builder.close();
            }
            builder.move_to(start_point.0, start_point.1);
        }
        match curve {
            OutlineCurve::Line(_, p1) => {
                builder.line_to(to_x(p1.x), to_y(p1.y));
                last_end = Some((to_x(p1.x), to_y(p1.y)));
            }
            OutlineCurve::Quad(_, p1, p2) => {
                builder.quad_to(to_x(p1.x), to_y(p1.y), to_x(p2.x), to_y(p2.y));
                last_end = Some((to_x(p2.x), to_y(p2.y)));
            }
            OutlineCurve::Cubic(_, p1, p2, p3) => {
                builder.cubic_to(to_x(p1.x), to_y(p1.y), to_x(p2.x), to_y(p2.y), to_x(p3.x), to_y(p3.y));
                last_end = Some((to_x(p3.x), to_y(p3.y)));
            }
        }
    }
    if last_end.is_some() {
        builder.close();
    }
}
