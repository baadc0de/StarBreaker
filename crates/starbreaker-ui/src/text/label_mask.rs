//! Label masks: one line of text in an imported SWF font as a white glyph mask on transparency, for a host that
//! places and tints the label itself (a viewer drawing its own UI in a game font).
//!
//! - [`render_label_mask`]: the mask of `text` at an em size, with GFx letter spacing, drawn by the same glyph
//!   rasteriser as the canvas renders ([`TextRenderer::draw_swf_font`]); alpha is the glyph coverage, colour white.

use image::{Rgba, RgbaImage};

use crate::bb_layout::Rect;
use crate::swf_assets::{FontGlyphSet, SwfEditTextMetrics};

use super::{TextAlign, TextRenderer, VerticalAlign};

/// A rendered label mask and the em size it was drawn at.
#[derive(Debug, Clone)]
pub struct LabelMask {
    /// White, alpha = coverage. Width: the line's advance plus `em_px / 4` of clear margin each side; height: the
    /// font's ascent + descent at `em_px` plus the same margin above and below.
    pub image: RgbaImage,
    pub em_px: f32,
}

/// `text` (one line) in `font` at `em_px`, letter spacing `letter_spacing_px` added to every advance (GFx
/// `letterSpacing`), centred in the mask. `None` for empty text or a font that measures nothing.
pub fn render_label_mask(
    font: &FontGlyphSet,
    metrics: Option<&SwfEditTextMetrics>,
    text: &str,
    em_px: f32,
    letter_spacing_px: f32,
) -> Option<LabelMask> {
    let renderer = TextRenderer::new();
    let advance = renderer.measure_swf_advance_width(text, font, em_px, letter_spacing_px)?;
    if advance <= 0.0 {
        return None;
    }
    let ascent = font.ascent.map(f32::from)?;
    let descent = font.descent.map(f32::from)?;
    let units_per_em = ascent.abs() + descent.abs();
    if units_per_em <= 0.0 {
        return None;
    }
    let margin = em_px / 4.0;
    let line_h = (ascent.abs() + descent.abs()) / units_per_em * em_px;
    let w = (advance + 2.0 * margin).ceil() as u32;
    let h = (line_h + 2.0 * margin).ceil() as u32;
    // Drawn white over transparency: the compositor leaves premultiplied white, i.e. every channel = coverage.
    let mut image = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));
    let rect = Rect { x: 0.0, y: 0.0, w: w as f32, h: h as f32 };
    if !renderer.draw_swf_font(
        &mut image,
        text,
        rect,
        font,
        metrics,
        em_px,
        [255, 255, 255, 255],
        TextAlign::Centre,
        VerticalAlign::Centre,
        None,
        letter_spacing_px,
    ) {
        return None;
    }
    for pixel in image.pixels_mut() {
        *pixel = Rgba([255, 255, 255, pixel[3]]);
    }
    Some(LabelMask { image, em_px })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swf_assets::FontGlyph;
    use swf::{Point, PointDelta, ShapeRecord, StyleChangeData, Twips};

    /// One square glyph 'A' (300 units on an 800 + 200 em, advance 360) and a space.
    fn square_font() -> FontGlyphSet {
        let edge = |x, y| ShapeRecord::StraightEdge { delta: PointDelta::new(Twips::new(x), Twips::new(y)) };
        FontGlyphSet {
            id: 1,
            name: "Test".to_string(),
            is_bold: false,
            is_italic: false,
            ascent: Some(800),
            descent: Some(200),
            leading: Some(0),
            glyphs: vec![
                FontGlyph {
                    code: Some('A' as u16),
                    advance: Some(360),
                    shape_records: vec![
                        ShapeRecord::StyleChange(Box::new(StyleChangeData {
                            move_to: Some(Point::new(Twips::new(0), Twips::new(-300))),
                            fill_style_0: None,
                            fill_style_1: None,
                            line_style: None,
                            new_styles: None,
                        })),
                        edge(300, 0),
                        edge(0, 300),
                        edge(-300, 0),
                        edge(0, -300),
                    ],
                },
                FontGlyph { code: Some(' ' as u16), advance: Some(250), shape_records: vec![] },
            ],
        }
    }

    fn ink_columns(image: &RgbaImage) -> (u32, u32) {
        let cols: Vec<u32> = (0..image.width()).filter(|&x| (0..image.height()).any(|y| image.get_pixel(x, y)[3] > 127)).collect();
        (*cols.first().unwrap(), *cols.last().unwrap())
    }

    #[test]
    fn mask_is_white_coverage_on_transparency() {
        let mask = render_label_mask(&square_font(), None, "A", 100.0, 0.0).expect("mask");
        assert_eq!(mask.em_px, 100.0);
        // 100 px line (800 + 200 units) + 25 px margin each side; 36 px advance + 50 px margins.
        assert_eq!((mask.image.width(), mask.image.height()), (86, 150));
        let mut ink = 0;
        for p in mask.image.pixels() {
            assert_eq!((p[0], p[1], p[2]), (255, 255, 255), "colour is white everywhere");
            ink += (p[3] > 127) as u32;
        }
        // The 30 x 30 px square, nothing else.
        assert!((850..=950).contains(&ink), "ink {ink}");
        assert_eq!(mask.image.get_pixel(0, 0)[3], 0, "margin stays clear");
    }

    #[test]
    fn letter_spacing_widens_the_line_and_keeps_it_centred() {
        let tight = render_label_mask(&square_font(), None, "AA", 100.0, 0.0).expect("tight");
        let spaced = render_label_mask(&square_font(), None, "AA", 100.0, 10.0).expect("spaced");
        assert_eq!(spaced.image.width(), tight.image.width() + 20, "two advances, each + 10 px");
        let (l, r) = ink_columns(&spaced.image);
        let centre = (l + r) as f32 / 2.0;
        assert!((centre - spaced.image.width() as f32 / 2.0).abs() <= 1.5, "ink centred: {l}..{r} in {}", spaced.image.width());
    }

    #[test]
    fn nothing_to_draw_gives_none() {
        assert!(render_label_mask(&square_font(), None, "", 100.0, 0.0).is_none());
    }
}
