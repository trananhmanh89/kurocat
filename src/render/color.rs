//! Parsing of CSS colour strings such as `#1a1b1e`, `#fff8`, `rgba(0, 0, 0, 0.25)`,
//! `hsl(30 80% 50%)` or `orange`.
//!
//! Skins and MCP clients describe colours as CSS strings, so the renderer accepts the same format.

use tiny_skia::Color;

/// Parses a CSS colour string.
///
/// Supported forms: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()` / `rgba()`, `hsl()` / `hsla()`,
/// the CSS colour names (`orange`, `pink`, ...) and `transparent`.
///
/// @param text The colour text.
/// @return The colour, or None when the text is not a supported colour.
pub fn parse_color(text: &str) -> Option<Color> {
    let lower = text.trim().to_ascii_lowercase();
    if lower == "transparent" {
        return Some(Color::TRANSPARENT);
    }
    if let Some(hex) = lower.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(inner) = function_arguments(&lower, "rgba") {
        return parse_rgb_arguments(inner);
    }
    if let Some(inner) = function_arguments(&lower, "rgb") {
        return parse_rgb_arguments(inner);
    }
    if let Some(inner) = function_arguments(&lower, "hsla") {
        return parse_hsl_arguments(inner);
    }
    if let Some(inner) = function_arguments(&lower, "hsl") {
        return parse_hsl_arguments(inner);
    }
    named_color(&lower)
}

/// Returns the text between `name(` and the closing `)`.
///
/// @param text Lower-case colour text.
/// @param name The function name, e.g. "rgba".
/// @return The argument text, or None when the text is not that function.
fn function_arguments<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(name)?.trim_start();
    let rest = rest.strip_prefix('(')?;
    rest.strip_suffix(')')
}

/// Splits function arguments separated by commas, spaces or `/` (before the alpha).
///
/// @param inner The text inside the parentheses.
/// @return The non-empty parts.
fn split_arguments(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    for part in inner.split(is_argument_separator) {
        if !part.is_empty() {
            parts.push(part);
        }
    }
    parts
}

/// Reports whether a character separates colour function arguments.
///
/// @param character The character.
/// @return True for commas, slashes and white space.
fn is_argument_separator(character: char) -> bool {
    character == ',' || character == '/' || character.is_whitespace()
}

/// Parses the arguments of `rgb()` / `rgba()`.
///
/// @param inner The text inside the parentheses.
/// @return The colour, or None when the arguments are invalid.
fn parse_rgb_arguments(inner: &str) -> Option<Color> {
    let parts = split_arguments(inner);
    if parts.len() < 3 {
        return None;
    }
    let red = parse_channel(parts[0])?;
    let green = parse_channel(parts[1])?;
    let blue = parse_channel(parts[2])?;
    let alpha = if parts.len() >= 4 { parse_alpha(parts[3])? } else { 1.0 };
    Color::from_rgba(red, green, blue, alpha)
}

/// Parses the arguments of `hsl()` / `hsla()`: hue in degrees (or `deg` / `turn`), then
/// saturation and lightness as percentages, then an optional alpha.
///
/// @param inner The text inside the parentheses.
/// @return The colour, or None when the arguments are invalid.
fn parse_hsl_arguments(inner: &str) -> Option<Color> {
    let parts = split_arguments(inner);
    if parts.len() < 3 {
        return None;
    }
    let hue_degrees = parse_hue(parts[0])?;
    let saturation = parse_percentage(parts[1])?;
    let lightness = parse_percentage(parts[2])?;
    let alpha = if parts.len() >= 4 { parse_alpha(parts[3])? } else { 1.0 };
    let (red, green, blue) = hsl_to_rgb(hue_degrees, saturation, lightness);
    Color::from_rgba(red, green, blue, alpha)
}

/// Parses a hue: a plain number or `deg` means degrees, `turn` means whole turns.
///
/// @param text The hue text.
/// @return The hue in degrees, 0..360.
fn parse_hue(text: &str) -> Option<f32> {
    let degrees: f32 = if let Some(turns) = text.strip_suffix("turn") {
        turns.parse::<f32>().ok()? * 360.0
    } else if let Some(value) = text.strip_suffix("deg") {
        value.parse().ok()?
    } else {
        text.parse().ok()?
    };
    Some(degrees.rem_euclid(360.0))
}

/// Parses a percentage such as `45%` (a plain number is read as a percentage too).
///
/// @param text The percentage text.
/// @return The value as 0..1.
fn parse_percentage(text: &str) -> Option<f32> {
    let number = text.strip_suffix('%').unwrap_or(text);
    let value: f32 = number.parse().ok()?;
    Some((value / 100.0).clamp(0.0, 1.0))
}

