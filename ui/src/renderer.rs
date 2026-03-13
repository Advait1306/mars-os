use std::collections::HashMap;
use skia_safe::{
    Canvas, Paint, RRect, FontMgr, FontStyle, PathEffect,
    PaintStyle, MaskFilter, BlurStyle, ClipOp, TileMode,
    image_filters, canvas::SaveLayerRec, gradient_shader,
};
use skia_safe::textlayout::{FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle, TextAlign as SkTextAlign,
    TextDecorationStyle as SkTextDecorationStyle, RectHeightStyle, RectWidthStyle};
use crate::color::Color;
use crate::display_list::{DrawCommand, Point};
use crate::layout::Rect;
use crate::element::ImageSource;

struct CacheEntry {
    image: skia_safe::Image,
    last_used: std::time::Instant,
    byte_size: usize,
}

pub struct SkiaRenderer {
    font_collection: FontCollection,
    image_cache: HashMap<u64, CacheEntry>,
    cache_total_bytes: usize,
    cache_max_bytes: usize,
    vector_svg_cache: HashMap<u64, crate::svg_render::VectorSvg>,
}

impl SkiaRenderer {
    pub fn new() -> Self {
        let mut font_collection = FontCollection::new();
        font_collection.set_default_font_manager(FontMgr::default(), None);
        Self {
            font_collection,
            image_cache: HashMap::new(),
            cache_total_bytes: 0,
            cache_max_bytes: 64 * 1024 * 1024, // 64 MB
            vector_svg_cache: HashMap::new(),
        }
    }

