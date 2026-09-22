//! Worldwall executable.

mod animation;
mod environment;
mod power;
mod renderer;
mod scene;
mod scene_registry;
mod scheduler;
mod wayland_surface;
mod weather;

fn main() {
    if let Err(error) = wayland_surface::run_multi() {
        eprintln!("worldwall: {error}");
        std::process::exit(1);
    }
}
