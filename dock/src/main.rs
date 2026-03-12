mod dock;
mod icons;
mod windows;

fn main() {
    env_logger::init();
    log::info!("Starting dock");
    dock::Dock::run();
}
