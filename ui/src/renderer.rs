use std::collections::HashMap;
use skia_safe::{
    Canvas, Paint, RRect, FontMgr, FontStyle,
    PaintStyle, MaskFilter, BlurStyle, ClipOp, TileMode,
    image_filters, canvas::SaveLayerRec, gradient_shader,
};
use skia_safe::textlayout::{FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle, TextAlign as SkTextAlign,
    TextDecorationStyle as SkTextDecorationStyle, RectHeightStyle, RectWidthStyle};
use crate::color::Color;
use crate::display_list::{DrawCommand, Point};
use crate::layout::Rect;
use crate::element::ImageSource;

pub struct SkiaRenderer {
    font_collection: FontCollection,
    image_cache: HashMap<String, skia_safe::Image>,
}

impl SkiaRenderer {
    pub fn new() -> Self {
        let mut font_collection = FontCollection::new();
        font_collection.set_default_font_manager(FontMgr::default(), None);
        Self {
            font_collection,
            image_cache: HashMap::new(),
        }
    }

    pub fn execute(&mut self, canvas: &Canvas, commands: &[DrawCommand]) {
        for cmd in commands {
            match cmd {
                DrawCommand::Rect { bounds, background, corner_radii, border } => {
                    self.draw_rect(canvas, bounds, background, corner_radii, border.as_ref());
                }
                DrawCommand::Text { text, position, font_size, color, max_width, font_family,
                    font_weight, font_italic, line_height, text_align, max_lines, text_overflow_ellipsis,
                    letter_spacing, word_spacing, underline, strikethrough, overline,
                    text_decoration_style, text_decoration_color, text_shadow,
                    cursor_byte_offset, selection_byte_range, scroll_offset } => {
                    self.draw_text(canvas, text, position, *font_size, color, *max_width,
                        font_family.as_deref(), *font_weight, *font_italic, *line_height,
                        *text_align, *max_lines, *text_overflow_ellipsis, *letter_spacing,
                        *word_spacing, *underline, *strikethrough, *overline,
                        *text_decoration_style, text_decoration_color.as_ref(),
                        text_shadow, *cursor_byte_offset, *selection_byte_range, *scroll_offset);
                }
                DrawCommand::Image { source, bounds } => {
                    self.draw_image(canvas, source, bounds);
                }
                DrawCommand::GradientRect { bounds, gradient, corner_radii } => {
                    self.draw_gradient_rect(canvas, bounds, gradient, corner_radii);
                }
                DrawCommand::BoxShadow { bounds, corner_radii, blur, spread, color, offset } => {
                    self.draw_box_shadow(canvas, bounds, corner_radii, *blur, *spread, color, offset);
                }
                DrawCommand::InsetBoxShadow { bounds, corner_radii, blur, spread, color, offset } => {
                    self.draw_inset_shadow(canvas, bounds, corner_radii, *blur, *spread, color, offset);
                }
                DrawCommand::PushClip { bounds, corner_radii } => {
                    canvas.save();
                    let rrect = to_rrect(bounds, corner_radii);
                    canvas.clip_rrect(rrect, ClipOp::Intersect, true);
                }
                DrawCommand::PopClip => {
                    canvas.restore();
                }
                DrawCommand::PushLayer { opacity } => {
                    let mut paint = Paint::default();
                    paint.set_alpha_f(*opacity);
                    let rec = SaveLayerRec::default().paint(&paint);
                    canvas.save_layer(&rec);
                }
                DrawCommand::PopLayer => {
                    canvas.restore();
                }
                DrawCommand::BackdropBlur { bounds, corner_radii, blur_radius } => {
                    self.draw_backdrop_blur(canvas, bounds, corner_radii, *blur_radius);
                }
                DrawCommand::PushTranslate { offset } => {
                    canvas.save();
                    canvas.translate((offset.x, offset.y));
                }
                DrawCommand::PopTranslate => {
                    canvas.restore();
                }
                DrawCommand::RichText { spans, position, max_width, font_size, color,
                    font_family, font_weight, font_italic, line_height, text_align,
                    max_lines, text_overflow_ellipsis, letter_spacing, word_spacing, text_shadow } => {
                    self.draw_rich_text(canvas, spans, position, *max_width, *font_size, color,
                        font_family.as_deref(), *font_weight, *font_italic, *line_height,
                        *text_align, *max_lines, *text_overflow_ellipsis, *letter_spacing,
                        *word_spacing, text_shadow);
                }
            }
        }
    }

