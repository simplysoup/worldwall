//! Worldwall executable.

mod animation;
mod renderer;
mod scene;
mod scheduler;
mod wayland_surface;

fn main() {
    if let Err(error) = wayland_surface::run() {
        eprintln!("worldwall: {error}");
        std::process::exit(1);
    }
}
