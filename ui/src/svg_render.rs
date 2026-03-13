//! SVG vector renderer: walks a usvg tree and emits native Skia draw calls.
//!
//! This provides resolution-independent SVG rendering by converting usvg's
//! normalized SVG tree directly to Skia canvas operations, avoiding the
//! rasterization step used by the resvg pipeline.

use std::collections::HashMap;

use resvg::usvg;
use skia_safe::{
    Canvas, Paint, PaintStyle, Path, PathFillType, Matrix, ImageFilter,
    canvas::SaveLayerRec, gradient_shader, TileMode,
    PictureRecorder,
};

use crate::color::Color;
use crate::layout::Rect;

/// A cached vector SVG, stored as a Skia Picture for resolution-independent replay.
pub struct VectorSvg {
    picture: skia_safe::Picture,
    intrinsic_width: f32,
    intrinsic_height: f32,
}

impl VectorSvg {
    /// Parse SVG data and record all drawing into a Skia Picture.
    pub fn from_data(data: &[u8]) -> Option<Self> {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_data(data, &opt).ok()?;
        let size = tree.size();

        let bounds = skia_safe::Rect::from_wh(size.width(), size.height());
        let mut recorder = PictureRecorder::new();
        {
            let canvas = recorder.begin_recording(bounds, None);
            render_node(canvas, tree.root());
        }
        let picture = recorder.finish_recording_as_picture(Some(&bounds))?;

        Some(Self {
            picture,
            intrinsic_width: size.width(),
            intrinsic_height: size.height(),
        })
    }

    /// Draw the vector SVG into the given destination bounds.
    pub fn draw(&self, canvas: &Canvas, dest: &Rect, tint: Option<&Color>) {
        canvas.save();
        let sx = dest.width / self.intrinsic_width;
        let sy = dest.height / self.intrinsic_height;
        canvas.translate((dest.x, dest.y));
        canvas.scale((sx, sy));

        if let Some(tint_color) = tint {
            // Draw into a layer, then apply tint via SrcIn color filter
            let mut paint = Paint::default();
            let rec = SaveLayerRec::default().paint(&paint);
            canvas.save_layer(&rec);
            canvas.draw_picture(&self.picture, None, None);
            // Apply tint over the layer
            let mut tint_paint = Paint::default();
            tint_paint.set_color(skia_safe::Color::from_argb(
                tint_color.a, tint_color.r, tint_color.g, tint_color.b,
            ));
            tint_paint.set_blend_mode(skia_safe::BlendMode::SrcIn);
            canvas.draw_paint(&tint_paint);
            canvas.restore();
        } else {
            canvas.draw_picture(&self.picture, None, None);
        }

        canvas.restore();
    }

    /// Draw with image fit computation.
    pub fn draw_fit(
        &self,
        canvas: &Canvas,
        dest: &Rect,
        tint: Option<&Color>,
        fit: crate::element::ImageFit,
    ) {
        let fitted = compute_vector_fit(
            self.intrinsic_width,
            self.intrinsic_height,
            dest,
            fit,
        );
        self.draw(canvas, &fitted, tint);
    }

    pub fn intrinsic_size(&self) -> (f32, f32) {
        (self.intrinsic_width, self.intrinsic_height)
    }
}

/// Render a usvg Group node and all its children to the Skia canvas.
fn render_node(canvas: &Canvas, group: &usvg::Group) {
    for child in group.children() {
        match child {
            usvg::Node::Group(ref g) => render_group(canvas, g),
            usvg::Node::Path(ref p) => render_path(canvas, p),
            usvg::Node::Image(ref img) => render_image(canvas, img),
            usvg::Node::Text(ref text) => render_text(canvas, text),
        }
    }
}

