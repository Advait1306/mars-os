mod apps;
mod render;
mod spotlight;

fn main() {
    env_logger::init();
    log::info!("Starting spotlight");
    spotlight::Spotlight::run();
}
