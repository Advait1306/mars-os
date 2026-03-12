mod dock;
mod icons;
mod windows;

fn main() {
    env_logger::init();
    log::info!("Starting dock");
    ui::run(
        dock::DockView::new(),
        ui::SurfaceConfig::LayerShell {
            namespace: "dock".into(),
            layer: ui::Layer::Top,
            anchor: ui::Anchor::BOTTOM,
            size: (200, 64),
            exclusive_zone: 0,
            keyboard: ui::KeyboardInteractivity::None,
            margin: (0, 0, 8, 0),
        },
    );
}