fn render_group(canvas: &Canvas, group: &usvg::Group) {
    canvas.save();

    // Apply transform
    let ts = group.transform();
    let matrix = usvg_transform_to_matrix(ts);
    canvas.concat(&matrix);

    // Apply clip-path
    if let Some(clip) = group.clip_path() {
        if let Some(clip_path) = build_clip_path(clip) {
            canvas.clip_path(&clip_path, skia_safe::ClipOp::Intersect, true);
        }
    }

    // Determine if we need a layer for opacity and/or filters
    let has_opacity = group.opacity().get() < 1.0;
    let filters = group.filters();
    let has_filters = !filters.is_empty();
    let has_mask = group.mask().is_some();

    if has_opacity || has_filters || has_mask {
        let mut paint = Paint::default();
        if has_opacity {
            paint.set_alpha_f(group.opacity().get());
        }
        if has_filters {
            if let Some(image_filter) = build_filters(filters) {
                paint.set_image_filter(image_filter);
            }
        }
        let rec = SaveLayerRec::default().paint(&paint);
        canvas.save_layer(&rec);
    }

    // Render children
    render_node(canvas, group);

    // Apply mask after rendering children (mask modulates content alpha)
    if let Some(mask) = group.mask() {
        apply_mask(canvas, mask);
    }

    // Pop layer (opacity and/or filter and/or mask)
    if has_opacity || has_filters || has_mask {
        canvas.restore();
    }

    canvas.restore();
}

fn render_path(canvas: &Canvas, path: &usvg::Path) {
    if !path.is_visible() {
        return;
    }

    let skia_path = usvg_path_to_skia(path.data());

    // Fill
    if let Some(fill) = path.fill() {
        if let Some(mut paint) = usvg_paint_to_skia(fill.paint(), fill.opacity()) {
            let mut p = skia_path.clone();
            p.set_fill_type(match fill.rule() {
                usvg::FillRule::NonZero => PathFillType::Winding,
                usvg::FillRule::EvenOdd => PathFillType::EvenOdd,
            });
            canvas.draw_path(&p, &paint);
        }
    }

    // Stroke
    if let Some(stroke) = path.stroke() {
        if let Some(mut paint) = usvg_paint_to_skia(stroke.paint(), stroke.opacity()) {
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(stroke.width().get());

            // Line cap
            paint.set_stroke_cap(match stroke.linecap() {
                usvg::LineCap::Butt => skia_safe::paint::Cap::Butt,
                usvg::LineCap::Round => skia_safe::paint::Cap::Round,
                usvg::LineCap::Square => skia_safe::paint::Cap::Square,
            });

            // Line join
            paint.set_stroke_join(match stroke.linejoin() {
                usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => skia_safe::paint::Join::Miter,
                usvg::LineJoin::Round => skia_safe::paint::Join::Round,
                usvg::LineJoin::Bevel => skia_safe::paint::Join::Bevel,
            });

            paint.set_stroke_miter(stroke.miterlimit().get());

            // Dash pattern
            if let Some(dash) = stroke.dasharray() {
                let intervals: Vec<f32> = dash.iter().copied().collect();
                if let Some(effect) = skia_safe::PathEffect::dash(&intervals, stroke.dashoffset()) {
                    paint.set_path_effect(effect);
                }
            }

            canvas.draw_path(&skia_path, &paint);
        }
    }
}

fn render_image(canvas: &Canvas, image: &usvg::Image) {
    if !image.is_visible() {
        return;
    }

    canvas.save();

    let bb = image.bounding_box();
    let rect = skia_safe::Rect::from_xywh(bb.x(), bb.y(), bb.width(), bb.height());

    match image.kind() {
        usvg::ImageKind::PNG(data)
        | usvg::ImageKind::JPEG(data)
        | usvg::ImageKind::GIF(data)
        | usvg::ImageKind::WEBP(data) => {
            if let Some(skia_img) = skia_safe::Image::from_encoded(
                skia_safe::Data::new_copy(data),
            ) {
                let paint = Paint::default();
                canvas.draw_image_rect(skia_img, None, rect, &paint);
            }
        }
        usvg::ImageKind::SVG(ref sub_tree) => {
            // Recursively render nested SVG
            canvas.save();
            let size = sub_tree.size();
            if size.width() > 0.0 && size.height() > 0.0 {
                let sx = bb.width() / size.width();
                let sy = bb.height() / size.height();
                canvas.translate((bb.x(), bb.y()));
                canvas.scale((sx, sy));
            }
            render_node(canvas, sub_tree.root());
            canvas.restore();
        }
    }

    canvas.restore();
}

