mod animation;
mod dock;
mod render;
mod windows;

fn main() {
    env_logger::init();
    log::info!("Starting mars-dock");

    dock::Dock::run();
}
