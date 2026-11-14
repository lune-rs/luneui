//! The glyph rasterizer: one glyph's coverage image for a cosmic-text cache key and the
//! [`GlyphParams`] (hinting and stem darkening) it is drawn with. The toolkit's text paths rasterize
//! through it, and an app's own glyph renderer can too, so both draw the same pixels.

mod embolden;

use std::fmt;

use cosmic_text::{CacheKey, CacheKeyFlags, FontSystem, SwashCache, SwashImage};
use lune_ui_core::text::GlyphParams;
use swash::scale::image::Content;
use swash::scale::outline::Outline;
use swash::scale::{ScaleContext, Source};
use swash::zeno::{Angle, Format, Mask, Origin, Style, Transform, Vector};

/// Rasterizes single glyphs with a [`GlyphParams`]: hinting on or off, and the outline grown by a
/// stem-darkening dilation on each side.
///
/// [`image`](Self::image) follows three rules:
/// 1. `GlyphParams::OFF` returns cosmic-text's own image of the key, unchanged
///    (`SwashCache::get_image_uncached`): the look without smoothing.
/// 2. Otherwise hinting off adds `CacheKeyFlags::DISABLE_HINTING` to the key (a key that already
///    carries it stays unhinted) and cosmic-text picks the glyph's source as it always does; a
///    colour outline or colour bitmap (an emoji, say) is returned as cosmic-text drew it, never
///    dilated.
/// 3. An outline glyph is filled again from its outline, grown by `params.dilation()` on each
///    side and moved back so it stays centred where it was, through the same scaler settings and
///    mask steps cosmic-text uses. At zero dilation that gives cosmic-text's bytes exactly.
///    Each layer is pushed outwards the way FreeType finds its orientation, from the sum of its
///    contours' own areas. If the grown glyph has less ink than rule 2's image (a layer whose
///    contours disagree with each other), the opposite push is tried; if that has less ink too,
///    rule 2's image is returned, so smoothing never draws a glyph thinner than it is without
///    dilation.
///
/// An app's own glyph cache must key each image by the glyph's `GlyphParams` as well as its
/// `CacheKey` (two settings give two images), and must leave `DISABLE_HINTING` to this rasterizer
/// rather than set it itself. Hinting here is the rasterization hinting of the glyph outline; it is
/// distinct from the text shaper's metrics hinting (cosmic-text's `Buffer::set_hinting`, which
/// rounds the layout, is off by default, and the toolkit never sets it).
///
/// The `FontSystem` and `CacheKey` passed in must come from the toolkit's cosmic-text version
/// (0.19): an app's dependency tree must hold that one version (`cargo tree -i cosmic-text` lists
/// one).
///
/// Deterministic: the same inputs give the same bytes on any instance; nothing is cached between
/// calls.
pub struct GlyphRasterizer {
    swash: SwashCache,
    scale: ScaleContext,
}

impl GlyphRasterizer {
    /// A rasterizer with empty scaling contexts.
    pub fn new() -> Self {
        Self {
            swash: SwashCache::new(),
            scale: ScaleContext::new(),
        }
    }

    /// The coverage image of `key` drawn with `params` (the rules are on the type), or `None` when
    /// the font or the glyph is missing.
    pub fn image(
        &mut self,
        fonts: &mut FontSystem,
        key: CacheKey,
        params: GlyphParams,
    ) -> Option<SwashImage> {
        if params == GlyphParams::OFF {
            return self.swash.get_image_uncached(fonts, key);
        }
        let mut key = key;
        if !params.hinting() {
            key.flags |= CacheKeyFlags::DISABLE_HINTING;
        }
        let image = self.swash.get_image_uncached(fonts, key)?;
        if !matches!(image.source, Source::Outline) {
            return Some(image);
        }
        let floor = ink(&image.data);
        Some(
            self.outline(fonts, key, params.dilation(), floor)
                .unwrap_or(image),
        )
    }

    /// The cosmic-text cache this rasterizer draws rule 1 and 2 images with.
    pub(crate) fn swash(&mut self) -> &mut SwashCache {
        &mut self.swash
    }

    /// Rule 3: the outline of `key`, grown by `dx`, `dy` a side and re-centred, filled as swash's
    /// `Render` fills an outline with cosmic-text's settings; `None` when no push keeps `floor`
    /// ink (see [`select`]).
    fn outline(
        &mut self,
        fonts: &mut FontSystem,
        key: CacheKey,
        dilation: [f32; 2],
        floor: u64,
    ) -> Option<SwashImage> {
        let font = fonts.get_font(key.font_id, key.font_weight)?;
        let face = font.as_swash();
        let wght = swash::Tag::from_be_bytes(*b"wght");
        let mut builder = self
            .scale
            .builder(face)
            .size(f32::from_bits(key.font_size_bits))
            .hint(!key.flags.contains(CacheKeyFlags::DISABLE_HINTING));
        if let Some(axis) = face.variations().find_by_tag(wght) {
            builder = builder.normalized_coords(face.variations().normalized_coords([(
                wght,
                f32::from(key.font_weight.0).clamp(axis.min_value(), axis.max_value()),
            )]));
        }
        let mut scaler = builder.build();
        let outline = scaler.scale_outline(key.glyph_id)?;
        let offset = if key.flags.contains(CacheKeyFlags::PIXEL_FONT) {
            Vector::new(key.x_bin.as_float().round(), key.y_bin.as_float().round())
        } else {
            Vector::new(key.x_bin.as_float(), key.y_bin.as_float())
        };
        let fill = FillSettings {
            skew: key.flags.contains(CacheKeyFlags::FAKE_ITALIC),
            offset,
        };
        select(&outline, dilation, floor, fill)
    }
}

