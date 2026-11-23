//! Turns a cat skin configuration (from `cats.json` or MCP) into a complete set of drawing colours.

use crate::cats::{CatSkinConfig, CatSpot};

/// Coat pattern of a custom cat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatPattern {
    /// One colour all over.
    Solid,
    /// Darker face mask and lighter body.
    Bicolor,
    /// Black-and-white "tuxedo" with a white bib and paws.
    Tuxedo,
    /// Striped tabby with an "M" on the forehead.
    Tabby,
    /// Patches of two colours on a light base.
    Calico,
    /// Light body with dark face, ears, paws and tail.
    Siamese,
}

impl CatPattern {
    /// Reads a pattern name. Unknown names become `Solid`.
    ///
    /// @param name "solid", "bicolor", "tuxedo", "tabby", "calico" or "siamese".
    /// @return The pattern.
    pub fn from_name(name: &str) -> CatPattern {
        match name {
            "bicolor" => CatPattern::Bicolor,
            "tuxedo" => CatPattern::Tuxedo,
            "tabby" => CatPattern::Tabby,
            "calico" => CatPattern::Calico,
            "siamese" => CatPattern::Siamese,
            _ => CatPattern::Solid,
        }
    }
}

/// All colours and options needed to draw a custom cat. Colours are CSS colour strings.
#[derive(Clone, Debug)]
pub struct ResolvedSkin {
    pub is_dark_base: bool,
    pub fur_color: String,
    pub fur_secondary_color: String,
    pub outline_color: Option<String>,
    pub leg_line_color: String,
    pub pattern: CatPattern,
    pub mask_color: Option<String>,
    pub forehead_bridge_color: Option<String>,
    pub flank_color: Option<String>,
    pub belly_bib_color: Option<String>,
    pub stripe_color: Option<String>,
    pub calico_patch_color: Option<String>,
    pub ear_outer_color: String,
    pub ear_tip_color: Option<String>,
    pub ear_inner_color: Option<String>,
    pub ear_tuft_color: Option<String>,
    pub tail_color: String,
    pub tail_tip_color: String,
    pub tail_fluffy: bool,
    pub tail_stripes: bool,
    pub eye_color_top: String,
    pub eye_color_bottom: String,
    pub pupil_color: String,
    pub eye_catchlight: bool,
    pub eye_rim_color: Option<String>,
    pub squint_color: String,
    pub squint_glow_color: Option<String>,
    pub nose_color: Option<String>,
    pub mouth_color: Option<String>,
    pub blush_color: Option<String>,
    pub whisker_color: String,
    pub arm_color: String,
    pub paw_color: String,
    pub paw_outline_color: Option<String>,
    pub arm_crease_color: Option<String>,
    pub accent_color: String,
    pub accent_side_color: String,
    pub spots: Vec<CatSpot>,
}

/// Returns the perceived brightness of a `#rgb` / `#rrggbb` colour.
///
/// @param hex The colour text.
/// @return Brightness 0..1, or 0.5 when the text is not a hex colour.
fn hex_luminance(hex: &str) -> f32 {
    let trimmed = hex.trim();
    let Some(digits) = trimmed.strip_prefix('#') else {
        return 0.5;
    };
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return 0.5;
    }
    let expanded: String = match digits.len() {
        3 => {
            let mut doubled = String::new();
            for character in digits.chars() {
                doubled.push(character);
                doubled.push(character);
            }
            doubled
        }
        6 => digits.to_string(),
        _ => return 0.5,
    };
    let red = u8::from_str_radix(&expanded[0..2], 16).unwrap_or(0) as f32 / 255.0;
    let green = u8::from_str_radix(&expanded[2..4], 16).unwrap_or(0) as f32 / 255.0;
    let blue = u8::from_str_radix(&expanded[4..6], 16).unwrap_or(0) as f32 / 255.0;
    0.299 * red + 0.587 * green + 0.114 * blue
}

/// Tells whether a colour is light (used to pick outline, nose and whisker defaults).
///
/// @param hex A `#rgb` or `#rrggbb` colour. Other formats count as light.
/// @return True when the colour is light.
pub fn is_light_color(hex: &str) -> bool {
    hex_luminance(hex) > 0.48
}

/// Returns an optional colour only when it is set and not empty.
///
/// @param value The optional colour.
/// @return The colour, or None when missing or empty.
fn non_empty(value: &Option<String>) -> Option<String> {
    match value {
        Some(text) if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    }
}

/// Returns a colour field that can be switched off on purpose: a missing field uses the
/// default, while an empty string means "none".
///
/// @param value The optional colour from the config.
/// @param default The colour to use when the field is missing.
/// @return The resulting colour or None.
fn explicit_or_default(value: &Option<String>, default: Option<String>) -> Option<String> {
    match value {
        Some(text) => {
            if text.trim().is_empty() {
                None
            } else {
                Some(text.clone())
            }
        }
        None => default,
    }
}

/// Returns one of two colours as an owned string.
///
/// @param condition Which colour to pick.
/// @param when_true Colour used when the condition is true.
/// @param when_false Colour used when the condition is false.
/// @return The chosen colour.
fn pick(condition: bool, when_true: &str, when_false: &str) -> String {
    if condition {
        when_true.to_string()
    } else {
        when_false.to_string()
    }
}