fn render_text(canvas: &Canvas, text: &usvg::Text) {
    // usvg flattens text into positioned paths via text-to-path conversion.
    let group = text.flattened();
    render_node(canvas, group);
}

// --- Conversion helpers ---

fn usvg_transform_to_matrix(ts: usvg::Transform) -> Matrix {
    Matrix::new_all(
        ts.sx, ts.kx, ts.tx,
        ts.ky, ts.sy, ts.ty,
        0.0, 0.0, 1.0,
    )
}

fn usvg_path_to_skia(data: &usvg::tiny_skia_path::Path) -> Path {
    use usvg::tiny_skia_path::PathSegment;

    let mut path = Path::new();

    for seg in data.segments() {
        match seg {
            PathSegment::MoveTo(pt) => {
                path.move_to((pt.x, pt.y));
            }
            PathSegment::LineTo(pt) => {
                path.line_to((pt.x, pt.y));
            }
            PathSegment::QuadTo(p1, p2) => {
                path.quad_to((p1.x, p1.y), (p2.x, p2.y));
            }
            PathSegment::CubicTo(p1, p2, p3) => {
                path.cubic_to((p1.x, p1.y), (p2.x, p2.y), (p3.x, p3.y));
            }
            PathSegment::Close => {
                path.close();
            }
        }
    }

    path
}

fn usvg_paint_to_skia(paint: &usvg::Paint, opacity: usvg::Opacity) -> Option<Paint> {
    let mut p = Paint::default();
    p.set_anti_alias(true);

    match paint {
        usvg::Paint::Color(c) => {
            p.set_color(skia_safe::Color::from_argb(
                (opacity.get() * 255.0) as u8,
                c.red,
                c.green,
                c.blue,
            ));
        }
        usvg::Paint::LinearGradient(lg) => {
            let (colors, positions) = gradient_stops(&lg.stops(), opacity);
            let start = skia_safe::Point::new(lg.x1(), lg.y1());
            let end = skia_safe::Point::new(lg.x2(), lg.y2());
            let tile_mode = spread_to_tile_mode(lg.spread_method());
            let local_matrix = usvg_transform_to_matrix(lg.transform());

            let shader = gradient_shader::linear(
                (start, end),
                colors.as_ref(),
                positions.as_deref(),
                tile_mode,
                None,
                Some(&local_matrix),
            )?;
            p.set_shader(shader);
        }
        usvg::Paint::RadialGradient(rg) => {
            let (colors, positions) = gradient_stops(&rg.stops(), opacity);
            let center = skia_safe::Point::new(rg.cx(), rg.cy());
            let focal = skia_safe::Point::new(rg.fx(), rg.fy());
            let tile_mode = spread_to_tile_mode(rg.spread_method());
            let local_matrix = usvg_transform_to_matrix(rg.transform());

            let shader = gradient_shader::two_point_conical(
                focal, 0.0,
                center, rg.r().get(),
                colors.as_ref(),
                positions.as_deref(),
                tile_mode,
                None,
                Some(&local_matrix),
            )?;
            p.set_shader(shader);
        }
        usvg::Paint::Pattern(pattern) => {
            let rect = pattern.rect();
            let tile_bounds = skia_safe::Rect::from_xywh(
                rect.x(), rect.y(), rect.width(), rect.height(),
            );

            // Record pattern content into a Picture
            let mut recorder = PictureRecorder::new();
            {
                let pattern_canvas = recorder.begin_recording(tile_bounds, None);
                render_node(pattern_canvas, pattern.root());
            }
            if let Some(picture) = recorder.finish_recording_as_picture(Some(&tile_bounds)) {
                let local_matrix = usvg_transform_to_matrix(pattern.transform());
                let shader = picture.to_shader(
                    Some((TileMode::Repeat, TileMode::Repeat)),
                    skia_safe::FilterMode::Linear,
                    Some(&local_matrix),
                    Some(&tile_bounds),
                );
                p.set_shader(shader);
                if opacity.get() < 1.0 {
                    p.set_alpha_f(opacity.get());
                }
            } else {
                // Fallback if picture recording fails
                p.set_color(skia_safe::Color::from_argb(
                    (opacity.get() * 255.0) as u8,
                    128, 128, 128,
                ));
            }
        }
    }

    Some(p)
}

