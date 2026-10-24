//! An image's size on a chapter's page (lib/Epub's `ChapterHtmlSlimParser`): from its CSS `width`
//! and `height` when it sets them, keeping its proportions for a side it leaves out, else its own
//! size; never wider than its container or taller than the page.

use crate::{Defined, LengthField, Style};

/// To the nearest pixel (halves up, then toward zero, as the C++ did). The cast saturates where
/// the C++ was undefined.
#[allow(clippy::cast_possible_truncation)]
fn round(px: f32) -> i32 {
    (px + 0.5) as i32
}

/// The factor that fits `width` by `height` pixels into `container` by `page`: at most 1.
#[allow(clippy::cast_precision_loss)]
fn fit(width: i32, height: i32, container: i32, page: i32) -> f32 {
    let x = if width > container {
        container as f32 / width as f32
    } else {
        1.0
    };
    let y = if height > page {
        page as f32 / height as f32
    } else {
        1.0
    };
    if x < y { x } else { y }
}

/// The size, in pixels, to show an image `natural` (width, height) pixels large, for `css`, with
/// ems of `em`, in a container `container` pixels wide on a page `page` pixels tall. A percentage
/// width is of the container, a percentage height of the page.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub fn size(css: &Style, natural: (i16, i16), em: f32, container: i32, page: u16) -> (i32, i32) {
    let (w, h) = (i32::from(natural.0), i32::from(natural.1));
    let page = i32::from(page);
    let css_px = |field, base: i32| round(css.length(field).pixels(em, base as f32));
    let wide = css.defined.has(Defined::length(LengthField::ImageWidth));
    let tall = css.defined.has(Defined::length(LengthField::ImageHeight));
    if !(w > 0 && h > 0) || !(wide || tall) {
        // Its own size, shrunk to fit.
        let scale = fit(w, h, container, page);
        return ((w as f32 * scale) as i32, (h as f32 * scale) as i32);
    }
    let across = w as f32 / h as f32;
    let down = h as f32 / w as f32;
    if wide && tall {
        let mut dw = css_px(LengthField::ImageWidth, container).max(1);
        let mut dh = css_px(LengthField::ImageHeight, page).max(1);
        if dw > container || dh > page {
            // Both shrink by the same factor, keeping the asked proportions.
            let scale = fit(dw, dh, container, page);
            dw = round(dw as f32 * scale).max(1);
            dh = round(dh as f32 * scale).max(1);
        }
        (dw, dh)
    } else if tall {
        let mut dh = css_px(LengthField::ImageHeight, page).max(1);
        let mut dw = round(dh as f32 * across);
        if dh > page {
            dh = page;
            dw = round(dh as f32 * across);
        }
        if dw > container {
            dw = container;
            dh = round(dw as f32 * down).max(1);
        }
        (dw.max(1), dh)
    } else {
        let mut dw = css_px(LengthField::ImageWidth, container)
            .min(container)
            .max(1);
        let mut dh = round(dw as f32 * down);
        if dh > page {
            dh = page;
            dw = round(dh as f32 * across).max(1);
        }
        (dw, dh.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::size;
    use crate::{Defined, Length, LengthField, Style, Unit};

    fn css(sides: &[(LengthField, f32, Unit)]) -> Style {
        let mut s = Style::DEFAULT;
        for &(field, value, unit) in sides {
            s.set_length(field, Length { value, unit });
            s.defined.set(Defined::length(field));
        }
        s
    }

    const W: LengthField = LengthField::ImageWidth;
    const H: LengthField = LengthField::ImageHeight;

    #[test]
    fn without_css_an_image_keeps_its_size_and_shrinks_to_fit() {
        let plain = Style::DEFAULT;
        assert_eq!(size(&plain, (100, 50), 10.0, 400, 800), (100, 50));
        assert_eq!(size(&plain, (1000, 500), 10.0, 400, 800), (400, 200));
        assert_eq!(size(&plain, (300, 1601), 10.0, 400, 800), (149, 800));
        // Without a usable size of its own, CSS cannot keep its proportions.
        let wide = css(&[(W, 50.0, Unit::Percent)]);
        assert_eq!(size(&wide, (0, 0), 10.0, 400, 800), (0, 0));
    }

    #[test]
    fn one_css_side_keeps_the_proportions() {
        let (natural, em) = ((300, 150), 10.0);
        assert_eq!(
            size(&css(&[(W, 50.0, Unit::Percent)]), natural, em, 400, 800),
            (200, 100)
        );
        assert_eq!(
            size(&css(&[(H, 2.0, Unit::Em)]), natural, em, 400, 800),
            (40, 20)
        );
        // Too tall for the page, then too wide for the container.
        assert_eq!(
            size(&css(&[(H, 150.0, Unit::Percent)]), natural, em, 400, 800),
            (400, 200)
        );
        assert_eq!(
            size(&css(&[(W, 900.0, Unit::Pixels)]), (100, 400), em, 400, 800),
            (200, 800)
        );
        // Never under a pixel.
        assert_eq!(
            size(&css(&[(W, 0.1, Unit::Pixels)]), (1000, 10), em, 400, 800),
            (1, 1)
        );
    }

    #[test]
    fn both_css_sides_shrink_together() {
        let both = css(&[(W, 800.0, Unit::Pixels), (H, 100.0, Unit::Pixels)]);
        assert_eq!(size(&both, (10, 10), 10.0, 400, 800), (400, 50));
        let points = css(&[(W, 30.0, Unit::Points), (H, 3.0, Unit::Rem)]);
        assert_eq!(size(&points, (10, 10), 10.0, 400, 800), (40, 30));
    }
}
