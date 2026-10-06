//! Premultiplied BGRA for the transparent taskbar surface.

pub fn pixel(coverage: u8, foreground: [u8; 3], hover: bool) -> [u8; 4] {
    // Alpha zero makes a layered-window pixel click-through. One out of 255
    // keeps the entire cell clickable with a visually negligible background.
    let background = if hover { 28 } else { 1 };
    let coverage = u32::from(coverage);
    let alpha = coverage + ((255 - coverage) * background + 127) / 255;
    let channel = |value: u8| ((u32::from(value) * alpha + 127) / 255) as u8;
    [
        channel(foreground[2]),
        channel(foreground[1]),
        channel(foreground[0]),
        alpha as u8,
    ]
}

/// Composite a premultiplied glyph over the subtle hit-test/hover background.
/// Retain DirectWrite's alpha and colored edge pixels instead of recoloring them.
pub fn over_background(glyph: [u8; 4], foreground: [u8; 3], hover: bool) -> [u8; 4] {
    let background = pixel(0, foreground, hover);
    let remainder = 255 - u32::from(glyph[3]);
    std::array::from_fn(|channel| {
        (u32::from(glyph[channel]) + (u32::from(background[channel]) * remainder + 127) / 255) as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directwrite_edges_keep_their_color_and_alpha_over_hover() {
        for alpha in 0..=255u8 {
            let glyph = [alpha / 4, alpha / 2, alpha, alpha];
            for hover in [false, true] {
                let result = over_background(glyph, [240; 3], hover);
                assert!(result[3] >= alpha);
                assert!(result[..3].iter().all(|channel| *channel <= result[3]));
                if alpha == 255 {
                    assert_eq!(result, glyph);
                }
            }
        }
        assert_eq!(over_background([0; 4], [0; 3], false), [0, 0, 0, 1]);
    }

    #[test]
    fn transparent_cells_remain_clickable_and_glyphs_keep_full_opacity() {
        assert_eq!(pixel(0, [0, 0, 0], false), [0, 0, 0, 1]);
        assert_eq!(pixel(0, [255, 255, 255], true), [28, 28, 28, 28]);
        assert_eq!(pixel(255, [16, 32, 64], false), [64, 32, 16, 255]);
    }

    #[test]
    fn every_antialiased_pixel_is_valid_premultiplied_bgra() {
        for coverage in 0..=255 {
            for hover in [false, true] {
                for foreground in [[0, 0, 0], [255, 255, 255], [24, 128, 240]] {
                    let pixel = pixel(coverage, foreground, hover);
                    assert!(pixel[3] > 0);
                    assert!(pixel[..3].iter().all(|channel| *channel <= pixel[3]));
                }
            }
        }
    }
}