fn gradient_stops(
    stops: &[usvg::Stop],
    opacity: usvg::Opacity,
) -> (Vec<skia_safe::Color>, Option<Vec<f32>>) {
    let colors: Vec<skia_safe::Color> = stops
        .iter()
        .map(|s| {
            let a = (s.opacity().get() * opacity.get() * 255.0) as u8;
            skia_safe::Color::from_argb(a, s.color().red, s.color().green, s.color().blue)
        })
        .collect();
    let positions: Vec<f32> = stops.iter().map(|s| s.offset().get()).collect();
    (colors, Some(positions))
}

fn spread_to_tile_mode(spread: usvg::SpreadMethod) -> TileMode {
    match spread {
        usvg::SpreadMethod::Pad => TileMode::Clamp,
        usvg::SpreadMethod::Reflect => TileMode::Mirror,
        usvg::SpreadMethod::Repeat => TileMode::Repeat,
    }
}

fn build_clip_path(clip: &usvg::ClipPath) -> Option<Path> {
    let mut combined = Path::new();

    for child in clip.root().children() {
        if let usvg::Node::Path(ref p) = child {
            let mut sub_path = usvg_path_to_skia(p.data());
            // Apply per-path fill rule for clipping
            if let Some(fill) = p.fill() {
                sub_path.set_fill_type(match fill.rule() {
                    usvg::FillRule::NonZero => PathFillType::Winding,
                    usvg::FillRule::EvenOdd => PathFillType::EvenOdd,
                });
            }
            combined.add_path(&sub_path, (0.0, 0.0), skia_safe::path::AddPathMode::Append);
        }
    }

    // Apply clip path transform
    let matrix = usvg_transform_to_matrix(clip.transform());
    combined.transform(&matrix);

    if combined.is_empty() {
        None
    } else {
        Some(combined)
    }
}

/// Apply an SVG mask to the current layer.
///
/// The mask content is rendered with DstIn blend mode, which keeps the
/// destination (already-drawn content) only where the mask has opacity.
/// For luminance masks, a color matrix converts RGB luminance to alpha first.
fn apply_mask(canvas: &Canvas, mask: &usvg::Mask) {
    let mut mask_paint = Paint::default();
    mask_paint.set_blend_mode(skia_safe::BlendMode::DstIn);

    // For luminance masks, convert RGB luminance to alpha
    if mask.kind() == usvg::MaskType::Luminance {
        let lum_matrix: [f32; 20] = [
            0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0,
            0.2126, 0.7152, 0.0722, 0.0, 0.0,
        ];
        let cf = skia_safe::color_filters::matrix_row_major(&lum_matrix, None);
        let img_filter = skia_safe::image_filters::color_filter(
            cf, None, skia_safe::image_filters::CropRect::NO_CROP_RECT,
        );
        if let Some(f) = img_filter {
            mask_paint.set_image_filter(f);
        }
    }

    let rec = SaveLayerRec::default().paint(&mask_paint);
    canvas.save_layer(&rec);

    // Render mask children
    render_node(canvas, mask.root());

    canvas.restore();

    // Apply nested mask if present
    if let Some(nested_mask) = mask.mask() {
        apply_mask(canvas, nested_mask);
    }
}

// --- SVG Filter support ---

