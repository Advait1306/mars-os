use ui::*;

struct HelloView;

impl View for HelloView {
    fn render(&self) -> Element {
        container()
            .background(rgba(30, 30, 30, 220))
            .rounded(12.0)
            .padding(20.0)
            .child(text("Hello from ui framework").font_size(18.0).color(WHITE))
    }
}

fn main() {
    env_logger::init();

    ui::run(
        HelloView,
        SurfaceConfig::LayerShell {
            namespace: "hello-ui".into(),
            layer: smithay_client_toolkit::shell::wlr_layer::Layer::Top,
            anchor: smithay_client_toolkit::shell::wlr_layer::Anchor::BOTTOM,
            size: (400, 60),
            exclusive_zone: 0,
            keyboard:
                smithay_client_toolkit::shell::wlr_layer::KeyboardInteractivity::None,
        },
    );
}
