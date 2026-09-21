//! Minimal S00 dependency and toolchain smoke target.
//!
//! This is intentionally not a renderer. Wayland ownership, layer-shell setup,
//! and wgpu surface creation belong to the later stages where their lifetimes
//! can be reviewed together.

use std::any::TypeId;

use raw_window_handle::RawDisplayHandle;
use smithay_client_toolkit::reexports::{
    calloop::EventLoop, calloop_wayland_source::WaylandSource, client::Connection,
};

fn dependency_api_spike() {
    // Keep the first target free of compositor side effects while making the
    // selected API path compile against the locked dependency graph.
    let _ = TypeId::of::<Connection>();
    let _ = TypeId::of::<EventLoop<'static, ()>>();
    let _ = TypeId::of::<WaylandSource<()>>();
    let _ = TypeId::of::<RawDisplayHandle>();
    let _ = wgpu::Backends::VULKAN;
}

fn main() {
    dependency_api_spike();
    println!("worldwall foundation scaffold");
}

#[cfg(test)]
mod tests {
    #[test]
    fn foundation_binary_has_a_test_harness() {
        super::dependency_api_spike();
    }
}