    fn draw_rect(&self, canvas: &Canvas, bounds: &Rect, bg: &Color, radii: &crate::style::CornerRadii, border: Option<&crate::style::Border>) {
        let rrect = to_rrect(bounds, radii);

        // Fill
        if bg.a > 0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(to_skia_color(bg));
            canvas.draw_rrect(rrect, &paint);
        }

        // Stroke
        if let Some(b) = border {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(b.width);
            paint.set_color(to_skia_color(&b.color));
            canvas.draw_rrect(rrect, &paint);
        }
    }

    fn draw_gradient_rect(
        &self,
        canvas: &Canvas,
        bounds: &Rect,
        gradient: &crate::style::Gradient,
        radii: &crate::style::CornerRadii,
    ) {
        let rrect = to_rrect(bounds, radii);

        // Resolve color stops: auto-distribute positions for stops without explicit position
        let resolve_stops = |stops: &[crate::style::ColorStop]| -> (Vec<skia_safe::Color>, Option<Vec<f32>>) {
            let colors: Vec<skia_safe::Color> = stops.iter().map(|s| to_skia_color(&s.color)).collect();
            let has_any_position = stops.iter().any(|s| s.position.is_some());
            if !has_any_position {
                // Even distribution — pass None to Skia
                (colors, None)
            } else {
                // Auto-distribute: fill in None positions linearly between known positions
                let n = stops.len();
                let mut positions = vec![0.0f32; n];
                // Set known positions
                for (i, stop) in stops.iter().enumerate() {
                    if let Some(p) = stop.position {
                        positions[i] = p;
                    } else if i == 0 {
                        positions[i] = 0.0;
                    } else if i == n - 1 {
                        positions[i] = 1.0;
                    } else {
                        positions[i] = f32::NAN; // mark for interpolation
                    }
                }
                // Interpolate NaN positions
                let mut i = 0;
                while i < n {
                    if positions[i].is_nan() {
                        let start_idx = i - 1;
                        let start_val = positions[start_idx];
                        let mut end_idx = i + 1;
                        while end_idx < n && positions[end_idx].is_nan() {
                            end_idx += 1;
                        }
                        let end_val = if end_idx < n { positions[end_idx] } else { 1.0 };
                        let count = end_idx - start_idx;
                        for j in (start_idx + 1)..end_idx {
                            positions[j] = start_val + (end_val - start_val) * ((j - start_idx) as f32 / count as f32);
                        }
                        i = end_idx;
                    } else {
                        i += 1;
                    }
                }
                (colors, Some(positions))
            }
        };

        let shader = match gradient {
            crate::style::Gradient::Linear { angle_deg, stops } => {
                let (colors, positions) = resolve_stops(stops);
                // CSS angles: 0deg = to top, 90deg = to right, 180deg = to bottom
                let angle_rad = (angle_deg - 90.0).to_radians();
                let cx = bounds.x + bounds.width / 2.0;
                let cy = bounds.y + bounds.height / 2.0;
                // Half-diagonal projection for full coverage
                let half_w = bounds.width / 2.0;
                let half_h = bounds.height / 2.0;
                let cos_a = angle_rad.cos();
                let sin_a = angle_rad.sin();
                let len = (half_w * cos_a).abs() + (half_h * sin_a).abs();
                let start = skia_safe::Point::new(cx - cos_a * len, cy - sin_a * len);
                let end = skia_safe::Point::new(cx + cos_a * len, cy + sin_a * len);
                gradient_shader::linear(
                    (start, end),
                    skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                    positions.as_deref(),
                    TileMode::Clamp,
                )
            }
            crate::style::Gradient::Radial { center, stops } => {
                let (colors, positions) = resolve_stops(stops);
                let (fx, fy) = center.unwrap_or((0.5, 0.5));
                let cx = bounds.x + bounds.width * fx;
                let cy = bounds.y + bounds.height * fy;
                // Radius: farthest corner distance
                let radius = ((bounds.width / 2.0).powi(2) + (bounds.height / 2.0).powi(2)).sqrt();
                gradient_shader::radial(
                    skia_safe::Point::new(cx, cy),
                    radius,
                    skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                    positions.as_deref(),
                    TileMode::Clamp,
                )
            }
            crate::style::Gradient::Conic { center, from_angle_deg, stops } => {
                let (colors, positions) = resolve_stops(stops);
                let (fx, fy) = center.unwrap_or((0.5, 0.5));
                let cx = bounds.x + bounds.width * fx;
                let cy = bounds.y + bounds.height * fy;
                gradient_shader::sweep(
                    skia_safe::Point::new(cx, cy),
                    skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                    positions.as_deref(),
                    TileMode::Clamp,
                    Some(*from_angle_deg),
                    Some(*from_angle_deg + 360.0),
                )
            }
        };

        if let Some(shader) = shader {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_shader(shader);
            canvas.draw_rrect(rrect, &paint);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &mut self,
        canvas: &Canvas,
        text: &str,
        pos: &Point,
        font_size: f32,
        color: &Color,
        max_width: f32,
        font_family: Option<&[String]>,
        font_weight: Option<i32>,
        font_italic: bool,
        line_height: Option<f32>,
        text_align: Option<crate::style::TextAlign>,
        max_lines: Option<usize>,
        text_overflow_ellipsis: bool,
        letter_spacing: f32,
        word_spacing: f32,
        underline: bool,
        strikethrough: bool,
        overline: bool,
        text_decoration_style: Option<crate::style::TextDecorationStyle>,
        text_decoration_color: Option<&Color>,
        text_shadow: &[(Color, (f32, f32), f64)],
        cursor_byte_offset: Option<usize>,
        selection_byte_range: Option<(usize, usize)>,
        scroll_offset: f32,
    ) {
        use skia_safe::textlayout::{TextDecoration, TextShadow};
        use skia_safe::font_style::{Weight, Width, Slant};

        // ParagraphStyle
        let mut para_style = ParagraphStyle::new();
        if let Some(align) = text_align {
            para_style.set_text_align(match align {
                crate::style::TextAlign::Left => SkTextAlign::Left,
                crate::style::TextAlign::Center => SkTextAlign::Center,
                crate::style::TextAlign::Right => SkTextAlign::Right,
                crate::style::TextAlign::Justify => SkTextAlign::Justify,
            });
        }
        if let Some(max) = max_lines {
            para_style.set_max_lines(max);
        }
        if text_overflow_ellipsis {
            para_style.set_ellipsis("\u{2026}");
        }

        // TextStyle
        let mut text_style = TextStyle::new();
        text_style.set_font_size(font_size);
        text_style.set_color(to_skia_color(color));

        if let Some(families) = font_family {
            text_style.set_font_families(families);
        }

        let weight = font_weight
            .map(|w| Weight::from(w))
            .unwrap_or(Weight::NORMAL);
        let slant = if font_italic { Slant::Italic } else { Slant::Upright };
        text_style.set_font_style(FontStyle::new(weight, Width::NORMAL, slant));

        if let Some(lh) = line_height {
            text_style.set_height(lh);
            text_style.set_height_override(true);
        }

        if letter_spacing != 0.0 {
            text_style.set_letter_spacing(letter_spacing);
        }
        if word_spacing != 0.0 {
            text_style.set_word_spacing(word_spacing);
        }

        // Decorations
        let mut deco = TextDecoration::NO_DECORATION;
        if underline {
            deco |= TextDecoration::UNDERLINE;
        }
        if strikethrough {
            deco |= TextDecoration::LINE_THROUGH;
        }
        if overline {
            deco |= TextDecoration::OVERLINE;
        }
        if deco != TextDecoration::NO_DECORATION {
            text_style.set_decoration_type(deco);
            if let Some(deco_style) = text_decoration_style {
                text_style.set_decoration_style(to_skia_decoration_style(deco_style));
            }
            if let Some(deco_color) = text_decoration_color {
                text_style.set_decoration_color(to_skia_color(deco_color));
            }
        }

        // Shadows
        for (shadow_color, (dx, dy), blur) in text_shadow {
            text_style.add_shadow(TextShadow::new(
                to_skia_color(shadow_color),
                (*dx, *dy),
                *blur,
            ));
        }

        para_style.set_text_style(&text_style);

        // Build and layout paragraph
        let mut builder = ParagraphBuilder::new(&para_style, self.font_collection.clone());
        builder.push_style(&text_style);
        builder.add_text(text);
        let mut paragraph = builder.build();
        paragraph.layout(max_width);

        let has_input_state = cursor_byte_offset.is_some() || selection_byte_range.is_some() || scroll_offset != 0.0;

        if has_input_state {
            // Text input: clip to bounds, apply scroll offset, draw selection/cursor
            canvas.save();
            canvas.clip_rect(
                skia_safe::Rect::from_xywh(pos.x, pos.y, max_width, paragraph.height()),
                ClipOp::Intersect,
                true,
            );

            let text_x = pos.x - scroll_offset;
            let text_y = pos.y;
            let para_height = paragraph.height();

            // Draw selection highlight
            if let Some((start, end)) = selection_byte_range {
                let rects = paragraph.get_rects_for_range(
                    start..end,
                    RectHeightStyle::Tight,
                    RectWidthStyle::Tight,
                );
                let mut sel_paint = Paint::default();
                sel_paint.set_color(skia_safe::Color::from_argb(80, 100, 150, 255));
                sel_paint.set_anti_alias(true);
                for text_box in &rects {
                    let r = text_box.rect;
                    canvas.draw_rect(
                        skia_safe::Rect::from_xywh(text_x + r.left, text_y + r.top, r.width(), r.height()),
                        &sel_paint,
                    );
                }
            }

            // Paint text
            paragraph.paint(canvas, (text_x, text_y));

            // Draw cursor
            if let Some(offset) = cursor_byte_offset {
                let cursor_x = if text.is_empty() || offset == 0 {
                    0.0
                } else {
                    let rects = paragraph.get_rects_for_range(
                        0..offset.min(text.len()),
                        RectHeightStyle::Tight,
                        RectWidthStyle::Tight,
                    );
                    rects.last().map(|r| r.rect.right).unwrap_or(0.0)
                };

                let mut cursor_paint = Paint::default();
                cursor_paint.set_color(to_skia_color(color));
                cursor_paint.set_anti_alias(true);
                cursor_paint.set_stroke_width(1.5);
                canvas.draw_rect(
                    skia_safe::Rect::from_xywh(text_x + cursor_x, text_y, 1.5, para_height),
                    &cursor_paint,
                );
            }

            canvas.restore();
        } else {
            paragraph.paint(canvas, (pos.x, pos.y));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_rich_text(
        &mut self,
        canvas: &Canvas,
        spans: &[crate::element::TextSpan],
        pos: &Point,
        max_width: f32,
        base_font_size: f32,
        base_color: &Color,
        font_family: Option<&[String]>,
        font_weight: Option<i32>,
        font_italic: bool,
        line_height: Option<f32>,
        text_align: Option<crate::style::TextAlign>,
        max_lines: Option<usize>,
        text_overflow_ellipsis: bool,
        letter_spacing: f32,
        word_spacing: f32,
        text_shadow: &[(Color, (f32, f32), f64)],
    ) {
        use skia_safe::textlayout::{TextDecoration, TextShadow};
        use skia_safe::font_style::{Weight, Width, Slant};

        // ParagraphStyle
        let mut para_style = ParagraphStyle::new();
        if let Some(align) = text_align {
            para_style.set_text_align(match align {
                crate::style::TextAlign::Left => SkTextAlign::Left,
                crate::style::TextAlign::Center => SkTextAlign::Center,
                crate::style::TextAlign::Right => SkTextAlign::Right,
                crate::style::TextAlign::Justify => SkTextAlign::Justify,
            });
        }
        if let Some(max) = max_lines {
            para_style.set_max_lines(max);
        }
        if text_overflow_ellipsis {
            para_style.set_ellipsis("\u{2026}");
        }

        // Base text style
        let mut base_style = TextStyle::new();
        base_style.set_font_size(base_font_size);
        base_style.set_color(to_skia_color(base_color));
        if let Some(families) = font_family {
            base_style.set_font_families(families);
        }
        let base_weight = font_weight.map(|w| Weight::from(w)).unwrap_or(Weight::NORMAL);
        let base_slant = if font_italic { Slant::Italic } else { Slant::Upright };
        base_style.set_font_style(FontStyle::new(base_weight, Width::NORMAL, base_slant));
        if let Some(lh) = line_height {
            base_style.set_height(lh);
            base_style.set_height_override(true);
        }
        if letter_spacing != 0.0 {
            base_style.set_letter_spacing(letter_spacing);
        }
        if word_spacing != 0.0 {
            base_style.set_word_spacing(word_spacing);
        }
        for (shadow_color, (dx, dy), blur) in text_shadow {
            base_style.add_shadow(TextShadow::new(
                to_skia_color(shadow_color),
                (*dx, *dy),
                *blur,
            ));
        }
        para_style.set_text_style(&base_style);

        let mut builder = ParagraphBuilder::new(&para_style, self.font_collection.clone());

        for span in spans {
            let mut style = base_style.clone();

            if let Some(c) = span.color {
                style.set_color(to_skia_color(&c));
            }
            if let Some(size) = span.font_size {
                style.set_font_size(size);
            }
            let w = span.font_weight.map(|w| Weight::from(w)).unwrap_or(base_weight);
            let s = if span.italic { Slant::Italic } else { base_slant };
            if span.font_weight.is_some() || span.italic {
                style.set_font_style(FontStyle::new(w, Width::NORMAL, s));
            }
            if let Some(ref families) = span.font_family {
                style.set_font_families(families);
            }
            if let Some(ls) = span.letter_spacing {
                style.set_letter_spacing(ls);
            }

            let mut deco = TextDecoration::NO_DECORATION;
            if span.underline { deco |= TextDecoration::UNDERLINE; }
            if span.strikethrough { deco |= TextDecoration::LINE_THROUGH; }
            if deco != TextDecoration::NO_DECORATION {
                style.set_decoration_type(deco);
                if let Some(deco_style) = span.text_decoration_style {
                    style.set_decoration_style(to_skia_decoration_style(deco_style));
                }
                if let Some(deco_color) = span.text_decoration_color {
                    style.set_decoration_color(to_skia_color(&deco_color));
                }
            }

            if let Some(bg) = span.background {
                let mut bg_paint = Paint::default();
                bg_paint.set_color(to_skia_color(&bg));
                style.set_background_paint(&bg_paint);
            }

            builder.push_style(&style);
            builder.add_text(&span.content);
            builder.pop();
        }

        let mut paragraph = builder.build();
        paragraph.layout(max_width);
        paragraph.paint(canvas, (pos.x, pos.y));
    }

    pub fn font_collection(&self) -> FontCollection {
        self.font_collection.clone()
    }

    fn draw_image(&mut self, canvas: &Canvas, source: &ImageSource, bounds: &Rect) {
        let cache_key = match source {
            ImageSource::Svg(s) => format!("svg:{}", &s[..s.len().min(64)]),
            ImageSource::File(p) => format!("file:{}", p),
        };

        if !self.image_cache.contains_key(&cache_key) {
            let img = match source {
                ImageSource::Svg(data) => load_svg(data.as_bytes(), (bounds.width as u32, bounds.height as u32)),
                ImageSource::File(path) => load_image_file(path, (bounds.width as u32, bounds.height as u32)),
            };
            if let Some(img) = img {
                self.image_cache.insert(cache_key.clone(), img);
            }
        }

        if let Some(img) = self.image_cache.get(&cache_key) {
            let dst = skia_safe::Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height);
            canvas.draw_image_rect(img, None, dst, &Paint::default());
        }
    }

    fn draw_box_shadow(&self, canvas: &Canvas, bounds: &Rect, radii: &crate::style::CornerRadii, blur: f32, spread: f32, color: &Color, offset: &Point) {
        let expanded = Rect {
            x: bounds.x + offset.x - spread,
            y: bounds.y + offset.y - spread,
            width: bounds.width + spread * 2.0,
            height: bounds.height + spread * 2.0,
        };
        let rrect = to_rrect(&expanded, radii);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(to_skia_color(color));
        paint.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, blur / 2.0, false));
        canvas.draw_rrect(rrect, &paint);
    }

    fn draw_inset_shadow(
        &self,
        canvas: &Canvas,
        bounds: &Rect,
        radii: &crate::style::CornerRadii,
        blur: f32,
        spread: f32,
        color: &Color,
        offset: &Point,
    ) {
        canvas.save();
        // Clip to the element bounds so the shadow only shows inside
        let clip_rrect = to_rrect(bounds, radii);
        canvas.clip_rrect(clip_rrect, ClipOp::Intersect, true);

        // The inner rect is the element bounds inset by spread, offset by shadow offset.
        // The shadow is the blur of the area *outside* this inner rect but *inside* the clip.
        let inner = Rect {
            x: bounds.x + offset.x + spread,
            y: bounds.y + offset.y + spread,
            width: (bounds.width - spread * 2.0).max(0.0),
            height: (bounds.height - spread * 2.0).max(0.0),
        };
        let inner_rrect = to_rrect(&inner, radii);

        // Draw a large outer rect with the inner rect subtracted (EvenOdd fill rule)
        // The large outer rect extends well beyond the clip to ensure full coverage
        let margin = blur * 2.0 + spread.abs() + 50.0;
        let outer_rect = skia_safe::Rect::from_xywh(
            bounds.x - margin,
            bounds.y - margin,
            bounds.width + margin * 2.0,
            bounds.height + margin * 2.0,
        );

        let mut path = skia_safe::Path::new();
        path.set_fill_type(skia_safe::PathFillType::EvenOdd);
        path.add_rect(outer_rect, None);
        path.add_rrect(inner_rrect, None);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(to_skia_color(color));
        paint.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, blur / 2.0, false));
        canvas.draw_path(&path, &paint);

        canvas.restore();
    }

    fn draw_backdrop_blur(&self, canvas: &Canvas, bounds: &Rect, radii: &crate::style::CornerRadii, blur_radius: f32) {
        canvas.save();
        let rrect = to_rrect(bounds, radii);
        canvas.clip_rrect(rrect, ClipOp::Intersect, true);

        if let Some(filter) = image_filters::blur((blur_radius, blur_radius), None, None, None) {
            let mut paint = Paint::default();
            paint.set_image_filter(filter);
            let rec = SaveLayerRec::default().paint(&paint);
            canvas.save_layer(&rec);
            canvas.restore();
        }

        canvas.restore();
    }
}