/// Converts HSL to RGB, as defined by CSS.
///
/// @param hue_degrees Hue, 0..360.
/// @param saturation Saturation, 0..1.
/// @param lightness Lightness, 0..1.
/// @return (red, green, blue), each 0..1.
fn hsl_to_rgb(hue_degrees: f32, saturation: f32, lightness: f32) -> (f32, f32, f32) {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue_degrees / 60.0;
    let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = if sector < 1.0 {
        (chroma, second, 0.0)
    } else if sector < 2.0 {
        (second, chroma, 0.0)
    } else if sector < 3.0 {
        (0.0, chroma, second)
    } else if sector < 4.0 {
        (0.0, second, chroma)
    } else if sector < 5.0 {
        (second, 0.0, chroma)
    } else {
        (chroma, 0.0, second)
    };
    let lift = lightness - chroma / 2.0;
    (red + lift, green + lift, blue + lift)
}

/// Parses one rgb() channel: a number 0..255 or a percentage.
///
/// @param text The channel text.
/// @return The channel as 0..1.
fn parse_channel(text: &str) -> Option<f32> {
    if let Some(percent) = text.strip_suffix('%') {
        let value: f32 = percent.parse().ok()?;
        return Some((value / 100.0).clamp(0.0, 1.0));
    }
    let value: f32 = text.parse().ok()?;
    Some((value / 255.0).clamp(0.0, 1.0))
}

/// Parses an alpha value: a number 0..1 or a percentage.
///
/// @param text The alpha text.
/// @return The alpha as 0..1.
fn parse_alpha(text: &str) -> Option<f32> {
    if let Some(percent) = text.strip_suffix('%') {
        let value: f32 = percent.parse().ok()?;
        return Some((value / 100.0).clamp(0.0, 1.0));
    }
    let value: f32 = text.parse().ok()?;
    Some(value.clamp(0.0, 1.0))
}

/// Parses the digits of a `#` colour.
///
/// @param hex The digits without the `#`.
/// @return The colour, or None when the digits are invalid.
fn parse_hex(hex: &str) -> Option<Color> {
    let mut digits: Vec<u8> = Vec::new();
    for character in hex.chars() {
        digits.push(character.to_digit(16)? as u8);
    }
    let (red, green, blue, alpha) = match digits.len() {
        3 => (digits[0] * 17, digits[1] * 17, digits[2] * 17, 255),
        4 => (digits[0] * 17, digits[1] * 17, digits[2] * 17, digits[3] * 17),
        6 => (
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
            255,
        ),
        8 => (
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
            digits[6] * 16 + digits[7],
        ),
        _ => return None,
    };
    Some(Color::from_rgba8(red, green, blue, alpha))
}

/// Looks up a CSS colour name.
///
/// @param name The lower-case name.
/// @return The colour, or None when the name is unknown.
fn named_color(name: &str) -> Option<Color> {
    for (known_name, value) in NAMED_COLORS {
        if *known_name == name {
            let red = ((value >> 16) & 0xff) as u8;
            let green = ((value >> 8) & 0xff) as u8;
            let blue = (value & 0xff) as u8;
            return Some(Color::from_rgba8(red, green, blue, 255));
        }
    }
    None
}