    pub fn execute(&mut self, canvas: &Canvas, commands: &[DrawCommand]) {
        for cmd in commands {
            match cmd {
                DrawCommand::Rect { bounds, background, corner_radii, border, border_style } => {
                    self.draw_rect(canvas, bounds, background, corner_radii, border.as_ref(), *border_style);
                }
                DrawCommand::PerSideBorder { bounds, corner_radii, full_border } => {
                    self.draw_per_side_border(canvas, bounds, corner_radii, full_border);
                }
                DrawCommand::Outline { bounds, corner_radii, outline } => {
                    self.draw_outline(canvas, bounds, corner_radii, outline);
                }
                DrawCommand::Text { text, position, font_size, color, max_width, font_family,
                    font_weight, font_italic, line_height, text_align, max_lines, text_overflow_ellipsis,
                    letter_spacing, word_spacing, underline, strikethrough, overline,
                    text_decoration_style, text_decoration_color, text_shadow,
                    font_features, font_variations,
                    cursor_byte_offset, selection_byte_range, scroll_offset,
                    preedit_byte_range } => {
                    self.draw_text(canvas, text, position, *font_size, color, *max_width,
                        font_family.as_deref(), *font_weight, *font_italic, *line_height,
                        *text_align, *max_lines, *text_overflow_ellipsis, *letter_spacing,
                        *word_spacing, *underline, *strikethrough, *overline,
                        *text_decoration_style, text_decoration_color.as_ref(),
                        text_shadow, font_features, font_variations,
                        *cursor_byte_offset, *selection_byte_range, *scroll_offset,
                        *preedit_byte_range);
                }
                DrawCommand::Path { data, bounds } => {
                    self.draw_path(canvas, data, bounds);
                }
                DrawCommand::Image { source, bounds, tint, image_fit } => {
                    self.draw_image(canvas, source, bounds, tint.as_ref(), *image_fit);
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
                DrawCommand::PushFilter { filters } => {
                    if let Some(filter) = build_image_filter(filters) {
                        let mut paint = Paint::default();
                        paint.set_image_filter(filter);
                        let rec = SaveLayerRec::default().paint(&paint);
                        canvas.save_layer(&rec);
                    } else {
                        canvas.save();
                    }
                }
                DrawCommand::PopFilter => {
                    canvas.restore();
                }
                DrawCommand::PushBlendMode { mode } => {
                    let mut paint = Paint::default();
                    paint.set_blend_mode(to_skia_blend_mode(*mode));
                    let rec = SaveLayerRec::default().paint(&paint);
                    canvas.save_layer(&rec);
                }
                DrawCommand::PopBlendMode => {
                    canvas.restore();
                }
                DrawCommand::ApplyBackdropFilter { bounds, corner_radii, filters } => {
                    self.draw_backdrop_filter(canvas, bounds, corner_radii, filters);
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
                DrawCommand::PushTransform { transforms, origin } => {
                    canvas.save();
                    // Translate to origin, apply transforms, translate back
                    canvas.translate((origin.x, origin.y));
                    for t in transforms {
                        match t {
                            crate::style::Transform::Translate(x, y) => {
                                canvas.translate((*x, *y));
                            }
                            crate::style::Transform::Rotate(deg) => {
                                canvas.rotate(*deg, None);
                            }
                            crate::style::Transform::Scale(sx, sy) => {
                                canvas.scale((*sx, *sy));
                            }
                            crate::style::Transform::Skew(x_deg, y_deg) => {
                                canvas.skew(x_deg.to_radians().tan(), y_deg.to_radians().tan());
                            }
                            crate::style::Transform::Matrix(m) => {
                                let mut mat = skia_safe::Matrix::new_identity();
                                // CSS matrix(a,b,c,d,e,f) -> | a c e | / | b d f | / | 0 0 1 |
                                mat.set_all(m[0], m[2], m[4], m[1], m[3], m[5], 0.0, 0.0, 1.0);
                                canvas.concat(&mat);
                            }
                        }
                    }
                    canvas.translate((-origin.x, -origin.y));
                }
                DrawCommand::PopTransform => {
                    canvas.restore();
                }
                DrawCommand::Circle { center, radius, fill, stroke } => {
                    if let Some(fill_color) = fill {
                        let mut paint = Paint::default();
                        paint.set_anti_alias(true);
                        paint.set_color(skia_safe::Color::from_argb(fill_color.a, fill_color.r, fill_color.g, fill_color.b));
                        canvas.draw_circle((center.x, center.y), *radius, &paint);
                    }
                    if let Some((stroke_color, stroke_width)) = stroke {
                        let mut paint = Paint::default();
                        paint.set_anti_alias(true);
                        paint.set_style(skia_safe::paint::Style::Stroke);
                        paint.set_stroke_width(*stroke_width);
                        paint.set_color(skia_safe::Color::from_argb(stroke_color.a, stroke_color.r, stroke_color.g, stroke_color.b));
                        canvas.draw_circle((center.x, center.y), *radius, &paint);
                    }
                }
                DrawCommand::Line { from, to, color, width } => {
                    let mut paint = Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_stroke_width(*width);
                    paint.set_color(skia_safe::Color::from_argb(color.a, color.r, color.g, color.b));
                    canvas.draw_line((from.x, from.y), (to.x, to.y), &paint);
                }
                DrawCommand::FocusRing { bounds, corner_radii } => {
                    let expanded = Rect {
                        x: bounds.x - 2.0,
                        y: bounds.y - 2.0,
                        width: bounds.width + 4.0,
                        height: bounds.height + 4.0,
                    };
                    let radii = crate::style::CornerRadii {
                        top_left: corner_radii.top_left + 2.0,
                        top_right: corner_radii.top_right + 2.0,
                        bottom_right: corner_radii.bottom_right + 2.0,
                        bottom_left: corner_radii.bottom_left + 2.0,
                    };
                    let rrect = to_rrect(&expanded, &radii);
                    let mut paint = Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_style(skia_safe::paint::Style::Stroke);
                    paint.set_stroke_width(2.0);
                    paint.set_color(skia_safe::Color::from_argb(200, 66, 133, 244));
                    canvas.draw_rrect(rrect, &paint);
                }
                DrawCommand::RichText { spans, position, max_width, font_size, color,
                    font_family, font_weight, font_italic, line_height, text_align,
                    max_lines, text_overflow_ellipsis, letter_spacing, word_spacing, text_shadow,
                    font_features, font_variations } => {
                    self.draw_rich_text(canvas, spans, position, *max_width, *font_size, color,
                        font_family.as_deref(), *font_weight, *font_italic, *line_height,
                        *text_align, *max_lines, *text_overflow_ellipsis, *letter_spacing,
                        *word_spacing, text_shadow, font_features, font_variations);
                }
                DrawCommand::MultilineText { text, bounds, font_size, color,
                    font_family, font_weight, font_italic, line_height,
                    letter_spacing, word_spacing,
                    cursor_pos, selection_range, scroll_offset_y, scroll_offset_x,
                    show_line_numbers } => {
                    self.draw_multiline_text(canvas, text, bounds, *font_size, color,
                        font_family.as_deref(), *font_weight, *font_italic, *line_height,
                        *letter_spacing, *word_spacing,
                        *cursor_pos, *selection_range,
                        *scroll_offset_y, *scroll_offset_x, *show_line_numbers);
                }
            }
        }
    }

    fn draw_rect(&self, canvas: &Canvas, bounds: &Rect, bg: &Color, radii: &crate::style::CornerRadii, border: Option<&crate::style::Border>, border_style: crate::style::BorderStyle) {
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
            if border_style == crate::style::BorderStyle::None {
                return;
            }
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(b.width);
            paint.set_color(to_skia_color(&b.color));
            apply_border_style_to_paint(&mut paint, border_style, b.width);

            if border_style == crate::style::BorderStyle::Double && b.width >= 3.0 {
                // Double: two strokes at 1/3 width with 1/3 gap
                let third = b.width / 3.0;
                paint.set_stroke_width(third);
                paint.set_path_effect(None);
                // Outer stroke
                let outer = inset_rrect(bounds, radii, -third / 2.0);
                canvas.draw_rrect(outer, &paint);
                // Inner stroke
                let inner = inset_rrect(bounds, radii, b.width - third / 2.0);
                canvas.draw_rrect(inner, &paint);
            } else {
                canvas.draw_rrect(rrect, &paint);
            }
        }
    }

    fn draw_per_side_border(
        &self,
        canvas: &Canvas,
        bounds: &Rect,
        radii: &crate::style::CornerRadii,
        full_border: &crate::style::FullBorder,
    ) {
        // Draw each side individually using clip regions + draw_drrect approach
        let sides = [
            (&full_border.top, Side::Top),
            (&full_border.right, Side::Right),
            (&full_border.bottom, Side::Bottom),
            (&full_border.left, Side::Left),
        ];

        for (side_opt, side) in &sides {
            if let Some(border_side) = side_opt {
                if border_side.width <= 0.0 || border_side.style == crate::style::BorderStyle::None {
                    continue;
                }

                canvas.save();

                // Clip to the side's quadrant (diagonal split at corners)
                let cx = bounds.x + bounds.width / 2.0;
                let cy = bounds.y + bounds.height / 2.0;
                let (x0, y0) = (bounds.x, bounds.y);
                let (x1, y1) = (bounds.x + bounds.width, bounds.y + bounds.height);

                let mut clip_path = skia_safe::Path::new();
                match side {
                    Side::Top => {
                        clip_path.move_to((x0, y0));
                        clip_path.line_to((x1, y0));
                        clip_path.line_to((cx, cy));
                        clip_path.close();
                    }
                    Side::Right => {
                        clip_path.move_to((x1, y0));
                        clip_path.line_to((x1, y1));
                        clip_path.line_to((cx, cy));
                        clip_path.close();
                    }
                    Side::Bottom => {
                        clip_path.move_to((x1, y1));
                        clip_path.line_to((x0, y1));
                        clip_path.line_to((cx, cy));
                        clip_path.close();
                    }
                    Side::Left => {
                        clip_path.move_to((x0, y1));
                        clip_path.line_to((x0, y0));
                        clip_path.line_to((cx, cy));
                        clip_path.close();
                    }
                }
                canvas.clip_path(&clip_path, ClipOp::Intersect, true);

                // Draw the border ring for this side
                let outer_rrect = to_rrect(bounds, radii);
                let inner_bounds = Rect {
                    x: bounds.x + full_border.left.as_ref().map_or(0.0, |s| s.width),
                    y: bounds.y + full_border.top.as_ref().map_or(0.0, |s| s.width),
                    width: bounds.width
                        - full_border.left.as_ref().map_or(0.0, |s| s.width)
                        - full_border.right.as_ref().map_or(0.0, |s| s.width),
                    height: bounds.height
                        - full_border.top.as_ref().map_or(0.0, |s| s.width)
                        - full_border.bottom.as_ref().map_or(0.0, |s| s.width),
                };
                let inner_radii = crate::style::CornerRadii {
                    top_left: (radii.top_left - border_side.width).max(0.0),
                    top_right: (radii.top_right - border_side.width).max(0.0),
                    bottom_right: (radii.bottom_right - border_side.width).max(0.0),
                    bottom_left: (radii.bottom_left - border_side.width).max(0.0),
                };
                let inner_rrect = to_rrect(&inner_bounds, &inner_radii);

                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_color(to_skia_color(&border_side.color));
                canvas.draw_drrect(outer_rrect, inner_rrect, &paint);

                canvas.restore();
            }
        }
    }

    fn draw_outline(
        &self,
        canvas: &Canvas,
        bounds: &Rect,
        radii: &crate::style::CornerRadii,
        outline: &crate::style::Outline,
    ) {
        let half_w = outline.width / 2.0;
        let expand = outline.offset + half_w;
        let outline_bounds = Rect {
            x: bounds.x - expand,
            y: bounds.y - expand,
            width: bounds.width + expand * 2.0,
            height: bounds.height + expand * 2.0,
        };
        let outline_radii = crate::style::CornerRadii {
            top_left: (radii.top_left + outline.offset).max(0.0),
            top_right: (radii.top_right + outline.offset).max(0.0),
            bottom_right: (radii.bottom_right + outline.offset).max(0.0),
            bottom_left: (radii.bottom_left + outline.offset).max(0.0),
        };
        let rrect = to_rrect(&outline_bounds, &outline_radii);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(outline.width);
        paint.set_color(to_skia_color(&outline.color));
        apply_border_style_to_paint(&mut paint, outline.style, outline.width);
        canvas.draw_rrect(rrect, &paint);
    }

    fn draw_path(
        &self,
        canvas: &Canvas,
        data: &crate::element::ShapeData,
        bounds: &Rect,
    ) {
        if let Some(mut path) = parse_svg_path(&data.path_data) {
            // Scale path from viewbox to element bounds
            let path_bounds = path.bounds();
            let (vb_w, vb_h) = data.viewbox.unwrap_or((path_bounds.width(), path_bounds.height()));
            if vb_w > 0.0 && vb_h > 0.0 {
                let sx = bounds.width / vb_w;
                let sy = bounds.height / vb_h;
                let mut matrix = skia_safe::Matrix::new_identity();
                matrix.set_scale_translate((sx, sy), (bounds.x, bounds.y));
                path.transform(&matrix);
            }

            // Fill
            if let Some(fill) = &data.fill {
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_color(to_skia_color(fill));
                canvas.draw_path(&path, &paint);
            }

            // Stroke
            if let Some((color, width)) = &data.stroke {
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(*width);
                paint.set_color(to_skia_color(color));
                canvas.draw_path(&path, &paint);
            }
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
        font_features: &[(String, i32)],
        font_variations: &[(String, f32)],
        cursor_byte_offset: Option<usize>,
        selection_byte_range: Option<(usize, usize)>,
        scroll_offset: f32,
        preedit_byte_range: Option<(usize, usize)>,
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

        // Font features (OpenType)
        for (tag, value) in font_features {
            text_style.add_font_feature(tag, *value);
        }

        // Variable font axes
        if !font_variations.is_empty() {
            let coords: Vec<skia_safe::font_arguments::variation_position::Coordinate> =
                font_variations.iter().map(|(axis, value)| {
                    let bytes = axis.as_bytes();
                    let tag = skia_safe::FourByteTag::from_chars(
                        bytes.get(0).copied().unwrap_or(b' ') as char,
                        bytes.get(1).copied().unwrap_or(b' ') as char,
                        bytes.get(2).copied().unwrap_or(b' ') as char,
                        bytes.get(3).copied().unwrap_or(b' ') as char,
                    );
                    skia_safe::font_arguments::variation_position::Coordinate {
                        axis: tag,
                        value: *value,
                    }
                }).collect();
            let fa = skia_safe::FontArguments::new()
                .set_variation_design_position(skia_safe::font_arguments::VariationPosition {
                    coordinates: &coords,
                });
            text_style.set_font_arguments(&fa);
        }

        para_style.set_text_style(&text_style);

        // Build and layout paragraph
        let mut builder = ParagraphBuilder::new(&para_style, self.font_collection.clone());
        builder.push_style(&text_style);
        builder.add_text(text);
        let mut paragraph = builder.build();
        paragraph.layout(max_width);

        let has_input_state = cursor_byte_offset.is_some() || selection_byte_range.is_some() || scroll_offset != 0.0 || preedit_byte_range.is_some();

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

            // Draw preedit underline (IME composition indicator)
            if let Some((preedit_start, preedit_end)) = preedit_byte_range {
                if preedit_start < preedit_end && preedit_end <= text.len() {
                    let rects = paragraph.get_rects_for_range(
                        preedit_start..preedit_end,
                        RectHeightStyle::Tight,
                        RectWidthStyle::Tight,
                    );
                    let mut underline_paint = Paint::default();
                    underline_paint.set_color(to_skia_color(color));
                    underline_paint.set_anti_alias(true);
                    underline_paint.set_stroke_width(1.0);
                    underline_paint.set_style(skia_safe::paint::Style::Stroke);
                    for text_box in &rects {
                        let r = text_box.rect;
                        let y = text_y + r.bottom - 1.0;
                        canvas.draw_line(
                            (text_x + r.left, y),
                            (text_x + r.right, y),
                            &underline_paint,
                        );
                    }
                }
            }

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
        font_features: &[(String, i32)],
        font_variations: &[(String, f32)],
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
        // Font features (OpenType) on base style
        for (tag, value) in font_features {
            base_style.add_font_feature(tag, *value);
        }
        // Variable font axes on base style
        if !font_variations.is_empty() {
            let coords: Vec<skia_safe::font_arguments::variation_position::Coordinate> =
                font_variations.iter().map(|(axis, value)| {
                    let bytes = axis.as_bytes();
                    let tag = skia_safe::FourByteTag::from_chars(
                        bytes.get(0).copied().unwrap_or(b' ') as char,
                        bytes.get(1).copied().unwrap_or(b' ') as char,
                        bytes.get(2).copied().unwrap_or(b' ') as char,
                        bytes.get(3).copied().unwrap_or(b' ') as char,
                    );
                    skia_safe::font_arguments::variation_position::Coordinate {
                        axis: tag,
                        value: *value,
                    }
                }).collect();
            let fa = skia_safe::FontArguments::new()
                .set_variation_design_position(skia_safe::font_arguments::VariationPosition {
                    coordinates: &coords,
                });
            base_style.set_font_arguments(&fa);
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

            // Per-span font features
            for (tag, value) in &span.font_features {
                style.add_font_feature(tag, *value);
            }
            // Per-span variable font axes
            if !span.font_variations.is_empty() {
                let coords: Vec<skia_safe::font_arguments::variation_position::Coordinate> =
                    span.font_variations.iter().map(|(axis, value)| {
                        let bytes = axis.as_bytes();
                        let tag = skia_safe::FourByteTag::from_chars(
                            bytes.get(0).copied().unwrap_or(b' ') as char,
                            bytes.get(1).copied().unwrap_or(b' ') as char,
                            bytes.get(2).copied().unwrap_or(b' ') as char,
                            bytes.get(3).copied().unwrap_or(b' ') as char,
                        );
                        skia_safe::font_arguments::variation_position::Coordinate {
                            axis: tag,
                            value: *value,
                        }
                    }).collect();
                let fa = skia_safe::FontArguments::new()
                    .set_variation_design_position(skia_safe::font_arguments::VariationPosition {
                        coordinates: &coords,
                    });
                style.set_font_arguments(&fa);
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

    fn draw_image(
        &mut self,
        canvas: &Canvas,
        source: &ImageSource,
        bounds: &Rect,
        tint: Option<&Color>,
        image_fit: crate::element::ImageFit,
    ) {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let tw = bounds.width as u32;
        let th = bounds.height as u32;

        // Handle vector SVG separately — resolution-independent, no rasterization
        if let ImageSource::VectorSvg(data) = source {
            let mut hasher = DefaultHasher::new();
            2u8.hash(&mut hasher);
            data.hash(&mut hasher);
            let key = hasher.finish();

            if !self.vector_svg_cache.contains_key(&key) {
                let svg_data = if let Some(path) = data.strip_prefix("file:") {
                    std::fs::read(path).ok()
                } else {
                    Some(data.as_bytes().to_vec())
                };
                if let Some(bytes) = svg_data {
                    if let Some(vector) = crate::svg_render::VectorSvg::from_data(&bytes) {
                        self.vector_svg_cache.insert(key, vector);
                    }
                }
            }

            if let Some(vector) = self.vector_svg_cache.get(&key) {
                vector.draw_fit(canvas, bounds, tint, image_fit);
            }
            return;
        }

        // Hash-based cache key including dimensions
        let cache_key = {
            let mut hasher = DefaultHasher::new();
            match source {
                ImageSource::Svg(s) => { 0u8.hash(&mut hasher); s.hash(&mut hasher); }
                ImageSource::File(p) => { 1u8.hash(&mut hasher); p.hash(&mut hasher); }
                ImageSource::VectorSvg(_) => unreachable!(),
            }
            tw.hash(&mut hasher);
            th.hash(&mut hasher);
            hasher.finish()
        };

        if !self.image_cache.contains_key(&cache_key) {
            let img = match source {
                ImageSource::Svg(data) => load_svg(data.as_bytes(), (tw, th)),
                ImageSource::File(path) => load_image_file(path, (tw, th)),
            };
            if let Some(img) = img {
                let byte_size = (img.width() * img.height() * 4) as usize;
                // LRU eviction
                while self.cache_total_bytes + byte_size > self.cache_max_bytes && !self.image_cache.is_empty() {
                    let oldest_key = self.image_cache.iter()
                        .min_by_key(|(_, e)| e.last_used)
                        .map(|(k, _)| *k);
                    if let Some(key) = oldest_key {
                        if let Some(evicted) = self.image_cache.remove(&key) {
                            self.cache_total_bytes -= evicted.byte_size;
                        }
                    }
                }
                self.cache_total_bytes += byte_size;
                self.image_cache.insert(cache_key, CacheEntry {
                    image: img,
                    last_used: std::time::Instant::now(),
                    byte_size,
                });
            }
        }

        if let Some(entry) = self.image_cache.get_mut(&cache_key) {
            entry.last_used = std::time::Instant::now();
            let img = &entry.image;

            // Compute destination rect based on image_fit
            let dst = compute_image_fit_rect(
                img.width() as f32,
                img.height() as f32,
                bounds,
                image_fit,
            );

            let mut paint = Paint::default();
            // Apply tint via SrcIn color filter (tints opaque areas)
            if let Some(tint_color) = tint {
                if let Some(cf) = skia_safe::color_filters::blend(
                    to_skia_color(tint_color),
                    skia_safe::BlendMode::SrcIn,
                ) {
                    paint.set_color_filter(cf);
                }
            }
            canvas.draw_image_rect(img, None, dst, &paint);
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

    fn draw_backdrop_filter(
        &self,
        canvas: &Canvas,
        bounds: &Rect,
        radii: &crate::style::CornerRadii,
        filters: &[crate::style::Filter],
    ) {
        if let Some(filter) = build_image_filter(filters) {
            canvas.save();
            let rrect = to_rrect(bounds, radii);
            canvas.clip_rrect(rrect, ClipOp::Intersect, true);
            let mut paint = Paint::default();
            paint.set_image_filter(filter);
            let rec = SaveLayerRec::default().paint(&paint);
            canvas.save_layer(&rec);
            canvas.restore();
            canvas.restore();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_multiline_text(
        &mut self,
        canvas: &Canvas,
        text: &str,
        bounds: &Rect,
        font_size: f32,
        color: &Color,
        font_family: Option<&[String]>,
        font_weight: Option<i32>,
        font_italic: bool,
        line_height: Option<f32>,
        letter_spacing: f32,
        word_spacing: f32,
        cursor_pos: Option<(usize, usize)>,
        selection_range: Option<((usize, usize), (usize, usize))>,
        scroll_offset_y: f32,
        scroll_offset_x: f32,
        show_line_numbers: bool,
    ) {
        use skia_safe::font_style::{Weight, Width, Slant};

        let padding = 8.0;
        let line_num_width = if show_line_numbers {
            let num_lines = text.split('\n').count();
            let digits = format!("{}", num_lines).len() as f32;
            digits * font_size * 0.6 + 16.0
        } else {
            0.0
        };

        // Clip to bounds
        canvas.save();
        canvas.clip_rect(
            skia_safe::Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height),
            ClipOp::Intersect,
            true,
        );

        // Compute line height in pixels
        let lh = line_height.unwrap_or(1.4) * font_size;
        let lines: Vec<&str> = text.split('\n').collect();

        // Calculate visible range
        let first_visible = (scroll_offset_y / lh).floor() as usize;
        let visible_count = (bounds.height / lh).ceil() as usize + 2;
        let last_visible = (first_visible + visible_count).min(lines.len());

        let content_x = bounds.x + padding + line_num_width - scroll_offset_x;
        let content_y = bounds.y + padding - scroll_offset_y;
        let text_area_width = bounds.width - padding * 2.0 - line_num_width;

        // Draw line numbers
        if show_line_numbers {
            let mut num_paint = Paint::default();
            num_paint.set_anti_alias(true);
            let dim_color = Color { r: color.r, g: color.g, b: color.b, a: (color.a as f32 * 0.4) as u8 };

            for i in first_visible..last_visible {
                let y = content_y + i as f32 * lh;
                if y + lh < bounds.y || y > bounds.y + bounds.height {
                    continue;
                }
                let num_str = format!("{}", i + 1);
                let mut para_style = ParagraphStyle::new();
                para_style.set_text_align(SkTextAlign::Right);
                let mut ts = TextStyle::new();
                ts.set_font_size(font_size);
                ts.set_color(to_skia_color(&dim_color));
                if let Some(families) = font_family {
                    ts.set_font_families(families);
                }
                let weight = font_weight.map(|w| Weight::from(w)).unwrap_or(Weight::NORMAL);
                let slant = if font_italic { Slant::Italic } else { Slant::Upright };
                ts.set_font_style(FontStyle::new(weight, Width::NORMAL, slant));
                para_style.set_text_style(&ts);
                let mut builder = ParagraphBuilder::new(&para_style, self.font_collection.clone());
                builder.push_style(&ts);
                builder.add_text(&num_str);
                let mut para = builder.build();
                para.layout(line_num_width - 8.0);
                para.paint(canvas, (bounds.x + padding, y));
            }

            // Draw separator line
            let mut sep_paint = Paint::default();
            sep_paint.set_color(skia_safe::Color::from_argb(40, color.r, color.g, color.b));
            sep_paint.set_stroke_width(1.0);
            let sep_x = bounds.x + padding + line_num_width - 4.0;
            canvas.draw_line(
                (sep_x, bounds.y),
                (sep_x, bounds.y + bounds.height),
                &sep_paint,
            );
        }

        // Draw selection highlight
        if let Some((start, end)) = selection_range {
            let mut sel_paint = Paint::default();
            sel_paint.set_color(skia_safe::Color::from_argb(80, 100, 150, 255));
            sel_paint.set_anti_alias(true);

            for line_idx in start.0..=end.0 {
                if line_idx < first_visible || line_idx >= last_visible {
                    continue;
                }
                let line_text = lines.get(line_idx).unwrap_or(&"");
                let line_chars = line_text.chars().count();
                let sel_start_col = if line_idx == start.0 { start.1 } else { 0 };
                let sel_end_col = if line_idx == end.0 { end.1 } else { line_chars };

                if sel_start_col == sel_end_col {
                    continue;
                }

                // Build a paragraph for this line to measure character positions
                let (x_start, x_end) = self.measure_char_range_in_line(
                    line_text, sel_start_col, sel_end_col,
                    font_size, font_family, font_weight, font_italic,
                    letter_spacing, word_spacing, text_area_width,
                );

                let y = content_y + line_idx as f32 * lh;
                canvas.draw_rect(
                    skia_safe::Rect::from_xywh(content_x + x_start, y, x_end - x_start, lh),
                    &sel_paint,
                );
            }
        }

        // Draw text lines
        for i in first_visible..last_visible {
            let y = content_y + i as f32 * lh;
            if y + lh < bounds.y || y > bounds.y + bounds.height {
                continue;
            }
            let line_text = lines.get(i).unwrap_or(&"");
            if line_text.is_empty() {
                continue;
            }

            let mut para_style = ParagraphStyle::new();
            para_style.set_max_lines(1);

            let mut ts = TextStyle::new();
            ts.set_font_size(font_size);
            ts.set_color(to_skia_color(color));
            if let Some(families) = font_family {
                ts.set_font_families(families);
            }
            let weight = font_weight.map(|w| Weight::from(w)).unwrap_or(Weight::NORMAL);
            let slant = if font_italic { Slant::Italic } else { Slant::Upright };
            ts.set_font_style(FontStyle::new(weight, Width::NORMAL, slant));
            if let Some(lh_val) = line_height {
                ts.set_height(lh_val);
                ts.set_height_override(true);
            }
            if letter_spacing != 0.0 {
                ts.set_letter_spacing(letter_spacing);
            }
            if word_spacing != 0.0 {
                ts.set_word_spacing(word_spacing);
            }
            para_style.set_text_style(&ts);

            let mut builder = ParagraphBuilder::new(&para_style, self.font_collection.clone());
            builder.push_style(&ts);
            builder.add_text(line_text);
            let mut para = builder.build();
            para.layout(text_area_width);
            para.paint(canvas, (content_x, y));
        }

        // Draw cursor
        if let Some((cline, ccol)) = cursor_pos {
            let y = content_y + cline as f32 * lh;
            let line_text = lines.get(cline).unwrap_or(&"");
            let cursor_x = if ccol == 0 || line_text.is_empty() {
                0.0
            } else {
                let (_, x) = self.measure_char_range_in_line(
                    line_text, 0, ccol.min(line_text.chars().count()),
                    font_size, font_family, font_weight, font_italic,
                    letter_spacing, word_spacing, text_area_width,
                );
                x
            };

            let mut cursor_paint = Paint::default();
            cursor_paint.set_color(to_skia_color(color));
            cursor_paint.set_anti_alias(true);
            canvas.draw_rect(
                skia_safe::Rect::from_xywh(content_x + cursor_x, y, 1.5, lh),
                &cursor_paint,
            );
        }

        canvas.restore();
    }

    /// Measure the x-range of characters [start_col..end_col] in a line of text.
    /// Returns (x_start, x_end) in pixels relative to line start.
    fn measure_char_range_in_line(
        &mut self,
        line_text: &str,
        start_col: usize,
        end_col: usize,
        font_size: f32,
        font_family: Option<&[String]>,
        font_weight: Option<i32>,
        font_italic: bool,
        letter_spacing: f32,
        word_spacing: f32,
        max_width: f32,
    ) -> (f32, f32) {
        use skia_safe::font_style::{Weight, Width, Slant};
        use skia_safe::textlayout::RectHeightStyle;
        use skia_safe::textlayout::RectWidthStyle;

        let mut para_style = ParagraphStyle::new();
        para_style.set_max_lines(1);
        let mut ts = TextStyle::new();
        ts.set_font_size(font_size);
        if let Some(families) = font_family {
            ts.set_font_families(families);
        }
        let weight = font_weight.map(|w| Weight::from(w)).unwrap_or(Weight::NORMAL);
        let slant = if font_italic { Slant::Italic } else { Slant::Upright };
        ts.set_font_style(FontStyle::new(weight, Width::NORMAL, slant));
        if letter_spacing != 0.0 { ts.set_letter_spacing(letter_spacing); }
        if word_spacing != 0.0 { ts.set_word_spacing(word_spacing); }
        para_style.set_text_style(&ts);

        let mut builder = ParagraphBuilder::new(&para_style, self.font_collection.clone());
        builder.push_style(&ts);
        builder.add_text(line_text);
        let mut para = builder.build();
        para.layout(max_width);

        // Convert char columns to byte offsets
        let start_byte = line_text.char_indices().nth(start_col).map(|(i, _)| i).unwrap_or(line_text.len());
        let end_byte = line_text.char_indices().nth(end_col).map(|(i, _)| i).unwrap_or(line_text.len());

        let x_start = if start_col == 0 {
            0.0
        } else {
            let rects = para.get_rects_for_range(0..start_byte, RectHeightStyle::Tight, RectWidthStyle::Tight);
            rects.last().map(|r| r.rect.right).unwrap_or(0.0)
        };

        let x_end = if end_col == 0 {
            0.0
        } else {
            let rects = para.get_rects_for_range(0..end_byte, RectHeightStyle::Tight, RectWidthStyle::Tight);
            rects.last().map(|r| r.rect.right).unwrap_or(0.0)
        };

        (x_start, x_end)
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

enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

fn apply_border_style_to_paint(paint: &mut Paint, style: crate::style::BorderStyle, width: f32) {
    match style {
        crate::style::BorderStyle::Dashed => {
            let dash_len = (3.0 * width).max(3.0);
            if let Some(effect) = PathEffect::dash(&[dash_len, dash_len], 0.0) {
                paint.set_path_effect(effect);
            }
        }
        crate::style::BorderStyle::Dotted => {
            let gap = (2.0 * width).max(2.0);
            paint.set_stroke_cap(skia_safe::PaintCap::Round);
            if let Some(effect) = PathEffect::dash(&[0.01, gap], 0.0) {
                paint.set_path_effect(effect);
            }
        }
        _ => {} // Solid, Double, None handled elsewhere
    }
}

fn inset_rrect(bounds: &Rect, radii: &crate::style::CornerRadii, inset: f32) -> RRect {
    let inset_bounds = Rect {
        x: bounds.x + inset,
        y: bounds.y + inset,
        width: (bounds.width - inset * 2.0).max(0.0),
        height: (bounds.height - inset * 2.0).max(0.0),
    };
    let inset_radii = crate::style::CornerRadii {
        top_left: (radii.top_left - inset).max(0.0),
        top_right: (radii.top_right - inset).max(0.0),
        bottom_right: (radii.bottom_right - inset).max(0.0),
        bottom_left: (radii.bottom_left - inset).max(0.0),
    };
    to_rrect(&inset_bounds, &inset_radii)
}

/// Build a chained Skia ImageFilter from a list of CSS filter values.
fn build_image_filter(filters: &[crate::style::Filter]) -> Option<skia_safe::ImageFilter> {
    use skia_safe::color_filters;

    let mut current: Option<skia_safe::ImageFilter> = None;

    for f in filters {
        current = match f {
            crate::style::Filter::Blur(radius) => {
                image_filters::blur((*radius, *radius), None, current, None)
            }
            crate::style::Filter::Brightness(amount) => {
                let b = *amount;
                let matrix: [f32; 20] = [
                    b, 0.0, 0.0, 0.0, 0.0,
                    0.0, b, 0.0, 0.0, 0.0,
                    0.0, 0.0, b, 0.0, 0.0,
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ];
                let cf = color_filters::matrix_row_major(&matrix);
                cf.and_then(|cf| image_filters::color_filter(cf, current, None))
            }
            crate::style::Filter::Contrast(amount) => {
                let c = *amount;
                let t = (1.0 - c) / 2.0 * 255.0;
                let matrix: [f32; 20] = [
                    c, 0.0, 0.0, 0.0, t,
                    0.0, c, 0.0, 0.0, t,
                    0.0, 0.0, c, 0.0, t,
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ];
                let cf = color_filters::matrix_row_major(&matrix);
                cf.and_then(|cf| image_filters::color_filter(cf, current, None))
            }
            crate::style::Filter::Grayscale(amount) => {
                let s = 1.0 - amount.clamp(0.0, 1.0);
                let matrix: [f32; 20] = [
                    0.2126 + 0.7874 * s, 0.7152 - 0.7152 * s, 0.0722 - 0.0722 * s, 0.0, 0.0,
                    0.2126 - 0.2126 * s, 0.7152 + 0.2848 * s, 0.0722 - 0.0722 * s, 0.0, 0.0,
                    0.2126 - 0.2126 * s, 0.7152 - 0.7152 * s, 0.0722 + 0.9278 * s, 0.0, 0.0,
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ];
                let cf = color_filters::matrix_row_major(&matrix);
                cf.and_then(|cf| image_filters::color_filter(cf, current, None))
            }
            crate::style::Filter::Sepia(amount) => {
                let s = 1.0 - amount.clamp(0.0, 1.0);
                let matrix: [f32; 20] = [
                    0.393 + 0.607 * s, 0.769 - 0.769 * s, 0.189 - 0.189 * s, 0.0, 0.0,
                    0.349 - 0.349 * s, 0.686 + 0.314 * s, 0.168 - 0.168 * s, 0.0, 0.0,
                    0.272 - 0.272 * s, 0.534 - 0.534 * s, 0.131 + 0.869 * s, 0.0, 0.0,
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ];
                let cf = color_filters::matrix_row_major(&matrix);
                cf.and_then(|cf| image_filters::color_filter(cf, current, None))
            }
            crate::style::Filter::HueRotate(degrees) => {
                let a = degrees.to_radians();
                let cos_a = a.cos();
                let sin_a = a.sin();
                let matrix: [f32; 20] = [
                    0.213 + 0.787 * cos_a - 0.213 * sin_a,
                    0.715 - 0.715 * cos_a - 0.715 * sin_a,
                    0.072 - 0.072 * cos_a + 0.928 * sin_a,
                    0.0, 0.0,
                    0.213 - 0.213 * cos_a + 0.143 * sin_a,
                    0.715 + 0.285 * cos_a + 0.140 * sin_a,
                    0.072 - 0.072 * cos_a - 0.283 * sin_a,
                    0.0, 0.0,
                    0.213 - 0.213 * cos_a - 0.787 * sin_a,
                    0.715 - 0.715 * cos_a + 0.715 * sin_a,
                    0.072 + 0.928 * cos_a + 0.072 * sin_a,
                    0.0, 0.0,
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ];
                let cf = color_filters::matrix_row_major(&matrix);
                cf.and_then(|cf| image_filters::color_filter(cf, current, None))
            }
            crate::style::Filter::Invert(amount) => {
                let i = *amount;
                let matrix: [f32; 20] = [
                    1.0 - 2.0 * i, 0.0, 0.0, 0.0, i * 255.0,
                    0.0, 1.0 - 2.0 * i, 0.0, 0.0, i * 255.0,
                    0.0, 0.0, 1.0 - 2.0 * i, 0.0, i * 255.0,
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ];
                let cf = color_filters::matrix_row_major(&matrix);
                cf.and_then(|cf| image_filters::color_filter(cf, current, None))
            }
            crate::style::Filter::Opacity(amount) => {
                let o = *amount;
                let matrix: [f32; 20] = [
                    1.0, 0.0, 0.0, 0.0, 0.0,
                    0.0, 1.0, 0.0, 0.0, 0.0,
                    0.0, 0.0, 1.0, 0.0, 0.0,
                    0.0, 0.0, 0.0, o, 0.0,
                ];
                let cf = color_filters::matrix_row_major(&matrix);
                cf.and_then(|cf| image_filters::color_filter(cf, current, None))
            }
            crate::style::Filter::Saturate(amount) => {
                let s = *amount;
                let matrix: [f32; 20] = [
                    0.2126 + 0.7874 * s, 0.7152 - 0.7152 * s, 0.0722 - 0.0722 * s, 0.0, 0.0,
                    0.2126 - 0.2126 * s, 0.7152 + 0.2848 * s, 0.0722 - 0.0722 * s, 0.0, 0.0,
                    0.2126 - 0.2126 * s, 0.7152 - 0.7152 * s, 0.0722 + 0.9278 * s, 0.0, 0.0,
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ];
                let cf = color_filters::matrix_row_major(&matrix);
                cf.and_then(|cf| image_filters::color_filter(cf, current, None))
            }
            crate::style::Filter::DropShadow { x, y, blur, color } => {
                image_filters::drop_shadow(
                    (*x, *y),
                    (*blur, *blur),
                    to_skia_color(color),
                    current,
                    None,
                )
            }
        };
    }

    current
}

fn compute_image_fit_rect(
    img_w: f32,
    img_h: f32,
    bounds: &Rect,
    fit: crate::element::ImageFit,
) -> skia_safe::Rect {
    match fit {
        crate::element::ImageFit::Fill => {
            skia_safe::Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height)
        }
        crate::element::ImageFit::Contain | crate::element::ImageFit::ScaleDown => {
            let scale_x = bounds.width / img_w;
            let scale_y = bounds.height / img_h;
            let mut scale = scale_x.min(scale_y);
            if fit == crate::element::ImageFit::ScaleDown {
                scale = scale.min(1.0);
            }
            let w = img_w * scale;
            let h = img_h * scale;
            let x = bounds.x + (bounds.width - w) / 2.0;
            let y = bounds.y + (bounds.height - h) / 2.0;
            skia_safe::Rect::from_xywh(x, y, w, h)
        }
        crate::element::ImageFit::Cover => {
            let scale_x = bounds.width / img_w;
            let scale_y = bounds.height / img_h;
            let scale = scale_x.max(scale_y);
            let w = img_w * scale;
            let h = img_h * scale;
            let x = bounds.x + (bounds.width - w) / 2.0;
            let y = bounds.y + (bounds.height - h) / 2.0;
            skia_safe::Rect::from_xywh(x, y, w, h)
        }
    }
}

fn to_skia_blend_mode(mode: crate::style::BlendMode) -> skia_safe::BlendMode {
    match mode {
        crate::style::BlendMode::Normal => skia_safe::BlendMode::SrcOver,
        crate::style::BlendMode::Multiply => skia_safe::BlendMode::Multiply,
        crate::style::BlendMode::Screen => skia_safe::BlendMode::Screen,
        crate::style::BlendMode::Overlay => skia_safe::BlendMode::Overlay,
        crate::style::BlendMode::Darken => skia_safe::BlendMode::Darken,
        crate::style::BlendMode::Lighten => skia_safe::BlendMode::Lighten,
        crate::style::BlendMode::ColorDodge => skia_safe::BlendMode::ColorDodge,
        crate::style::BlendMode::ColorBurn => skia_safe::BlendMode::ColorBurn,
        crate::style::BlendMode::HardLight => skia_safe::BlendMode::HardLight,
        crate::style::BlendMode::SoftLight => skia_safe::BlendMode::SoftLight,
        crate::style::BlendMode::Difference => skia_safe::BlendMode::Difference,
        crate::style::BlendMode::Exclusion => skia_safe::BlendMode::Exclusion,
        crate::style::BlendMode::Hue => skia_safe::BlendMode::Hue,
        crate::style::BlendMode::Saturation => skia_safe::BlendMode::Saturation,
        crate::style::BlendMode::Color => skia_safe::BlendMode::Color,
        crate::style::BlendMode::Luminosity => skia_safe::BlendMode::Luminosity,
    }
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

/// Parse an SVG path `d` attribute string into a Skia Path.
fn parse_svg_path(d: &str) -> Option<skia_safe::Path> {
    let mut path = skia_safe::Path::new();
    let mut chars = d.chars().peekable();
    let mut current_x = 0.0f32;
    let mut current_y = 0.0f32;
    let mut last_cmd = ' ';

    fn skip_ws_comma(chars: &mut std::iter::Peekable<std::str::Chars>) {
        while let Some(&c) = chars.peek() {
            if c == ' ' || c == ',' || c == '\t' || c == '\n' || c == '\r' {
                chars.next();
            } else {
                break;
            }
        }
    }

    fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<f32> {
        skip_ws_comma(chars);
        let mut s = String::new();
        // Handle sign
        if let Some(&c) = chars.peek() {
            if c == '-' || c == '+' {
                s.push(c);
                chars.next();
            }
        }
        let mut has_dot = false;
        let mut has_e = false;
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                chars.next();
            } else if c == '.' && !has_dot && !has_e {
                has_dot = true;
                s.push(c);
                chars.next();
            } else if (c == 'e' || c == 'E') && !has_e {
                has_e = true;
                s.push(c);
                chars.next();
                if let Some(&c2) = chars.peek() {
                    if c2 == '-' || c2 == '+' {
                        s.push(c2);
                        chars.next();
                    }
                }
            } else {
                break;
            }
        }
        if s.is_empty() || s == "-" || s == "+" {
            None
        } else {
            s.parse().ok()
        }
    }

    while chars.peek().is_some() {
        skip_ws_comma(&mut chars);
        let cmd = if let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() {
                chars.next();
                c
            } else {
                // Implicit repeat of last command
                last_cmd
            }
        } else {
            break;
        };

        match cmd {
            'M' => {
                if let (Some(x), Some(y)) = (parse_number(&mut chars), parse_number(&mut chars)) {
                    path.move_to((x, y));
                    current_x = x;
                    current_y = y;
                    // Subsequent coords are implicit LineTo
                    while { skip_ws_comma(&mut chars); chars.peek().map_or(false, |c| c.is_ascii_digit() || *c == '-' || *c == '+' || *c == '.') } {
                        if let (Some(x), Some(y)) = (parse_number(&mut chars), parse_number(&mut chars)) {
                            path.line_to((x, y));
                            current_x = x;
                            current_y = y;
                        }
                    }
                }
            }
            'm' => {
                if let (Some(dx), Some(dy)) = (parse_number(&mut chars), parse_number(&mut chars)) {
                    current_x += dx;
                    current_y += dy;
                    path.move_to((current_x, current_y));
                    while { skip_ws_comma(&mut chars); chars.peek().map_or(false, |c| c.is_ascii_digit() || *c == '-' || *c == '+' || *c == '.') } {
                        if let (Some(dx), Some(dy)) = (parse_number(&mut chars), parse_number(&mut chars)) {
                            current_x += dx;
                            current_y += dy;
                            path.line_to((current_x, current_y));
                        }
                    }
                }
            }
            'L' => {
                while { skip_ws_comma(&mut chars); chars.peek().map_or(false, |c| c.is_ascii_digit() || *c == '-' || *c == '+' || *c == '.') } {
                    if let (Some(x), Some(y)) = (parse_number(&mut chars), parse_number(&mut chars)) {
                        path.line_to((x, y));
                        current_x = x;
                        current_y = y;
                    }
                }
            }
            'l' => {
                while { skip_ws_comma(&mut chars); chars.peek().map_or(false, |c| c.is_ascii_digit() || *c == '-' || *c == '+' || *c == '.') } {
                    if let (Some(dx), Some(dy)) = (parse_number(&mut chars), parse_number(&mut chars)) {
                        current_x += dx;
                        current_y += dy;
                        path.line_to((current_x, current_y));
                    }
                }
            }
            'H' => {
                if let Some(x) = parse_number(&mut chars) {
                    current_x = x;
                    path.line_to((current_x, current_y));
                }
            }
            'h' => {
                if let Some(dx) = parse_number(&mut chars) {
                    current_x += dx;
                    path.line_to((current_x, current_y));
                }
            }
            'V' => {
                if let Some(y) = parse_number(&mut chars) {
                    current_y = y;
                    path.line_to((current_x, current_y));
                }
            }
            'v' => {
                if let Some(dy) = parse_number(&mut chars) {
                    current_y += dy;
                    path.line_to((current_x, current_y));
                }
            }
            'C' => {
                while { skip_ws_comma(&mut chars); chars.peek().map_or(false, |c| c.is_ascii_digit() || *c == '-' || *c == '+' || *c == '.') } {
                    if let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                        parse_number(&mut chars), parse_number(&mut chars),
                        parse_number(&mut chars), parse_number(&mut chars),
                        parse_number(&mut chars), parse_number(&mut chars),
                    ) {
                        path.cubic_to((x1, y1), (x2, y2), (x, y));
                        current_x = x;
                        current_y = y;
                    }
                }
            }
            'c' => {
                while { skip_ws_comma(&mut chars); chars.peek().map_or(false, |c| c.is_ascii_digit() || *c == '-' || *c == '+' || *c == '.') } {
                    if let (Some(dx1), Some(dy1), Some(dx2), Some(dy2), Some(dx), Some(dy)) = (
                        parse_number(&mut chars), parse_number(&mut chars),
                        parse_number(&mut chars), parse_number(&mut chars),
                        parse_number(&mut chars), parse_number(&mut chars),
                    ) {
                        path.cubic_to(
                            (current_x + dx1, current_y + dy1),
                            (current_x + dx2, current_y + dy2),
                            (current_x + dx, current_y + dy),
                        );
                        current_x += dx;
                        current_y += dy;
                    }
                }
            }
            'Q' => {
                while { skip_ws_comma(&mut chars); chars.peek().map_or(false, |c| c.is_ascii_digit() || *c == '-' || *c == '+' || *c == '.') } {
                    if let (Some(cx), Some(cy), Some(x), Some(y)) = (
                        parse_number(&mut chars), parse_number(&mut chars),
                        parse_number(&mut chars), parse_number(&mut chars),
                    ) {
                        path.quad_to((cx, cy), (x, y));
                        current_x = x;
                        current_y = y;
                    }
                }
            }
            'q' => {
                while { skip_ws_comma(&mut chars); chars.peek().map_or(false, |c| c.is_ascii_digit() || *c == '-' || *c == '+' || *c == '.') } {
                    if let (Some(dcx), Some(dcy), Some(dx), Some(dy)) = (
                        parse_number(&mut chars), parse_number(&mut chars),
                        parse_number(&mut chars), parse_number(&mut chars),
                    ) {
                        path.quad_to(
                            (current_x + dcx, current_y + dcy),
                            (current_x + dx, current_y + dy),
                        );
                        current_x += dx;
                        current_y += dy;
                    }
                }
            }
            'Z' | 'z' => {
                path.close();
            }
            _ => {
                // Skip unknown commands
            }
        }
        last_cmd = cmd;
    }

    if path.count_points() > 0 {
        Some(path)
    } else {
        None
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