/// Measure text using SkParagraph. Returns (width, height).
/// The `width` returned is the longest line width (max_intrinsic for single-line).
pub fn measure_text_paragraph(
    text: &str,
    font_size: f32,
    font_collection: &mut FontCollection,
    font_family: Option<&[String]>,
    font_weight: Option<i32>,
    font_italic: bool,
    line_height: Option<f32>,
    letter_spacing: f32,
    word_spacing: f32,
    max_width: f32,
    max_lines: Option<usize>,
    text_overflow_ellipsis: bool,
) -> (f32, f32) {
    use skia_safe::font_style::{Weight, Width, Slant};

    let mut para_style = ParagraphStyle::new();
    if let Some(max) = max_lines {
        para_style.set_max_lines(max);
    }
    if text_overflow_ellipsis {
        para_style.set_ellipsis("\u{2026}");
    }

    let mut text_style = TextStyle::new();
    text_style.set_font_size(font_size);
    if let Some(families) = font_family {
        text_style.set_font_families(families);
    }
    let weight = font_weight.map(|w| Weight::from(w)).unwrap_or(Weight::NORMAL);
    let slant = if font_italic { Slant::Italic } else { Slant::Upright };
    text_style.set_font_style(FontStyle::new(weight, Width::NORMAL, slant));
    if let Some(lh) = line_height {
        text_style.set_height(lh);
        text_style.set_height_override(true);
    }
    if letter_spacing != 0.0 {
        text_style.set_letter_spacing(letter_spacing);
    }
    if word_spacing != 0.0 {
        text_style.set_word_spacing(word_spacing);
    }
    para_style.set_text_style(&text_style);

    let mut builder = ParagraphBuilder::new(&para_style, font_collection.clone());
    builder.push_style(&text_style);
    builder.add_text(text);
    let mut paragraph = builder.build();
    paragraph.layout(max_width);
    (paragraph.longest_line().ceil(), paragraph.height())
}

