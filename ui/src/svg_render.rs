//! SVG vector renderer: walks a usvg tree and emits native Skia draw calls.
//!
//! This provides resolution-independent SVG rendering by converting usvg's
//! normalized SVG tree directly to Skia canvas operations, avoiding the
//! rasterization step used by the resvg pipeline.

use resvg::usvg;
use skia_safe::{
    Canvas, Paint, PaintStyle, Path, PathFillType, Matrix,
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

    // Apply opacity via layer
    let has_opacity = group.opacity().get() < 1.0;
    if has_opacity {
        let mut paint = Paint::default();
        paint.set_alpha_f(group.opacity().get());
        let rec = SaveLayerRec::default().paint(&paint);
        canvas.save_layer(&rec);
    }

    // Apply clip-path
    if let Some(clip) = group.clip_path() {
        if let Some(clip_path) = build_clip_path(clip) {
            canvas.clip_path(&clip_path, skia_safe::ClipOp::Intersect, true);
        }
    }

    // Render children
    render_node(canvas, group);

    // Pop opacity layer
    if has_opacity {
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
        usvg::Paint::Pattern(_) => {
            // Pattern paint is complex; fall back to gray for now
            p.set_color(skia_safe::Color::from_argb(
                (opacity.get() * 255.0) as u8,
                128, 128, 128,
            ));
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
