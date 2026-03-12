use std::collections::HashMap;
use skia_safe::{
    Canvas, Paint, RRect, Font, FontMgr, FontStyle, Typeface,
    PaintStyle, MaskFilter, BlurStyle, ClipOp,
    image_filters, canvas::SaveLayerRec,
};
use crate::color::Color;
use crate::display_list::{DrawCommand, Point};
use crate::layout::Rect;
use crate::element::ImageSource;

pub struct SkiaRenderer {
    font_mgr: FontMgr,
    typeface_cache: HashMap<String, Typeface>,
    image_cache: HashMap<String, skia_safe::Image>,
}

impl SkiaRenderer {
    pub fn new() -> Self {
        Self {
            font_mgr: FontMgr::default(),
            typeface_cache: HashMap::new(),
            image_cache: HashMap::new(),
        }
    }

    pub fn execute(&mut self, canvas: &Canvas, commands: &[DrawCommand]) {
        for cmd in commands {
            match cmd {
                DrawCommand::Rect { bounds, background, corner_radius, border } => {
                    self.draw_rect(canvas, bounds, background, *corner_radius, border.as_ref());
                }
                DrawCommand::Text { text, position, font_size, color } => {
                    self.draw_text(canvas, text, position, *font_size, color);
                }
                DrawCommand::Image { source, bounds } => {
                    self.draw_image(canvas, source, bounds);
                }
                DrawCommand::BoxShadow { bounds, corner_radius, blur, spread, color, offset } => {
                    self.draw_box_shadow(canvas, bounds, *corner_radius, *blur, *spread, color, offset);
                }
                DrawCommand::PushClip { bounds, corner_radius } => {
                    canvas.save();
                    let rrect = to_rrect(bounds, *corner_radius);
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
                DrawCommand::BackdropBlur { bounds, corner_radius, blur_radius } => {
                    self.draw_backdrop_blur(canvas, bounds, *corner_radius, *blur_radius);
                }
                DrawCommand::PushTranslate { offset } => {
                    canvas.save();
                    canvas.translate((offset.x, offset.y));
                }
                DrawCommand::PopTranslate => {
                    canvas.restore();
                }
            }
        }
    }

    fn draw_rect(&self, canvas: &Canvas, bounds: &Rect, bg: &Color, radius: f32, border: Option<&crate::style::Border>) {
        let rrect = to_rrect(bounds, radius);

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

    fn draw_text(&self, canvas: &Canvas, text: &str, pos: &Point, font_size: f32, color: &Color) {
        let typeface = self.font_mgr
            .legacy_make_typeface(None, FontStyle::default())
            .expect("no default typeface available");
        let font = Font::from_typeface(typeface, font_size);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(to_skia_color(color));

        // Get font metrics for baseline positioning
        let (_, metrics) = font.metrics();
        let baseline_y = pos.y - metrics.ascent; // ascent is negative

        canvas.draw_str(text, (pos.x, baseline_y), &font, &paint);
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

    fn draw_box_shadow(&self, canvas: &Canvas, bounds: &Rect, radius: f32, blur: f32, spread: f32, color: &Color, offset: &Point) {
        let expanded = Rect {
            x: bounds.x + offset.x - spread,
            y: bounds.y + offset.y - spread,
            width: bounds.width + spread * 2.0,
            height: bounds.height + spread * 2.0,
        };
        let rrect = to_rrect(&expanded, radius);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(to_skia_color(color));
        paint.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, blur / 2.0, false));
        canvas.draw_rrect(rrect, &paint);
    }

    fn draw_backdrop_blur(&self, canvas: &Canvas, bounds: &Rect, radius: f32, blur_radius: f32) {
        canvas.save();
        let rrect = to_rrect(bounds, radius);
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

/// Text measurement for layout (called from layout.rs if needed).
pub fn measure_text(text: &str, font_size: f32) -> (f32, f32) {
    let font_mgr = FontMgr::default();
    let typeface = font_mgr
        .legacy_make_typeface(None, FontStyle::default())
        .expect("no default typeface available");
    let font = Font::from_typeface(typeface, font_size);
    let (width, _) = font.measure_str(text, None);
    let (_, metrics) = font.metrics();
    let height = metrics.descent - metrics.ascent;
    (width, height)
}

fn to_skia_color(c: &Color) -> skia_safe::Color {
    skia_safe::Color::from_argb(c.a, c.r, c.g, c.b)
}

fn to_rrect(bounds: &Rect, radius: f32) -> RRect {
    RRect::new_rect_xy(
        skia_safe::Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height),
        radius,
        radius,
    )
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