fn to_skia_color(c: &Color) -> skia_safe::Color {
    skia_safe::Color::from_argb(c.a, c.r, c.g, c.b)
}

fn to_skia_decoration_style(s: crate::style::TextDecorationStyle) -> SkTextDecorationStyle {
    match s {
        crate::style::TextDecorationStyle::Solid => SkTextDecorationStyle::Solid,
        crate::style::TextDecorationStyle::Double => SkTextDecorationStyle::Double,
        crate::style::TextDecorationStyle::Dotted => SkTextDecorationStyle::Dotted,
        crate::style::TextDecorationStyle::Dashed => SkTextDecorationStyle::Dashed,
        crate::style::TextDecorationStyle::Wavy => SkTextDecorationStyle::Wavy,
    }
}

fn to_rrect(bounds: &Rect, radii: &crate::style::CornerRadii) -> RRect {
    let rect = skia_safe::Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height);
    if radii.top_left == radii.top_right
        && radii.top_right == radii.bottom_right
        && radii.bottom_right == radii.bottom_left
    {
        // Fast path: uniform radius
        RRect::new_rect_xy(rect, radii.top_left, radii.top_left)
    } else {
        // Per-corner radii: [top-left, top-right, bottom-right, bottom-left], each (rx, ry)
        let corner_radii = [
            skia_safe::Point::new(radii.top_left, radii.top_left),
            skia_safe::Point::new(radii.top_right, radii.top_right),
            skia_safe::Point::new(radii.bottom_right, radii.bottom_right),
            skia_safe::Point::new(radii.bottom_left, radii.bottom_left),
        ];
        RRect::new_rect_radii(rect, &corner_radii)
    }
}

