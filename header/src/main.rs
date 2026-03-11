mod controls;
mod header;
mod render;

fn main() {
    env_logger::init();
    log::info!("Starting header");
    header::Header::run();
}
