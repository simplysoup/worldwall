//! Temporary static scene interface used by the S02 renderer foundation.
//!
//! S03 replaces this interface with the bounded PNG decoder and palette
//! validation implementation.

use std::error::Error;

#[derive(Debug, Clone)]
pub struct IndexedImage {
    pub width: u32,
    pub height: u32,
    pub indices: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Palette {
    pub rgba: [u8; 256 * 4],
}

#[derive(Debug, Clone)]
pub struct Scene {
    pub image: IndexedImage,
    pub palette: Palette,
}

impl Scene {
    pub fn reference() -> Self {
        let mut rgba = [0; 256 * 4];
        for index in 0..256u16 {
            let offset = usize::from(index) * 4;
            let value = index as u8;
            rgba[offset..offset + 4].copy_from_slice(&[
                value,
                255u8.saturating_sub(value),
                value.rotate_left(1),
                255,
            ]);
        }
        rgba[0..4].copy_from_slice(&[0, 0, 0, 255]);
        rgba[4..8].copy_from_slice(&[255, 0, 0, 255]);
        rgba[8..12].copy_from_slice(&[0, 255, 0, 255]);
        rgba[255 * 4..256 * 4].copy_from_slice(&[255, 255, 255, 255]);
        Self {
            image: IndexedImage {
                width: 3,
                height: 2,
                indices: vec![0, 1, 2, 255, 1, 0],
            },
            palette: Palette { rgba },
        }
    }
}

pub fn load_png_pair(
    _scene_path: impl AsRef<std::path::Path>,
    _palette_path: impl AsRef<std::path::Path>,
) -> Result<Scene, Box<dyn Error>> {
    Err("S03 scene package loading is not yet implemented".into())
}

pub fn cover_source_coord(source: (u32, u32), output: (u32, u32), pixel: (u32, u32)) -> (u32, u32) {
    assert!(source.0 > 0 && source.1 > 0 && output.0 > 0 && output.1 > 0);
    assert!(pixel.0 < output.0 && pixel.1 < output.1);
    let scale =
        (f64::from(output.0) / f64::from(source.0)).max(f64::from(output.1) / f64::from(source.1));
    let crop_x = (f64::from(source.0) * scale - f64::from(output.0)) / 2.0;
    let crop_y = (f64::from(source.1) * scale - f64::from(output.1)) / 2.0;
    (
        (((f64::from(pixel.0) + 0.5 + crop_x) / scale).floor()).clamp(0.0, f64::from(source.0 - 1))
            as u32,
        (((f64::from(pixel.1) + 0.5 + crop_y) / scale).floor()).clamp(0.0, f64::from(source.1 - 1))
            as u32,
    )
}