/// Build a composed Skia ImageFilter from a list of usvg filters.
/// Each filter contains a chain of primitives; we compose all of them.
fn build_filters(filters: &[std::sync::Arc<usvg::filter::Filter>]) -> Option<ImageFilter> {
    let mut result: Option<ImageFilter> = None;

    for filter in filters {
        // Build filter primitives, tracking named results for cross-references
        let mut named_results: HashMap<String, ImageFilter> = HashMap::new();
        let mut last_result: Option<ImageFilter> = None;

        for primitive in filter.primitives() {
            let built = build_filter_primitive(primitive, &named_results, &last_result);
            if let Some(ref f) = built {
                if !primitive.result().is_empty() {
                    named_results.insert(primitive.result().to_string(), f.clone());
                }
                last_result = built;
            }
        }

        // Compose multiple filters together
        if let Some(filter_result) = last_result {
            result = Some(match result {
                Some(existing) => {
                    skia_safe::image_filters::compose(filter_result, existing)
                        .unwrap_or(existing)
                }
                None => filter_result,
            });
        }
    }

    result
}

/// Build a single filter primitive, resolving input references.
fn build_filter_primitive(
    primitive: &usvg::filter::Primitive,
    named: &HashMap<String, ImageFilter>,
    last: &Option<ImageFilter>,
) -> Option<ImageFilter> {
    use usvg::filter::*;

    let crop = skia_safe::image_filters::CropRect::NO_CROP_RECT;

    match primitive.kind() {
        Kind::GaussianBlur(blur) => {
            let input = resolve_input(blur.input(), named, last);
            skia_safe::image_filters::blur(
                (blur.std_dev_x().get(), blur.std_dev_y().get()),
                TileMode::Decal,
                input,
                crop,
            )
        }

        Kind::Offset(offset) => {
            let input = resolve_input(offset.input(), named, last);
            skia_safe::image_filters::offset(
                (offset.dx(), offset.dy()),
                input,
                crop,
            )
        }

        Kind::ColorMatrix(cm) => {
            let input = resolve_input(cm.input(), named, last);
            let color_filter = match cm.kind() {
                ColorMatrixKind::Matrix(values) => {
                    // 20-element row-major 4x5 matrix
                    if values.len() == 20 {
                        let mut arr = [0.0f32; 20];
                        arr.copy_from_slice(values);
                        Some(skia_safe::color_filters::matrix_row_major(&arr, None))
                    } else {
                        None
                    }
                }
                ColorMatrixKind::Saturate(s) => {
                    let mat = saturate_matrix(s.get());
                    Some(skia_safe::color_filters::matrix_row_major(&mat, None))
                }
                ColorMatrixKind::HueRotate(deg) => {
                    let mat = hue_rotate_matrix(*deg);
                    Some(skia_safe::color_filters::matrix_row_major(&mat, None))
                }
                ColorMatrixKind::LuminanceToAlpha => {
                    let mat = luminance_to_alpha_matrix();
                    Some(skia_safe::color_filters::matrix_row_major(&mat, None))
                }
            };
            color_filter.and_then(|cf| skia_safe::image_filters::color_filter(cf, input, crop))
        }

        Kind::ComponentTransfer(ct) => {
            let input = resolve_input(ct.input(), named, last);
            let r_table = build_transfer_table(ct.func_r());
            let g_table = build_transfer_table(ct.func_g());
            let b_table = build_transfer_table(ct.func_b());
            let a_table = build_transfer_table(ct.func_a());
            let cf = skia_safe::color_filters::table_argb(&a_table, &r_table, &g_table, &b_table);
            cf.and_then(|cf| skia_safe::image_filters::color_filter(cf, input, crop))
        }

        Kind::Composite(comp) => {
            let bg = resolve_input(comp.input2(), named, last);
            let fg = resolve_input(comp.input1(), named, last);
            match comp.operator() {
                CompositeOperator::Over => {
                    skia_safe::image_filters::blend(skia_safe::BlendMode::SrcOver, bg, fg, crop)
                }
                CompositeOperator::In => {
                    skia_safe::image_filters::blend(skia_safe::BlendMode::SrcIn, bg, fg, crop)
                }
                CompositeOperator::Out => {
                    skia_safe::image_filters::blend(skia_safe::BlendMode::SrcOut, bg, fg, crop)
                }
                CompositeOperator::Atop => {
                    skia_safe::image_filters::blend(skia_safe::BlendMode::SrcATop, bg, fg, crop)
                }
                CompositeOperator::Xor => {
                    skia_safe::image_filters::blend(skia_safe::BlendMode::Xor, bg, fg, crop)
                }
                CompositeOperator::Arithmetic { k1, k2, k3, k4 } => {
                    skia_safe::image_filters::arithmetic(*k1, *k2, *k3, *k4, true, bg, fg, crop)
                }
            }
        }

        Kind::Blend(blend) => {
            let bg = resolve_input(blend.input2(), named, last);
            let fg = resolve_input(blend.input1(), named, last);
            let mode = usvg_blend_to_skia(blend.mode());
            skia_safe::image_filters::blend(mode, bg, fg, crop)
        }

        Kind::Morphology(morph) => {
            let input = resolve_input(morph.input(), named, last);
            match morph.operator() {
                MorphologyOperator::Erode => {
                    skia_safe::image_filters::erode(
                        (morph.radius_x().get(), morph.radius_y().get()),
                        input,
                        crop,
                    )
                }
                MorphologyOperator::Dilate => {
                    skia_safe::image_filters::dilate(
                        (morph.radius_x().get(), morph.radius_y().get()),
                        input,
                        crop,
                    )
                }
            }
        }

        Kind::DropShadow(ds) => {
            let input = resolve_input(ds.input(), named, last);
            let color = skia_safe::Color4f::new(
                ds.color().red as f32 / 255.0,
                ds.color().green as f32 / 255.0,
                ds.color().blue as f32 / 255.0,
                ds.opacity().get(),
            );
            skia_safe::image_filters::drop_shadow(
                (ds.dx(), ds.dy()),
                (ds.std_dev_x().get(), ds.std_dev_y().get()),
                color,
                None,
                input,
                crop,
            )
        }

        Kind::Flood(flood) => {
            let color = skia_safe::Color::from_argb(
                (flood.opacity().get() * 255.0) as u8,
                flood.color().red,
                flood.color().green,
                flood.color().blue,
            );
            let cf = skia_safe::color_filters::blend(color, skia_safe::BlendMode::Src);
            cf.and_then(|cf| skia_safe::image_filters::color_filter(cf, None, crop))
        }

        Kind::Merge(merge) => {
            let filters: Vec<Option<ImageFilter>> = merge
                .inputs()
                .iter()
                .map(|input| resolve_input(input, named, last))
                .collect();
            skia_safe::image_filters::merge(filters, crop)
        }

        Kind::Tile(tile) => {
            let input = resolve_input(tile.input(), named, last);
            let prim_rect = primitive.rect();
            let src_rect = skia_safe::Rect::from_xywh(
                prim_rect.x(), prim_rect.y(), prim_rect.width(), prim_rect.height(),
            );
            skia_safe::image_filters::tile(src_rect, src_rect, input)
        }

        Kind::Turbulence(turb) => {
            // Turbulence is a source filter (no input) - use shader
            // Skia doesn't have fractal noise as image filter directly,
            // so we skip this for now (uncommon in icon SVGs)
            None
        }

        Kind::Image(_) => {
            // feImage requires rendering embedded content - skip for now
            None
        }

        Kind::ConvolveMatrix(_) => {
            // Complex convolution - skip for now (uncommon in UI icons)
            None
        }

        Kind::DisplacementMap(_) => {
            // Displacement map - skip for now (uncommon in UI icons)
            None
        }

        // Lighting filters - skip as planned
        Kind::DiffuseLighting(_) | Kind::SpecularLighting(_) => None,
    }
}

