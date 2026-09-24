//! Worldwall executable.

mod animation;
mod config;
mod environment;
mod ipc;
mod power;
mod renderer;
mod scene;
mod scene_pools;
mod scene_registry;
mod scheduler;
mod selection_state;
mod wayland_surface;
mod weather;

fn main() {
    // The preserved single-output S01/S02 baseline is reachable only through
    // this explicit diagnostic seam (see tools/run_nested_s01.sh). Normal
    // operation always takes the multi-output path.
    let result = if std::env::var_os("WORLDWALL_SINGLE_OUTPUT_BASELINE").is_some() {
        wayland_surface::run()
    } else {
        wayland_surface::run_multi()
    };
    if let Err(error) = result {
        eprintln!("worldwall: {error}");
        std::process::exit(1);
    }
}