/// The CSS named colours, as 0xRRGGBB.
const NAMED_COLORS: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff), ("antiquewhite", 0xfaebd7), ("aqua", 0x00ffff), ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff), ("beige", 0xf5f5dc), ("bisque", 0xffe4c4), ("black", 0x000000),
    ("blanchedalmond", 0xffebcd), ("blue", 0x0000ff), ("blueviolet", 0x8a2be2), ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887), ("cadetblue", 0x5f9ea0), ("chartreuse", 0x7fff00), ("chocolate", 0xd2691e),
    ("coral", 0xff7f50), ("cornflowerblue", 0x6495ed), ("cornsilk", 0xfff8dc), ("crimson", 0xdc143c),
    ("cyan", 0x00ffff), ("darkblue", 0x00008b), ("darkcyan", 0x008b8b), ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9), ("darkgreen", 0x006400), ("darkgrey", 0xa9a9a9), ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b), ("darkolivegreen", 0x556b2f), ("darkorange", 0xff8c00), ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000), ("darksalmon", 0xe9967a), ("darkseagreen", 0x8fbc8f), ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f), ("darkslategrey", 0x2f4f4f), ("darkturquoise", 0x00ced1), ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493), ("deepskyblue", 0x00bfff), ("dimgray", 0x696969), ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff), ("firebrick", 0xb22222), ("floralwhite", 0xfffaf0), ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff), ("gainsboro", 0xdcdcdc), ("ghostwhite", 0xf8f8ff), ("gold", 0xffd700),
    ("goldenrod", 0xdaa520), ("gray", 0x808080), ("green", 0x008000), ("greenyellow", 0xadff2f),
    ("grey", 0x808080), ("honeydew", 0xf0fff0), ("hotpink", 0xff69b4), ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082), ("ivory", 0xfffff0), ("khaki", 0xf0e68c), ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5), ("lawngreen", 0x7cfc00), ("lemonchiffon", 0xfffacd), ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080), ("lightcyan", 0xe0ffff), ("lightgoldenrodyellow", 0xfafad2), ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90), ("lightgrey", 0xd3d3d3), ("lightpink", 0xffb6c1), ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa), ("lightskyblue", 0x87cefa), ("lightslategray", 0x778899), ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de), ("lightyellow", 0xffffe0), ("lime", 0x00ff00), ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6), ("magenta", 0xff00ff), ("maroon", 0x800000), ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd), ("mediumorchid", 0xba55d3), ("mediumpurple", 0x9370db), ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee), ("mediumspringgreen", 0x00fa9a), ("mediumturquoise", 0x48d1cc), ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970), ("mintcream", 0xf5fffa), ("mistyrose", 0xffe4e1), ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead), ("navy", 0x000080), ("oldlace", 0xfdf5e6), ("olive", 0x808000),
    ("olivedrab", 0x6b8e23), ("orange", 0xffa500), ("orangered", 0xff4500), ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa), ("palegreen", 0x98fb98), ("paleturquoise", 0xafeeee), ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5), ("peachpuff", 0xffdab9), ("peru", 0xcd853f), ("pink", 0xffc0cb),
    ("plum", 0xdda0dd), ("powderblue", 0xb0e0e6), ("purple", 0x800080), ("rebeccapurple", 0x663399),
    ("red", 0xff0000), ("rosybrown", 0xbc8f8f), ("royalblue", 0x4169e1), ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072), ("sandybrown", 0xf4a460), ("seagreen", 0x2e8b57), ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d), ("silver", 0xc0c0c0), ("skyblue", 0x87ceeb), ("slateblue", 0x6a5acd),
    ("slategray", 0x708090), ("slategrey", 0x708090), ("snow", 0xfffafa), ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4), ("tan", 0xd2b48c), ("teal", 0x008080), ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347), ("turquoise", 0x40e0d0), ("violet", 0xee82ee), ("wheat", 0xf5deb3),
    ("white", 0xffffff), ("whitesmoke", 0xf5f5f5), ("yellow", 0xffff00), ("yellowgreen", 0x9acd32),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks that a colour matches 8-bit channel values.
    ///
    /// @param color The colour to check.
    /// @param expected The expected (r, g, b, a) bytes.
    fn assert_rgba8(color: Color, expected: (u8, u8, u8, u8)) {
        let actual = color.to_color_u8();
        assert_eq!((actual.red(), actual.green(), actual.blue(), actual.alpha()), expected);
    }

    #[test]
    fn parses_hex_forms() {
        assert_rgba8(parse_color("#fff").unwrap(), (255, 255, 255, 255));
        assert_rgba8(parse_color("#1a1b1e").unwrap(), (26, 27, 30, 255));
        assert_rgba8(parse_color("#00000080").unwrap(), (0, 0, 0, 128));
        assert_rgba8(parse_color("#f008").unwrap(), (255, 0, 0, 136));
    }

    #[test]
    fn parses_rgb_functions() {
        assert_rgba8(parse_color("rgba(0, 0, 0, 0.5)").unwrap(), (0, 0, 0, 128));
        assert_rgba8(parse_color("rgb(255,128,0)").unwrap(), (255, 128, 0, 255));
        assert_rgba8(parse_color("rgb(255 128 0 / 50%)").unwrap(), (255, 128, 0, 128));
    }

    #[test]
    fn parses_hsl_functions() {
        assert_rgba8(parse_color("hsl(0, 100%, 50%)").unwrap(), (255, 0, 0, 255));
        assert_rgba8(parse_color("hsl(120deg 100% 25%)").unwrap(), (0, 128, 0, 255));
        assert_rgba8(parse_color("hsla(240, 100%, 50%, 0.5)").unwrap(), (0, 0, 255, 128));
        assert_rgba8(parse_color("hsl(0.5turn 0% 100%)").unwrap(), (255, 255, 255, 255));
    }

    #[test]
    fn parses_named_colors() {
        assert_rgba8(parse_color("Orange").unwrap(), (255, 165, 0, 255));
        assert_rgba8(parse_color("pink").unwrap(), (255, 192, 203, 255));
        assert_rgba8(parse_color("transparent").unwrap(), (0, 0, 0, 0));
    }

    #[test]
    fn rejects_invalid_text() {
        assert!(parse_color("#12").is_none());
        assert!(parse_color("banana").is_none());
        assert!(parse_color("rgb(1,2)").is_none());
    }
}