/// Resolve a filter input reference to a Skia ImageFilter.
fn resolve_input(
    input: &usvg::filter::Input,
    named: &HashMap<String, ImageFilter>,
    last: &Option<ImageFilter>,
) -> Option<ImageFilter> {
    match input {
        usvg::filter::Input::SourceGraphic => None, // None means "use source"
        usvg::filter::Input::SourceAlpha => {
            // Extract alpha channel from source (zero out RGB, keep alpha)
            let mat: [f32; 20] = [
                0.0, 0.0, 0.0, 0.0, 0.0,
                0.0, 0.0, 0.0, 0.0, 0.0,
                0.0, 0.0, 0.0, 0.0, 0.0,
                0.0, 0.0, 0.0, 1.0, 0.0,
            ];
            let cf = skia_safe::color_filters::matrix_row_major(&mat, None);
            skia_safe::image_filters::color_filter(cf, None, skia_safe::image_filters::CropRect::NO_CROP_RECT)
        }
        usvg::filter::Input::Reference(name) => {
            named.get(name).cloned().or_else(|| last.clone())
        }
    }
}

/// Map usvg BlendMode to Skia BlendMode.
fn usvg_blend_to_skia(mode: usvg::BlendMode) -> skia_safe::BlendMode {
    match mode {
        usvg::BlendMode::Normal => skia_safe::BlendMode::SrcOver,
        usvg::BlendMode::Multiply => skia_safe::BlendMode::Multiply,
        usvg::BlendMode::Screen => skia_safe::BlendMode::Screen,
        usvg::BlendMode::Overlay => skia_safe::BlendMode::Overlay,
        usvg::BlendMode::Darken => skia_safe::BlendMode::Darken,
        usvg::BlendMode::Lighten => skia_safe::BlendMode::Lighten,
        usvg::BlendMode::ColorDodge => skia_safe::BlendMode::ColorDodge,
        usvg::BlendMode::ColorBurn => skia_safe::BlendMode::ColorBurn,
        usvg::BlendMode::HardLight => skia_safe::BlendMode::HardLight,
        usvg::BlendMode::SoftLight => skia_safe::BlendMode::SoftLight,
        usvg::BlendMode::Difference => skia_safe::BlendMode::Difference,
        usvg::BlendMode::Exclusion => skia_safe::BlendMode::Exclusion,
        usvg::BlendMode::Hue => skia_safe::BlendMode::Hue,
        usvg::BlendMode::Saturation => skia_safe::BlendMode::Saturation,
        usvg::BlendMode::Color => skia_safe::BlendMode::Color,
        usvg::BlendMode::Luminosity => skia_safe::BlendMode::Luminosity,
    }
}