/// The coverage total of a mask: the sum of its bytes, exact.
fn ink(mask: &[u8]) -> u64 {
    mask.iter().map(|&a| u64::from(a)).sum()
}

/// How `fill` turns an outline into a mask: the `FAKE_ITALIC` skew and the sub-pixel offset.
#[derive(Debug, Clone, Copy)]
struct FillSettings {
    skew: bool,
    offset: Vector,
}

/// Grows `outline` by `dx`, `dy` a side, moves it back by the same and fills it. Each layer is
/// pushed out by its summed orientation; if the result has less ink than `floor` the opposite
/// push is tried, and if that has less ink too, `None` (the caller keeps the undilated image).
/// Zero dilation fills the outline as it is, with no check.
fn select(
    outline: &Outline,
    [dx, dy]: [f32; 2],
    floor: u64,
    fill: FillSettings,
) -> Option<SwashImage> {
    if dx == 0.0 && dy == 0.0 {
        return Some(fill_outline(outline.clone(), fill));
    }
    for orient in [embolden::Orient::Summed, embolden::Orient::Opposite] {
        let mut grown = outline.clone();
        // `grow` grows the box by twice the strength with its min corner fixed; the translate
        // moves it back so the growth is `dx`, `dy` on each side.
        embolden::grow(&mut grown, orient, dx, dy);
        grown.transform(&Transform::translation(-dx, -dy));
        let image = fill_outline(grown, fill);
        if ink(&image.data) >= floor {
            return Some(image);
        }
    }
    None
}

/// The outline skewed for `FAKE_ITALIC` and masked as swash's `Render` masks it.
fn fill_outline(mut outline: Outline, FillSettings { skew, offset }: FillSettings) -> SwashImage {
    if skew {
        outline.transform(&Transform::skew(
            Angle::from_degrees(14.0),
            Angle::from_degrees(0.0),
        ));
    }
    let mut image = SwashImage::new();
    // `inspect` sizes the buffer, and also sets the mask height that `placement.top` reads.
    let placement = Mask::new(outline.path())
        .format(Format::Alpha)
        .origin(Origin::BottomLeft)
        .style(Style::Fill(swash::zeno::Fill::NonZero))
        .offset(offset)
        .render_offset(offset)
        .inspect(|format, width, height| {
            image.data.resize(format.buffer_size(width, height), 0);
        })
        .render_into(&mut image.data[..], None);
    image.placement = placement;
    image.content = Content::Mask;
    image.source = Source::Outline;
    image
}

impl Default for GlyphRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for GlyphRasterizer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlyphRasterizer").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lune_ui_core::text::TextRendering;
    use swash::FontRef;

    const SANS: &[u8] = include_bytes!("../../../apps/gallery/assets/RznSans-Regular.ttf");

    /// RznSans's `⅓` is two far-apart contours groups whose joined sign points inward (0.5.1
    /// drew it thinner than undilated). Its mirror (x negated) takes the mirrored push: both grow
    /// above their own undilated ink, by the same amount within 1%.
    #[test]
    fn a_glyph_and_its_mirror_both_grow_above_their_floor() {
        let font = FontRef::from_index(SANS, 0).expect("the font parses");
        let glyph = font.charmap().map('\u{2153}');
        assert_ne!(glyph, 0, "RznSans has the one third");
        let mut ctx = ScaleContext::new();
        let mut scaler = ctx.builder(font).size(26.0).hint(false).build();
        let outline = scaler
            .scale_outline(glyph)
            .expect("the glyph has an outline");
        let mut mirror = outline.clone();
        for point in mirror.points_mut() {
            point.x = -point.x;
        }
        let dilation = TextRendering::SMOOTH.glyph_params(26.0).dilation();
        let fill = FillSettings {
            skew: false,
            offset: Vector::new(0.0, 0.0),
        };
        let mut grown = Vec::new();
        for (name, o) in [("glyph", &outline), ("mirror", &mirror)] {
            let floor = ink(&select(o, [0.0, 0.0], 0, fill).expect("fills").data);
            // The first try alone, with no retry to rescue it, already clears the floor.
            let mut first = o.clone();
            embolden::grow(
                &mut first,
                embolden::Orient::Summed,
                dilation[0],
                dilation[1],
            );
            first.transform(&Transform::translation(-dilation[0], -dilation[1]));
            let first = ink(&fill_outline(first, fill).data);
            assert!(
                first > floor,
                "the {name} at its summed push: {floor} -> {first}"
            );
            let image = select(o, dilation, floor, fill)
                .unwrap_or_else(|| panic!("the {name} grows, it does not fall back"));
            let got = ink(&image.data);
            assert!(got > floor, "the {name} grows: {floor} -> {got}");
            grown.push(got as f64);
        }
        let rel = (grown[0] - grown[1]).abs() / grown[0];
        assert!(
            rel < 0.01,
            "ink {} vs {} differs by {rel}",
            grown[0],
            grown[1]
        );
    }
}