// Image loading functions
fn load_svg(data: &[u8], target_size: (u32, u32)) -> Option<skia_safe::Image> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(data, &opt).ok()?;
    let size = tree.size();

    let (tw, th) = target_size;
    let tw = tw.max(1);
    let th = th.max(1);

    // Render SVG to a tiny-skia pixmap first, then convert to skia Image
    let sx = tw as f32 / size.width();
    let sy = th as f32 / size.height();
    let scale = sx.min(sy);

    let mut pixmap = resvg::tiny_skia::Pixmap::new(tw, th)?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Convert tiny-skia pixmap to skia-safe Image
    pixmap_to_skia_image(&pixmap)
}

fn load_image_file(path: &str, target_size: (u32, u32)) -> Option<skia_safe::Image> {
    if path.ends_with(".svg") {
        let data = std::fs::read(path).ok()?;
        return load_svg(&data, target_size);
    }

    let data = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&data).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());

    let info = skia_safe::ImageInfo::new(
        (w as i32, h as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    );

    skia_safe::images::raster_from_data(
        &info,
        skia_safe::Data::new_copy(rgba.as_raw()),
        w as usize * 4,
    )
}

fn pixmap_to_skia_image(pixmap: &resvg::tiny_skia::Pixmap) -> Option<skia_safe::Image> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let info = skia_safe::ImageInfo::new(
        (w as i32, h as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );

    skia_safe::images::raster_from_data(
        &info,
        skia_safe::Data::new_copy(pixmap.data()),
        w as usize * 4,
    )
}