/// Returns a colour only when a condition holds.
///
/// @param condition Whether the colour applies.
/// @param color The colour.
/// @return The colour, or None.
fn only_if(condition: bool, color: &str) -> Option<String> {
    if condition {
        Some(color.to_string())
    } else {
        None
    }
}

/// Turns a cat skin configuration into a complete set of drawing colours, filling in
/// defaults that suit the fur colour and pattern.
///
/// @param raw The configuration from `cats.json` or an MCP client.
/// @return The resolved skin.
pub fn resolve_custom_skin(raw: &CatSkinConfig) -> ResolvedSkin {
    let fur = if raw.fur_color.trim().is_empty() {
        "#f2a452".to_string()
    } else {
        raw.fur_color.clone()
    };
    let dark_base = !is_light_color(&fur);
    let pattern = CatPattern::from_name(raw.pattern.as_deref().unwrap_or("solid"));

    let outline_color = explicit_or_default(&raw.outline_color, only_if(!dark_base, "#524840"));
    let paw_color = match non_empty(&raw.paw_color) {
        Some(color) => color,
        None => pick(pattern == CatPattern::Tuxedo, "#faf9f5", &fur),
    };
    let arm_color = non_empty(&raw.arm_color).unwrap_or_else(|| fur.clone());
    let paw_is_light = is_light_color(&paw_color);

    let raw_mask = non_empty(&raw.mask_color);
    let raw_ear_outer = non_empty(&raw.ear_outer_color);
    let raw_tail = non_empty(&raw.tail_color);
    let raw_eye_top = if raw.eye_color_top.trim().is_empty() {
        None
    } else {
        Some(raw.eye_color_top.clone())
    };

    let mask_color = match &raw_mask {
        Some(color) => Some(color.clone()),
        None => match pattern {
            CatPattern::Siamese => Some("#4a382c".to_string()),
            CatPattern::Bicolor => Some("#9e8e7f".to_string()),
            _ => None,
        },
    };
    let ear_outer_color = raw_ear_outer
        .clone()
        .or_else(|| raw_mask.clone())
        .unwrap_or_else(|| fur.clone());
    let tail_color = raw_tail
        .clone()
        .or_else(|| raw_ear_outer.clone())
        .or_else(|| raw_mask.clone())
        .unwrap_or_else(|| fur.clone());
    let tail_tip_color = non_empty(&raw.tail_tip_color)
        .or_else(|| non_empty(&raw.ear_tip_color))
        .or_else(|| raw_tail.clone())
        .unwrap_or_else(|| fur.clone());

    let eye_color_top = raw_eye_top.clone().unwrap_or_else(|| "#5a94d6".to_string());
    let eye_color_bottom = non_empty(&raw.eye_color_bottom)
        .or_else(|| raw_eye_top.clone())
        .unwrap_or_else(|| "#a3d2f2".to_string());
    let squint_color = match non_empty(&raw.squint_color) {
        Some(color) => color,
        None => {
            if dark_base {
                raw_eye_top.clone().unwrap_or_else(|| "#d4f044".to_string())
            } else {
                "#2c2928".to_string()
            }
        }
    };

    let default_paw_outline = if paw_is_light {
        Some(outline_color.clone().unwrap_or_else(|| "#524840".to_string()))
    } else {
        None
    };
    let default_arm_crease = only_if(!paw_is_light, "rgba(216, 218, 222, 0.62)");

    ResolvedSkin {
        is_dark_base: dark_base,
        fur_color: fur.clone(),
        fur_secondary_color: non_empty(&raw.fur_secondary_color).unwrap_or_else(|| fur.clone()),
        outline_color: outline_color.clone(),
        leg_line_color: non_empty(&raw.leg_line_color).unwrap_or_else(|| {
            pick(dark_base, "rgba(214, 218, 224, 0.62)", "rgba(82, 72, 64, 0.62)")
        }),
        pattern,
        mask_color,
        forehead_bridge_color: non_empty(&raw.forehead_bridge_color),
        flank_color: non_empty(&raw.flank_color),
        belly_bib_color: non_empty(&raw.belly_bib_color)
            .or_else(|| only_if(pattern == CatPattern::Tuxedo, "#faf9f5")),
        stripe_color: non_empty(&raw.stripe_color).or_else(|| only_if(pattern == CatPattern::Tabby, "#9e5318")),
        calico_patch_color: non_empty(&raw.calico_patch_color)
            .or_else(|| only_if(pattern == CatPattern::Calico, "#3a3533")),
        ear_outer_color,
        ear_tip_color: non_empty(&raw.ear_tip_color),
        ear_inner_color: non_empty(&raw.ear_inner_color).or_else(|| only_if(!dark_base, "#efc4c8")),
        ear_tuft_color: non_empty(&raw.ear_tuft_color),
        tail_color,
        tail_tip_color,
        tail_fluffy: raw.tail_fluffy.unwrap_or(false),
        tail_stripes: raw.tail_stripes.unwrap_or(false) || pattern == CatPattern::Tabby,
        eye_color_top,
        eye_color_bottom,
        pupil_color: non_empty(&raw.pupil_color).unwrap_or_else(|| "#10141e".to_string()),
        eye_catchlight: raw.eye_catchlight.unwrap_or(!dark_base),
        eye_rim_color: non_empty(&raw.eye_rim_color).or_else(|| only_if(!dark_base, "rgba(42, 46, 58, 0.78)")),
        squint_color,
        squint_glow_color: non_empty(&raw.squint_glow_color),
        nose_color: explicit_or_default(&raw.nose_color, only_if(!dark_base, "#ee8e9e")),
        mouth_color: non_empty(&raw.mouth_color).or_else(|| only_if(!dark_base, "rgba(72, 68, 76, 0.78)")),
        blush_color: non_empty(&raw.blush_color).or_else(|| only_if(!dark_base, "#f492a6")),
        whisker_color: non_empty(&raw.whisker_color).unwrap_or_else(|| {
            pick(dark_base, "rgba(210, 214, 220, 0.56)", "rgba(84, 76, 70, 0.56)")
        }),
        arm_color,
        paw_color,
        paw_outline_color: explicit_or_default(&raw.paw_outline_color, default_paw_outline),
        arm_crease_color: explicit_or_default(&raw.arm_crease_color, default_arm_crease),
        accent_color: non_empty(&raw.accent_color).unwrap_or_else(|| "#d2f244".to_string()),
        accent_side_color: non_empty(&raw.accent_side_color).unwrap_or_else(|| "#8cb014".to_string()),
        spots: raw.spots.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a skin config from JSON text.
    ///
    /// @param json The config as JSON.
    /// @return The parsed config.
    fn config(json: &str) -> CatSkinConfig {
        serde_json::from_str(json).expect("test config is valid")
    }

    #[test]
    fn dark_base_has_no_outline_or_nose() {
        let skin = resolve_custom_skin(&config(r##"{"id":"a","name":"A","furColor":"#1a1b1e"}"##));
        assert!(skin.is_dark_base);
        assert!(skin.outline_color.is_none());
        assert!(skin.nose_color.is_none());
        assert_eq!(skin.leg_line_color, "rgba(214, 218, 224, 0.62)");
    }

    #[test]
    fn light_base_gets_outline_and_paw_outline() {
        let skin = resolve_custom_skin(&config(r##"{"id":"b","name":"B","furColor":"#f2f0ea"}"##));
        assert!(!skin.is_dark_base);
        assert_eq!(skin.outline_color.as_deref(), Some("#524840"));
        assert_eq!(skin.paw_outline_color.as_deref(), Some("#524840"));
        assert!(skin.arm_crease_color.is_none());
    }

    #[test]
    fn explicit_empty_outline_means_none() {
        let skin = resolve_custom_skin(&config(
            r##"{"id":"c","name":"C","furColor":"#f2f0ea","outlineColor":""}"##,
        ));
        assert!(skin.outline_color.is_none());
        assert_eq!(skin.paw_outline_color.as_deref(), Some("#524840"));
    }

    #[test]
    fn tabby_defaults_stripes() {
        let skin = resolve_custom_skin(&config(
            r##"{"id":"d","name":"D","furColor":"#f2a452","pattern":"tabby"}"##,
        ));
        assert_eq!(skin.pattern, CatPattern::Tabby);
        assert_eq!(skin.stripe_color.as_deref(), Some("#9e5318"));
        assert!(skin.tail_stripes);
    }

    #[test]
    fn tail_colour_falls_back_through_ear_and_mask() {
        let with_ear = resolve_custom_skin(&config(
            r##"{"id":"e","name":"E","furColor":"#f2f0ea","earOuterColor":"#333333","maskColor":"#444444"}"##,
        ));
        assert_eq!(with_ear.tail_color, "#333333");
        let with_mask = resolve_custom_skin(&config(
            r##"{"id":"f","name":"F","furColor":"#f2f0ea","maskColor":"#444444"}"##,
        ));
        assert_eq!(with_mask.tail_color, "#444444");
        assert_eq!(with_mask.ear_outer_color, "#444444");
        let plain = resolve_custom_skin(&config(r##"{"id":"g","name":"G","furColor":"#f2f0ea"}"##));
        assert_eq!(plain.tail_color, "#f2f0ea");
    }

    #[test]
    fn siamese_and_tuxedo_defaults() {
        let siamese = resolve_custom_skin(&config(
            r##"{"id":"h","name":"H","furColor":"#f4ecdc","pattern":"siamese"}"##,
        ));
        assert_eq!(siamese.mask_color.as_deref(), Some("#4a382c"));
        let tuxedo = resolve_custom_skin(&config(
            r##"{"id":"i","name":"I","furColor":"#1a1b1e","pattern":"tuxedo"}"##,
        ));
        assert_eq!(tuxedo.paw_color, "#faf9f5");
        assert_eq!(tuxedo.belly_bib_color.as_deref(), Some("#faf9f5"));
        assert!(tuxedo.paw_outline_color.is_some());
    }
}
