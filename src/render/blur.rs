//! Fast approximate Gaussian blur, used for drop shadows and soft edges.
//!
//! Three box blurs in a row look almost identical to a Gaussian blur and cost the same
//! no matter how large the blur radius is.

/// Works out the three box widths that together approximate a Gaussian blur.
///
/// @param sigma The Gaussian standard deviation in pixels.
/// @return Three box radii (half-widths) in pixels.
pub fn box_radii_for_sigma(sigma: f32) -> [usize; 3] {
    if sigma <= 0.0 {
        return [0, 0, 0];
    }
    let passes = 3.0_f32;
    let ideal_width = (12.0 * sigma * sigma / passes + 1.0).sqrt();
    let mut lower_width = ideal_width.floor() as i32;
    if lower_width % 2 == 0 {
        lower_width -= 1;
    }
    let upper_width = lower_width + 2;
    let lower = lower_width as f32;
    let ideal_count = (12.0 * sigma * sigma
        - passes * lower * lower
        - 4.0 * passes * lower
        - 3.0 * passes)
        / (-4.0 * lower - 4.0);
    let lower_count = ideal_count.round() as i32;

    let mut radii = [0usize; 3];
    for (index, radius) in radii.iter_mut().enumerate() {
        let width = if (index as i32) < lower_count { lower_width } else { upper_width };
        *radius = ((width - 1) / 2).max(0) as usize;
    }
    radii
}

/// Blurs a single-channel image in place.
///
/// @param values One byte per pixel, row by row.
/// @param width Image width in pixels.
/// @param height Image height in pixels.
/// @param sigma The Gaussian standard deviation in pixels. Canvas `shadowBlur` = 2 × sigma.
pub fn blur_alpha(values: &mut [u8], width: usize, height: usize, sigma: f32) {
    if width == 0 || height == 0 || sigma < 0.3 {
        return;
    }
    let mut scratch = vec![0u8; values.len()];
    for radius in box_radii_for_sigma(sigma) {
        if radius == 0 {
            continue;
        }
        box_blur_horizontal(values, &mut scratch, width, height, radius);
        box_blur_vertical(&scratch, values, width, height, radius);
    }
}

/// One horizontal box-blur pass. Pixels outside the image count as transparent.
///
/// @param source Input pixels.
/// @param target Output pixels (same size).
/// @param width Image width.
/// @param height Image height.
/// @param radius Box half-width.
fn box_blur_horizontal(source: &[u8], target: &mut [u8], width: usize, height: usize, radius: usize) {
    let box_size = (radius * 2 + 1) as u32;
    for row in 0..height {
        let start = row * width;
        let line = &source[start..start + width];
        let mut sum: u32 = 0;
        for x in 0..radius.min(width) {
            sum += line[x] as u32;
        }
        for x in 0..width {
            let entering = x + radius;
            if entering < width {
                sum += line[entering] as u32;
            }
            target[start + x] = ((sum + box_size / 2) / box_size) as u8;
            if x >= radius {
                sum -= line[x - radius] as u32;
            }
        }
    }
}

/// One vertical box-blur pass. Pixels outside the image count as transparent.
///
/// @param source Input pixels.
/// @param target Output pixels (same size).
/// @param width Image width.
/// @param height Image height.
/// @param radius Box half-height.
fn box_blur_vertical(source: &[u8], target: &mut [u8], width: usize, height: usize, radius: usize) {
    let box_size = (radius * 2 + 1) as u32;
    let mut sums = vec![0u32; width];
    for y in 0..radius.min(height) {
        let row = &source[y * width..(y + 1) * width];
        for x in 0..width {
            sums[x] += row[x] as u32;
        }
    }
    for y in 0..height {
        let entering = y + radius;
        if entering < height {
            let row = &source[entering * width..(entering + 1) * width];
            for x in 0..width {
                sums[x] += row[x] as u32;
            }
        }
        let out = &mut target[y * width..(y + 1) * width];
        for x in 0..width {
            out[x] = ((sums[x] + box_size / 2) / box_size) as u8;
        }
        if y >= radius {
            let leaving = &source[(y - radius) * width..(y - radius + 1) * width];
            for x in 0..width {
                sums[x] -= leaving[x] as u32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_sigma_leaves_image_unchanged() {
        let mut values = vec![0, 255, 0, 0];
        blur_alpha(&mut values, 4, 1, 0.0);
        assert_eq!(values, vec![0, 255, 0, 0]);
    }

    #[test]
    fn blur_spreads_a_single_dot_and_keeps_it_centred() {
        let size = 21;
        let mut values = vec![0u8; size * size];
        values[10 * size + 10] = 255;
        blur_alpha(&mut values, size, size, 2.0);
        let centre = values[10 * size + 10];
        assert!(centre > 0 && centre < 255);
        assert_eq!(values[10 * size + 7], values[10 * size + 13]);
        assert_eq!(values[7 * size + 10], values[13 * size + 10]);
    }

    #[test]
    fn large_filled_area_stays_solid_in_the_middle() {
        let size = 40;
        let mut values = vec![255u8; size * size];
        blur_alpha(&mut values, size, size, 3.0);
        assert_eq!(values[20 * size + 20], 255);
        assert!(values[0] < 255);
    }
}