// --- Color matrix helpers ---

/// Build a 4x5 saturation matrix (SVG feColorMatrix type="saturate").
fn saturate_matrix(s: f32) -> [f32; 20] {
    [
        0.2126 + 0.7874 * s, 0.7152 - 0.7152 * s, 0.0722 - 0.0722 * s, 0.0, 0.0,
        0.2126 - 0.2126 * s, 0.7152 + 0.2848 * s, 0.0722 - 0.0722 * s, 0.0, 0.0,
        0.2126 - 0.2126 * s, 0.7152 - 0.7152 * s, 0.0722 + 0.9278 * s, 0.0, 0.0,
        0.0,                 0.0,                  0.0,                  1.0, 0.0,
    ]
}

/// Build a 4x5 hue rotation matrix (SVG feColorMatrix type="hueRotate").
fn hue_rotate_matrix(degrees: f32) -> [f32; 20] {
    let rad = degrees * std::f32::consts::PI / 180.0;
    let cos = rad.cos();
    let sin = rad.sin();

    [
        0.213 + cos * 0.787 - sin * 0.213,
        0.715 - cos * 0.715 - sin * 0.715,
        0.072 - cos * 0.072 + sin * 0.928,
        0.0, 0.0,
        0.213 - cos * 0.213 + sin * 0.143,
        0.715 + cos * 0.285 + sin * 0.140,
        0.072 - cos * 0.072 - sin * 0.283,
        0.0, 0.0,
        0.213 - cos * 0.213 - sin * 0.787,
        0.715 - cos * 0.715 + sin * 0.715,
        0.072 + cos * 0.928 + sin * 0.072,
        0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ]
}

/// Build a 4x5 luminance-to-alpha matrix.
fn luminance_to_alpha_matrix() -> [f32; 20] {
    [
        0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0,
        0.2126, 0.7152, 0.0722, 0.0, 0.0,
    ]
}

/// Build a 256-entry lookup table from a TransferFunction.
fn build_transfer_table(func: &usvg::filter::TransferFunction) -> [u8; 256] {
    use usvg::filter::TransferFunction;
    let mut table = [0u8; 256];

    match func {
        TransferFunction::Identity => {
            for i in 0..256 {
                table[i] = i as u8;
            }
        }
        TransferFunction::Table(values) => {
            if values.is_empty() {
                for i in 0..256 {
                    table[i] = i as u8;
                }
            } else {
                let n = values.len() - 1;
                for i in 0..256 {
                    let pos = (i as f32 / 255.0) * n as f32;
                    let idx = (pos as usize).min(n - 1);
                    let frac = pos - idx as f32;
                    let val = values[idx] * (1.0 - frac) + values[idx + 1] * frac;
                    table[i] = (val.clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
        }
        TransferFunction::Discrete(values) => {
            if values.is_empty() {
                for i in 0..256 {
                    table[i] = i as u8;
                }
            } else {
                let n = values.len();
                for i in 0..256 {
                    let idx = ((i as f32 / 255.0) * n as f32) as usize;
                    let idx = idx.min(n - 1);
                    table[i] = (values[idx].clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
        }
        TransferFunction::Linear { slope, intercept } => {
            for i in 0..256 {
                let val = slope * (i as f32 / 255.0) + intercept;
                table[i] = (val.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
        TransferFunction::Gamma { amplitude, exponent, offset } => {
            for i in 0..256 {
                let val = amplitude * (i as f32 / 255.0).powf(*exponent) + offset;
                table[i] = (val.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }

    table
}

fn compute_vector_fit(
    intrinsic_w: f32,
    intrinsic_h: f32,
    dest: &Rect,
    fit: crate::element::ImageFit,
) -> Rect {
    if intrinsic_w <= 0.0 || intrinsic_h <= 0.0 {
        return *dest;
    }

    let aspect = intrinsic_w / intrinsic_h;

    match fit {
        crate::element::ImageFit::Fill => *dest,
        crate::element::ImageFit::Contain => {
            let (w, h) = if dest.width / dest.height > aspect {
                (dest.height * aspect, dest.height)
            } else {
                (dest.width, dest.width / aspect)
            };
            Rect {
                x: dest.x + (dest.width - w) / 2.0,
                y: dest.y + (dest.height - h) / 2.0,
                width: w,
                height: h,
            }
        }
        crate::element::ImageFit::Cover => {
            let (w, h) = if dest.width / dest.height > aspect {
                (dest.width, dest.width / aspect)
            } else {
                (dest.height * aspect, dest.height)
            };
            Rect {
                x: dest.x + (dest.width - w) / 2.0,
                y: dest.y + (dest.height - h) / 2.0,
                width: w,
                height: h,
            }
        }
        crate::element::ImageFit::ScaleDown => {
            if intrinsic_w <= dest.width && intrinsic_h <= dest.height {
                // Don't upscale — center at intrinsic size
                Rect {
                    x: dest.x + (dest.width - intrinsic_w) / 2.0,
                    y: dest.y + (dest.height - intrinsic_h) / 2.0,
                    width: intrinsic_w,
                    height: intrinsic_h,
                }
            } else {
                compute_vector_fit(intrinsic_w, intrinsic_h, dest, crate::element::ImageFit::Contain)
            }
        }
    }
}
