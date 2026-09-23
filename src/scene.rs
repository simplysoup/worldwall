//! Validated static indexed-scene data for S03.
//!
//! The loader deliberately returns raw index bytes. It never expands an indexed
//! PNG to RGB and it does not interpret an index as grayscale or alpha.

use std::{
    error::Error,
    fmt,
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

use crate::{animation::Animation, environment::SolarEvents};
use png::{BitDepth, ColorType, Decoder, Limits};

/// Maximum compressed size accepted for one S03 PNG input.
pub const MAX_ENCODED_BYTES: usize = 16 * 1024 * 1024;
/// Maximum number of source pixels accepted for an indexed image.
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
/// Maximum decoded bytes reserved for one PNG, including decoder overhead.
const MAX_DECODER_BYTES: usize = 64 * 1024 * 1024;
/// Civil time wraps at one local day.
pub const CIVIL_SECONDS_PER_DAY: u32 = 86_400;
/// Time-of-day palette rows are intentionally bounded to keep scene uploads small.
pub const MAX_TIME_OF_DAY_ROWS: u32 = 256;
/// Shader traversal and per-frame solar anchor sorting have a fixed upper bound.
pub const MAX_TIME_OF_DAY_KEYFRAMES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedImage {
    pub width: u32,
    pub height: u32,
    pub indices: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// Exactly 256 opaque RGBA8 entries in authoring sRGB byte order.
    pub rgba: [u8; 256 * 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOfDayKeyframe {
    /// Local civil seconds in [0, 86_400), ordered in authored time.
    pub civil_seconds: u32,
    /// Row in the contained 256 x N RGBA8 atlas.
    pub row: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolarEvent {
    Sunrise,
    Sunset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolarTimeOfDayKeyframe {
    pub event: SolarEvent,
    pub offset_minutes: i16,
    pub row: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResolvedPaletteSample {
    /// Solar events could not safely define a sample; use the base palette.
    Base,
    Rows {
        lower_row: u32,
        upper_row: u32,
        amount: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeOfDayPalette {
    pub rows: u32,
    /// Authored sRGB RGBA8 bytes, row-major, with 256 entries per row.
    pub rgba: Vec<u8>,
    /// Schema-v1 civil keyframes. Empty only for a solar schema-v2 timeline.
    pub keyframes: Vec<TimeOfDayKeyframe>,
    /// Present only for schema-v2 solar timelines.
    pub solar_keyframes: Option<Vec<SolarTimeOfDayKeyframe>>,
}

impl TimeOfDayPalette {
    pub fn new(
        rows: u32,
        rgba: Vec<u8>,
        keyframes: Vec<TimeOfDayKeyframe>,
    ) -> Result<Self, SceneError> {
        validate_time_of_day(rows, &rgba, &keyframes)?;
        Ok(Self {
            rows,
            rgba,
            keyframes,
            solar_keyframes: None,
        })
    }

    pub fn new_solar(
        rows: u32,
        rgba: Vec<u8>,
        keyframes: Vec<SolarTimeOfDayKeyframe>,
    ) -> Result<Self, SceneError> {
        validate_atlas(rows, &rgba)?;
        validate_solar_keyframes(rows, &keyframes)?;
        Ok(Self {
            rows,
            rgba,
            keyframes: Vec::new(),
            solar_keyframes: Some(keyframes),
        })
    }

    pub fn keyframe_count(&self) -> usize {
        self.solar_keyframes
            .as_ref()
            .map_or(self.keyframes.len(), Vec::len)
    }

    pub fn keyframe_bytes(&self) -> Vec<u8> {
        if let Some(keyframes) = &self.solar_keyframes {
            return keyframes
                .iter()
                .flat_map(|keyframe| {
                    [
                        0u32.to_ne_bytes(),
                        keyframe.row.to_ne_bytes(),
                        0u32.to_ne_bytes(),
                        0u32.to_ne_bytes(),
                    ]
                    .into_iter()
                    .flatten()
                })
                .collect();
        }
        self.keyframes
            .iter()
            .flat_map(|keyframe| {
                [
                    keyframe.civil_seconds.to_ne_bytes(),
                    keyframe.row.to_ne_bytes(),
                    0u32.to_ne_bytes(),
                    0u32.to_ne_bytes(),
                ]
                .into_iter()
                .flatten()
            })
            .collect()
    }

    /// Resolve a bounded row-pair/fraction using previous/current/next local
    /// dates in order. `None` means this is a civil timeline; `Base` is the
    /// specified fallback for missing, polar, duplicate, or unbracketed data.
    pub fn resolve_solar_sample(
        &self,
        utc_seconds: i64,
        events_by_local_date: &[SolarEvents; 3],
    ) -> Option<ResolvedPaletteSample> {
        let keyframes = self.solar_keyframes.as_ref()?;
        let mut anchors = Vec::with_capacity(keyframes.len() * events_by_local_date.len());
        for events in events_by_local_date {
            for keyframe in keyframes {
                let event_seconds = match keyframe.event {
                    SolarEvent::Sunrise => events.sunrise_utc_seconds,
                    SolarEvent::Sunset => events.sunset_utc_seconds,
                };
                let Some(event_seconds) = event_seconds else {
                    return Some(ResolvedPaletteSample::Base);
                };
                let Some(instant) =
                    event_seconds.checked_add(i64::from(keyframe.offset_minutes) * 60)
                else {
                    return Some(ResolvedPaletteSample::Base);
                };
                anchors.push((instant, keyframe.row));
            }
        }
        anchors.sort_unstable_by_key(|anchor| anchor.0);
        if anchors.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Some(ResolvedPaletteSample::Base);
        }
        let upper = anchors.partition_point(|anchor| anchor.0 <= utc_seconds);
        if upper == 0 || upper == anchors.len() {
            return Some(ResolvedPaletteSample::Base);
        }
        let (lower_seconds, lower_row) = anchors[upper - 1];
        let (upper_seconds, upper_row) = anchors[upper];
        let amount = (utc_seconds - lower_seconds) as f64 / (upper_seconds - lower_seconds) as f64;
        Some(ResolvedPaletteSample::Rows {
            lower_row,
            upper_row,
            amount: amount.clamp(0.0, 1.0) as f32,
        })
    }

    /// CPU reference for the shader's authored-byte civil timeline.
    #[cfg(test)]
    pub fn sample_rgb(&self, index: usize, civil_seconds: u32) -> [u8; 3] {
        assert!(index < 256);
        assert!(self.solar_keyframes.is_none());
        let civil_seconds = civil_seconds % CIVIL_SECONDS_PER_DAY;
        let lower = self
            .keyframes
            .iter()
            .rposition(|keyframe| keyframe.civil_seconds <= civil_seconds)
            .unwrap_or(self.keyframes.len() - 1);
        let upper = (lower + 1) % self.keyframes.len();
        let lower_seconds = self.keyframes[lower].civil_seconds;
        let mut upper_seconds = self.keyframes[upper].civil_seconds;
        let mut sample_seconds = civil_seconds;
        if upper_seconds <= lower_seconds {
            upper_seconds += CIVIL_SECONDS_PER_DAY;
        }
        if sample_seconds < lower_seconds {
            sample_seconds += CIVIL_SECONDS_PER_DAY;
        }
        let amount =
            f64::from(sample_seconds - lower_seconds) / f64::from(upper_seconds - lower_seconds);
        let lower_offset = (self.keyframes[lower].row as usize * 256 + index) * 4;
        let upper_offset = (self.keyframes[upper].row as usize * 256 + index) * 4;
        std::array::from_fn(|channel| {
            (f64::from(self.rgba[lower_offset + channel])
                + (f64::from(self.rgba[upper_offset + channel])
                    - f64::from(self.rgba[lower_offset + channel]))
                    * amount)
                .floor()
                .clamp(0.0, 255.0) as u8
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub image: IndexedImage,
    pub palette: Palette,
    pub animation: Animation,
    pub time_of_day: Option<TimeOfDayPalette>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneError(String);

impl fmt::Display for SceneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SceneError {}

impl Scene {
    pub fn new(image: IndexedImage, palette: Palette) -> Result<Self, SceneError> {
        validate_image_shape(image.width, image.height, image.indices.len())?;
        validate_palette(&palette.rgba)?;
        Ok(Self {
            image,
            palette,
            animation: Animation::static_default(),
            time_of_day: None,
        })
    }

    pub fn with_animation(
        image: IndexedImage,
        palette: Palette,
        animation: Animation,
    ) -> Result<Self, SceneError> {
        let mut scene = Self::new(image, palette)?;
        scene.animation = animation;
        Ok(scene)
    }

    pub fn with_resources(
        image: IndexedImage,
        palette: Palette,
        animation: Animation,
        time_of_day: Option<TimeOfDayPalette>,
    ) -> Result<Self, SceneError> {
        let mut scene = Self::with_animation(image, palette, animation)?;
        scene.time_of_day = time_of_day;
        Ok(scene)
    }

    /// A small original synthetic scene used when no external package is
    /// configured. It is a diagnostic/reference scene, not bundled artwork.
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

        // 3 × 2 deliberately includes index 255 and repeated colors.
        Self {
            image: IndexedImage {
                width: 3,
                height: 2,
                indices: vec![0, 1, 2, 255, 1, 0],
            },
            palette: Palette { rgba },
            animation: Animation::static_default(),
            time_of_day: None,
        }
    }
}

/// Decode an 8-bit grayscale or 8-bit indexed PNG while preserving labels.
pub fn decode_index_png(bytes: &[u8]) -> Result<IndexedImage, SceneError> {
    reject_encoded_size(bytes, "scene")?;

    let decoder = Decoder::new_with_limits(
        Cursor::new(bytes),
        Limits {
            bytes: MAX_DECODER_BYTES,
        },
    );
    let mut reader = decoder
        .read_info()
        .map_err(|error| SceneError(format!("scene PNG header/decode setup failed: {error}")))?;

    let (width, height, color_type, bit_depth, has_transparency, is_animation) = {
        let info = reader.info();
        (
            info.width,
            info.height,
            info.color_type,
            info.bit_depth,
            info.trns.is_some(),
            info.animation_control.is_some(),
        )
    };
    validate_dimensions(width, height, "scene")?;
    if is_animation {
        return Err(SceneError(
            "scene PNG animation is not supported in v1".into(),
        ));
    }
    if has_transparency {
        return Err(SceneError(
            "scene PNG transparency metadata is not allowed; indices are labels, not alpha".into(),
        ));
    }
    if bit_depth != BitDepth::Eight
        || !matches!(color_type, ColorType::Grayscale | ColorType::Indexed)
    {
        return Err(SceneError(format!(
            "scene PNG must be 8-bit grayscale or indexed; got {color_type:?} at {bit_depth:?}"
        )));
    }

    let output_size = reader.output_buffer_size();
    let expected_size = pixel_count(width, height)?;
    if output_size != expected_size {
        return Err(SceneError(format!(
            "scene PNG decoder output size {output_size} does not match {expected_size} raw indices"
        )));
    }
    let mut indices = vec![0u8; output_size];
    let output = reader
        .next_frame(&mut indices)
        .map_err(|error| SceneError(format!("scene PNG pixel decode failed: {error}")))?;
    if output.color_type != color_type || output.bit_depth != BitDepth::Eight {
        return Err(SceneError(format!(
            "scene PNG decoder changed raw index representation to {:?} at {:?}",
            output.color_type, output.bit_depth
        )));
    }
    indices.truncate(output.buffer_size());
    validate_image_shape(width, height, indices.len())?;
    Ok(IndexedImage {
        width,
        height,
        indices,
    })
}

/// Decode the authoritative 256 × 1 opaque RGBA8 palette PNG.
pub fn decode_palette_png(bytes: &[u8]) -> Result<Palette, SceneError> {
    reject_encoded_size(bytes, "palette")?;
    let decoder = Decoder::new_with_limits(
        Cursor::new(bytes),
        Limits {
            bytes: MAX_DECODER_BYTES,
        },
    );
    let mut reader = decoder
        .read_info()
        .map_err(|error| SceneError(format!("palette PNG header/decode setup failed: {error}")))?;

    let (width, height, color_type, bit_depth, has_transparency, is_animation) = {
        let info = reader.info();
        (
            info.width,
            info.height,
            info.color_type,
            info.bit_depth,
            info.trns.is_some(),
            info.animation_control.is_some(),
        )
    };
    if (width, height) != (256, 1) {
        return Err(SceneError(format!(
            "palette PNG must be exactly 256x1, got {width}x{height}"
        )));
    }
    if is_animation || has_transparency {
        return Err(SceneError(
            "palette PNG animation/transparency metadata is not allowed".into(),
        ));
    }
    if color_type != ColorType::Rgba || bit_depth != BitDepth::Eight {
        return Err(SceneError(format!(
            "palette PNG must be 8-bit RGBA, got {color_type:?} at {bit_depth:?}"
        )));
    }

    let output_size = reader.output_buffer_size();
    if output_size != 256 * 4 {
        return Err(SceneError(format!(
            "palette PNG decoder output size {output_size} is not 1024 RGBA bytes"
        )));
    }
    let mut bytes_out = vec![0u8; output_size];
    let output = reader
        .next_frame(&mut bytes_out)
        .map_err(|error| SceneError(format!("palette PNG pixel decode failed: {error}")))?;
    if output.color_type != ColorType::Rgba || output.bit_depth != BitDepth::Eight {
        return Err(SceneError(
            "palette PNG decoder changed the required RGBA8 representation".into(),
        ));
    }
    let mut rgba = [0u8; 256 * 4];
    rgba.copy_from_slice(&bytes_out);
    validate_palette(&rgba)?;
    Ok(Palette { rgba })
}

/// Load an optional civil-time palette timeline and its contained RGBA8 atlas.
/// The config and atlas must remain in the same package directory.
pub fn load_time_of_day(path: impl AsRef<Path>) -> Result<TimeOfDayPalette, SceneError> {
    let config_path = canonicalize_file(path.as_ref(), "time-of-day manifest")?;
    let package_dir = config_path
        .parent()
        .ok_or_else(|| SceneError("time-of-day manifest has no package directory".into()))?;
    let input = read_bounded(&config_path, "time-of-day manifest")?;
    let input = std::str::from_utf8(&input)
        .map_err(|error| SceneError(format!("time-of-day manifest is not UTF-8: {error}")))?;
    let values = parse_time_of_day_manifest(input)?;
    let atlas = values.atlas;
    let atlas_relative = Path::new(&atlas);
    if atlas_relative.is_absolute()
        || atlas_relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(SceneError(
            "time-of-day atlas path must stay inside the scene package".into(),
        ));
    }
    let atlas_path = canonicalize_file(&package_dir.join(atlas_relative), "time-of-day atlas")?;
    if !atlas_path.starts_with(package_dir) {
        return Err(SceneError(
            "time-of-day atlas path escapes the scene package".into(),
        ));
    }
    let atlas_bytes = read_bounded(&atlas_path, "time-of-day atlas")?;
    let (rows, rgba) = decode_time_of_day_png(&atlas_bytes)?;
    match values.schema_version {
        1 => TimeOfDayPalette::new(rows, rgba, values.civil_keyframes),
        2 => TimeOfDayPalette::new_solar(rows, rgba, values.solar_keyframes),
        _ => unreachable!("manifest parser validates schema versions"),
    }
}

/// Decode the contained 256 x N opaque RGBA8 time-of-day atlas.
pub fn decode_time_of_day_png(bytes: &[u8]) -> Result<(u32, Vec<u8>), SceneError> {
    reject_encoded_size(bytes, "time-of-day atlas")?;
    let decoder = Decoder::new_with_limits(
        Cursor::new(bytes),
        Limits {
            bytes: MAX_DECODER_BYTES,
        },
    );
    let mut reader = decoder.read_info().map_err(|error| {
        SceneError(format!(
            "time-of-day atlas PNG header/decode setup failed: {error}"
        ))
    })?;
    let (width, height, color_type, bit_depth, has_transparency, is_animation) = {
        let info = reader.info();
        (
            info.width,
            info.height,
            info.color_type,
            info.bit_depth,
            info.trns.is_some(),
            info.animation_control.is_some(),
        )
    };
    if width != 256 || height == 0 || height > MAX_TIME_OF_DAY_ROWS {
        return Err(SceneError(format!(
            "time-of-day atlas must be 256x1..{}, got {width}x{height}",
            MAX_TIME_OF_DAY_ROWS
        )));
    }
    if is_animation || has_transparency {
        return Err(SceneError(
            "time-of-day atlas animation/transparency metadata is not allowed".into(),
        ));
    }
    if color_type != ColorType::Rgba || bit_depth != BitDepth::Eight {
        return Err(SceneError(format!(
            "time-of-day atlas must be 8-bit RGBA, got {color_type:?} at {bit_depth:?}"
        )));
    }
    let output_size = reader.output_buffer_size();
    let expected_size = usize::try_from(height)
        .ok()
        .and_then(|height| height.checked_mul(256 * 4))
        .ok_or_else(|| SceneError("time-of-day atlas size overflows this platform".into()))?;
    if output_size != expected_size {
        return Err(SceneError(format!(
            "time-of-day atlas decoder output size {output_size} does not match {expected_size}"
        )));
    }
    let mut rgba = vec![0u8; output_size];
    let output = reader.next_frame(&mut rgba).map_err(|error| {
        SceneError(format!(
            "time-of-day atlas PNG pixel decode failed: {error}"
        ))
    })?;
    if output.color_type != ColorType::Rgba || output.bit_depth != BitDepth::Eight {
        return Err(SceneError(
            "time-of-day atlas decoder changed the required RGBA8 representation".into(),
        ));
    }
    if rgba.chunks_exact(4).any(|entry| entry[3] != 255) {
        return Err(SceneError(
            "time-of-day atlas entries must be opaque RGBA8 (alpha 255)".into(),
        ));
    }
    Ok((height, rgba))
}

/// Load the two S03 PNGs from a package directory. `scene.toml` is intentionally
/// outside this first fixed-scene slice; S05 owns package metadata and tags.
#[cfg(test)]
pub fn load_png_pair(
    scene_path: impl AsRef<Path>,
    palette_path: impl AsRef<Path>,
) -> Result<Scene, SceneError> {
    load_png_pair_with_animation(scene_path, palette_path, None::<&Path>)
}

/// Load the fixed S03 image/palette pair and an optional S04 animation file.
/// Package containment and scene.toml metadata remain S05 responsibilities.
#[allow(dead_code)]
pub fn load_png_pair_with_animation(
    scene_path: impl AsRef<Path>,
    palette_path: impl AsRef<Path>,
    animation_path: Option<impl AsRef<Path>>,
) -> Result<Scene, SceneError> {
    load_png_pair_with_resources(scene_path, palette_path, animation_path, None::<&Path>)
}

/// Load a scene package's immutable image, palette, animation, and optional
/// civil-time palette resources.
pub fn load_png_pair_with_resources(
    scene_path: impl AsRef<Path>,
    palette_path: impl AsRef<Path>,
    animation_path: Option<impl AsRef<Path>>,
    time_of_day_path: Option<impl AsRef<Path>>,
) -> Result<Scene, SceneError> {
    let scene_path = scene_path.as_ref();
    let palette_path = palette_path.as_ref();
    let scene_bytes = read_bounded(scene_path, "scene")?;
    let palette_bytes = read_bounded(palette_path, "palette")?;
    let animation = match animation_path {
        Some(path) => Animation::load(path).map_err(|error| SceneError(error.to_string()))?,
        None => Animation::static_default(),
    };
    let time_of_day = time_of_day_path.map(load_time_of_day).transpose()?;
    Scene::with_resources(
        decode_index_png(&scene_bytes)?,
        decode_palette_png(&palette_bytes)?,
        animation,
        time_of_day,
    )
}

/// Map an output pixel to a nearest-neighbor source pixel using centered cover.
pub fn cover_source_coord(source: (u32, u32), output: (u32, u32), pixel: (u32, u32)) -> (u32, u32) {
    assert!(source.0 > 0 && source.1 > 0 && output.0 > 0 && output.1 > 0);
    assert!(pixel.0 < output.0 && pixel.1 < output.1);

    let source_width = f64::from(source.0);
    let source_height = f64::from(source.1);
    let output_width = f64::from(output.0);
    let output_height = f64::from(output.1);
    let scale = (output_width / source_width).max(output_height / source_height);
    let crop_x = (source_width * scale - output_width) / 2.0;
    let crop_y = (source_height * scale - output_height) / 2.0;

    let x = ((f64::from(pixel.0) + 0.5 + crop_x) / scale).floor();
    let y = ((f64::from(pixel.1) + 0.5 + crop_y) / scale).floor();
    (
        x.clamp(0.0, source_width - 1.0) as u32,
        y.clamp(0.0, source_height - 1.0) as u32,
    )
}

struct TimeOfDayManifestValues {
    schema_version: u32,
    atlas: String,
    civil_keyframes: Vec<TimeOfDayKeyframe>,
    solar_keyframes: Vec<SolarTimeOfDayKeyframe>,
}

#[derive(Default)]
struct TimeOfDayManifestBuilder {
    schema_version: Option<u32>,
    basis: Option<String>,
    atlas: Option<String>,
    keyframes: Vec<TimeOfDayKeyframeBuilder>,
}

#[derive(Default)]
struct TimeOfDayKeyframeBuilder {
    civil_seconds: Option<u32>,
    event: Option<SolarEvent>,
    offset_minutes: Option<i16>,
    row: Option<u32>,
}

fn parse_time_of_day_manifest(input: &str) -> Result<TimeOfDayManifestValues, SceneError> {
    let mut values = TimeOfDayManifestBuilder::default();
    let mut current: Option<TimeOfDayKeyframeBuilder> = None;
    for (line_number, raw_line) in input.lines().enumerate() {
        let line_number = line_number + 1;
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if line == "[[keyframe]]" {
            if let Some(builder) = current.take() {
                values.keyframes.push(builder);
            }
            current = Some(TimeOfDayKeyframeBuilder::default());
            continue;
        }
        let (key, raw_value) = line.split_once('=').ok_or_else(|| {
            SceneError(format!(
                "time-of-day line {line_number}: expected key = value"
            ))
        })?;
        let key = key.trim();
        let value = raw_value.trim();
        if let Some(keyframe) = current.as_mut() {
            match key {
                "civil_seconds" => {
                    assign_time_of_day_u32(&mut keyframe.civil_seconds, value, line_number, key)?
                }
                "event" => {
                    let event = parse_time_of_day_string(value, line_number, key)?;
                    if keyframe.event.is_some() {
                        return Err(SceneError(format!(
                            "time-of-day line {line_number}: duplicate key {key:?}"
                        )));
                    }
                    keyframe.event = Some(match event.as_str() {
                        "sunrise" => SolarEvent::Sunrise,
                        "sunset" => SolarEvent::Sunset,
                        _ => {
                            return Err(SceneError(format!(
                                "time-of-day line {line_number}: event must be sunrise or sunset"
                            )));
                        }
                    });
                }
                "offset_minutes" => {
                    assign_time_of_day_i16(&mut keyframe.offset_minutes, value, line_number, key)?
                }
                "row" => assign_time_of_day_u32(&mut keyframe.row, value, line_number, key)?,
                _ => {
                    return Err(SceneError(format!(
                        "time-of-day line {line_number}: unknown key {key:?}"
                    )));
                }
            }
        } else {
            match key {
                "schema_version" => {
                    assign_time_of_day_u32(&mut values.schema_version, value, line_number, key)?
                }
                "basis" | "atlas" => {
                    let slot = if key == "basis" {
                        &mut values.basis
                    } else {
                        &mut values.atlas
                    };
                    if slot.is_some() {
                        return Err(SceneError(format!(
                            "time-of-day line {line_number}: duplicate key {key:?}"
                        )));
                    }
                    *slot = Some(parse_time_of_day_string(value, line_number, key)?);
                }
                _ => {
                    return Err(SceneError(format!(
                        "time-of-day line {line_number}: unknown top-level key {key:?}"
                    )));
                }
            }
        }
    }
    if let Some(builder) = current {
        values.keyframes.push(builder);
    }
    let schema_version = values
        .schema_version
        .ok_or_else(|| SceneError("time-of-day schema_version is required".into()))?;
    let atlas = values
        .atlas
        .filter(|atlas| !atlas.is_empty())
        .ok_or_else(|| SceneError("time-of-day atlas is required".into()))?;
    let line = input.lines().count().max(1);
    let (civil_keyframes, solar_keyframes) = match (schema_version, values.basis.as_deref()) {
        (1, None) => {
            let keyframes = values
                .keyframes
                .into_iter()
                .map(|builder| finish_civil_time_of_day_keyframe(builder, line))
                .collect::<Result<Vec<_>, _>>()?;
            (keyframes, Vec::new())
        }
        (2, Some("solar")) => {
            let keyframes = values
                .keyframes
                .into_iter()
                .map(|builder| finish_solar_time_of_day_keyframe(builder, line))
                .collect::<Result<Vec<_>, _>>()?;
            (Vec::new(), keyframes)
        }
        (1, Some(_)) => {
            return Err(SceneError(
                "time-of-day schema v1 does not support basis".into(),
            ));
        }
        (2, _) => {
            return Err(SceneError(
                "time-of-day schema v2 requires basis = \"solar\"".into(),
            ));
        }
        (version, _) => {
            return Err(SceneError(format!(
                "time-of-day schema_version must be 1 or 2, got {version}"
            )));
        }
    };
    Ok(TimeOfDayManifestValues {
        schema_version,
        atlas,
        civil_keyframes,
        solar_keyframes,
    })
}

fn finish_civil_time_of_day_keyframe(
    builder: TimeOfDayKeyframeBuilder,
    line: usize,
) -> Result<TimeOfDayKeyframe, SceneError> {
    if builder.event.is_some() || builder.offset_minutes.is_some() {
        return Err(SceneError(format!(
            "time-of-day line {line}: schema v1 accepts only civil_seconds and row"
        )));
    }
    Ok(TimeOfDayKeyframe {
        civil_seconds: builder.civil_seconds.ok_or_else(|| {
            SceneError(format!(
                "time-of-day line {line}: civil_seconds is required"
            ))
        })?,
        row: builder
            .row
            .ok_or_else(|| SceneError(format!("time-of-day line {line}: row is required")))?,
    })
}

fn finish_solar_time_of_day_keyframe(
    builder: TimeOfDayKeyframeBuilder,
    line: usize,
) -> Result<SolarTimeOfDayKeyframe, SceneError> {
    if builder.civil_seconds.is_some() {
        return Err(SceneError(format!(
            "time-of-day line {line}: schema v2 accepts event anchors, not civil_seconds"
        )));
    }
    Ok(SolarTimeOfDayKeyframe {
        event: builder
            .event
            .ok_or_else(|| SceneError(format!("time-of-day line {line}: event is required")))?,
        offset_minutes: builder.offset_minutes.ok_or_else(|| {
            SceneError(format!(
                "time-of-day line {line}: offset_minutes is required"
            ))
        })?,
        row: builder
            .row
            .ok_or_else(|| SceneError(format!("time-of-day line {line}: row is required")))?,
    })
}

fn assign_time_of_day_u32(
    slot: &mut Option<u32>,
    value: &str,
    line: usize,
    key: &str,
) -> Result<(), SceneError> {
    if slot.is_some() {
        return Err(SceneError(format!(
            "time-of-day line {line}: duplicate key {key:?}"
        )));
    }
    *slot = Some(value.parse::<u32>().map_err(|_| {
        SceneError(format!(
            "time-of-day line {line}: {key} must be an unsigned integer"
        ))
    })?);
    Ok(())
}

fn assign_time_of_day_i16(
    slot: &mut Option<i16>,
    value: &str,
    line: usize,
    key: &str,
) -> Result<(), SceneError> {
    if slot.is_some() {
        return Err(SceneError(format!(
            "time-of-day line {line}: duplicate key {key:?}"
        )));
    }
    *slot = Some(value.parse::<i16>().map_err(|_| {
        SceneError(format!(
            "time-of-day line {line}: {key} must be a signed integer"
        ))
    })?);
    Ok(())
}

fn parse_time_of_day_string(value: &str, line: usize, key: &str) -> Result<String, SceneError> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .filter(|value| !value.contains('"') && !value.contains('\n'))
        .map(str::to_owned)
        .ok_or_else(|| {
            SceneError(format!(
                "time-of-day line {line}: {key} must be a quoted string"
            ))
        })
}

fn validate_atlas(rows: u32, rgba: &[u8]) -> Result<(), SceneError> {
    if !(1..=MAX_TIME_OF_DAY_ROWS).contains(&rows) {
        return Err(SceneError(format!(
            "time-of-day atlas has invalid row count {rows}"
        )));
    }
    let expected = usize::try_from(rows)
        .ok()
        .and_then(|rows| rows.checked_mul(256 * 4))
        .ok_or_else(|| SceneError("time-of-day atlas size overflows this platform".into()))?;
    if rgba.len() != expected {
        return Err(SceneError(format!(
            "time-of-day atlas has {} bytes, expected {expected}",
            rgba.len()
        )));
    }
    if rgba.chunks_exact(4).any(|entry| entry[3] != 255) {
        return Err(SceneError(
            "time-of-day atlas entries must be opaque RGBA8 (alpha 255)".into(),
        ));
    }
    Ok(())
}

fn validate_time_of_day(
    rows: u32,
    rgba: &[u8],
    keyframes: &[TimeOfDayKeyframe],
) -> Result<(), SceneError> {
    validate_atlas(rows, rgba)?;
    if keyframes.len() < 2 {
        return Err(SceneError(
            "time-of-day timeline requires at least two keyframes".into(),
        ));
    }
    if keyframes.len() > MAX_TIME_OF_DAY_KEYFRAMES {
        return Err(SceneError(
            "time-of-day timeline exceeds the bounded keyframe limit".into(),
        ));
    }
    let mut previous = None;
    for keyframe in keyframes {
        if keyframe.civil_seconds >= CIVIL_SECONDS_PER_DAY {
            return Err(SceneError(format!(
                "time-of-day keyframe second {} is outside one civil day",
                keyframe.civil_seconds
            )));
        }
        if previous.is_some_and(|value| keyframe.civil_seconds <= value) {
            return Err(SceneError(
                "time-of-day keyframes must have strictly increasing civil seconds".into(),
            ));
        }
        previous = Some(keyframe.civil_seconds);
        if keyframe.row >= rows {
            return Err(SceneError(format!(
                "time-of-day keyframe row {} is outside the atlas",
                keyframe.row
            )));
        }
    }
    Ok(())
}

fn validate_solar_keyframes(
    rows: u32,
    keyframes: &[SolarTimeOfDayKeyframe],
) -> Result<(), SceneError> {
    if keyframes.len() < 2 {
        return Err(SceneError(
            "solar time-of-day timeline requires at least two keyframes".into(),
        ));
    }
    if keyframes.len() > MAX_TIME_OF_DAY_KEYFRAMES {
        return Err(SceneError(
            "solar time-of-day timeline exceeds the bounded keyframe limit".into(),
        ));
    }
    for keyframe in keyframes {
        if !(-720..=720).contains(&keyframe.offset_minutes) {
            return Err(SceneError(
                "solar time-of-day offset_minutes must be between -720 and 720".into(),
            ));
        }
        if keyframe.row >= rows {
            return Err(SceneError(format!(
                "solar time-of-day keyframe row {} is outside the atlas",
                keyframe.row
            )));
        }
    }
    Ok(())
}

fn canonicalize_file(path: &Path, label: &str) -> Result<std::path::PathBuf, SceneError> {
    let path = path.canonicalize().map_err(|error| {
        SceneError(format!(
            "canonicalizing {label} {} failed: {error}",
            path.display()
        ))
    })?;
    if !path.is_file() {
        return Err(SceneError(format!(
            "{label} {} is not a file",
            path.display()
        )));
    }
    Ok(path)
}

fn read_bounded(path: &Path, label: &str) -> Result<Vec<u8>, SceneError> {
    let file = File::open(path)
        .map_err(|error| SceneError(format!("reading {} failed: {error}", path.display())))?;
    if let Ok(metadata) = file.metadata() {
        if metadata.len() > (MAX_ENCODED_BYTES as u64) {
            return Err(SceneError(format!(
                "{label} PNG encoded size {} exceeds {} bytes",
                metadata.len(),
                MAX_ENCODED_BYTES
            )));
        }
    }

    let mut bytes = Vec::with_capacity(
        MAX_ENCODED_BYTES.min(
            file.metadata()
                .map(|metadata| metadata.len() as usize)
                .unwrap_or(0),
        ),
    );
    file.take((MAX_ENCODED_BYTES as u64) + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| SceneError(format!("reading {} failed: {error}", path.display())))?;
    reject_encoded_size(&bytes, label)?;
    Ok(bytes)
}

fn reject_encoded_size(bytes: &[u8], label: &str) -> Result<(), SceneError> {
    if bytes.len() > MAX_ENCODED_BYTES {
        return Err(SceneError(format!(
            "{label} PNG encoded size {} exceeds {} bytes",
            bytes.len(),
            MAX_ENCODED_BYTES
        )));
    }
    Ok(())
}

fn validate_dimensions(width: u32, height: u32, label: &str) -> Result<(), SceneError> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| SceneError(format!("{label} dimensions overflow")))?;
    if width == 0 || height == 0 || pixels > MAX_PIXELS {
        return Err(SceneError(format!(
            "{label} dimensions {width}x{height} exceed nonzero {}-pixel limit",
            MAX_PIXELS
        )));
    }
    Ok(())
}

fn pixel_count(width: u32, height: u32) -> Result<usize, SceneError> {
    let count = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| SceneError("pixel count overflow".into()))?;
    usize::try_from(count).map_err(|_| SceneError("pixel count does not fit this platform".into()))
}

fn validate_image_shape(width: u32, height: u32, bytes: usize) -> Result<(), SceneError> {
    validate_dimensions(width, height, "scene")?;
    let expected = pixel_count(width, height)?;
    if bytes != expected {
        return Err(SceneError(format!(
            "scene has {bytes} index bytes, expected {expected}"
        )));
    }
    Ok(())
}

fn validate_palette(rgba: &[u8; 256 * 4]) -> Result<(), SceneError> {
    if rgba.chunks_exact(4).any(|entry| entry[3] != 255) {
        return Err(SceneError(
            "palette entries must be opaque RGBA8 (alpha 255)".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use png::{BitDepth, ColorType, Encoder};

    fn encode_indexed(width: u32, height: u32, indices: &[u8], transparent: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = Encoder::new(&mut bytes, width, height);
            encoder.set_color(ColorType::Indexed);
            encoder.set_depth(BitDepth::Eight);
            encoder.set_palette(
                (0u16..=255)
                    .flat_map(|value| [value as u8, 255u8.saturating_sub(value as u8), 0])
                    .collect::<Vec<u8>>(),
            );
            if transparent {
                encoder.set_trns(vec![0]);
            }
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(indices).unwrap();
        }
        bytes
    }

    fn encode_grayscale(width: u32, height: u32, indices: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = Encoder::new(&mut bytes, width, height);
            encoder.set_color(ColorType::Grayscale);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(indices).unwrap();
        }
        bytes
    }

    fn encode_palette(rgba: &[u8]) -> Vec<u8> {
        encode_rgba(256, 1, rgba)
    }

    fn encode_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = Encoder::new(&mut bytes, width, height);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(rgba).unwrap();
        }
        bytes
    }

    #[test]
    fn grayscale_path_preserves_zero_and_255() {
        let bytes = encode_grayscale(3, 2, &[0, 1, 255, 2, 255, 0]);
        let image = decode_index_png(&bytes).unwrap();
        assert_eq!(image.width, 3);
        assert_eq!(image.height, 2);
        assert_eq!(image.indices, vec![0, 1, 255, 2, 255, 0]);
    }

    #[test]
    fn indexed_path_preserves_raw_labels_without_palette_expansion() {
        let bytes = encode_indexed(3, 2, &[0, 255, 1, 2, 255, 0], false);
        let image = decode_index_png(&bytes).unwrap();
        assert_eq!(image.indices, vec![0, 255, 1, 2, 255, 0]);
    }

    #[test]
    fn both_supported_encodings_preserve_every_8_bit_label() {
        let expected: Vec<u8> = (0..=255).collect();
        let grayscale = decode_index_png(&encode_grayscale(256, 1, &expected)).unwrap();
        let indexed = decode_index_png(&encode_indexed(256, 1, &expected, false)).unwrap();
        assert_eq!(grayscale.indices, expected);
        assert_eq!(indexed.indices, expected);
    }

    #[test]
    fn transparency_and_wrong_shapes_are_rejected() {
        let transparent = encode_indexed(1, 1, &[0], true);
        assert!(decode_index_png(&transparent).is_err());
        assert!(decode_index_png(&encode_palette(&[255; 256 * 4])).is_err());
        assert!(decode_palette_png(&encode_grayscale(1, 1, &[0])).is_err());
    }

    #[test]
    fn palette_requires_opaque_256_rgba_entries() {
        let mut rgba = vec![0u8; 256 * 4];
        for entry in rgba.chunks_exact_mut(4) {
            entry[3] = 255;
        }
        rgba[4 * 255 + 3] = 0;
        let error = decode_palette_png(&encode_palette(&rgba)).unwrap_err();
        assert!(error.to_string().contains("opaque"));
    }

    #[test]
    fn package_pair_loader_reads_bounded_png_inputs() {
        let directory =
            std::env::temp_dir().join(format!("worldwall-s03-scene-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir(&directory).unwrap();
        let mut rgba = vec![0u8; 256 * 4];
        for entry in rgba.chunks_exact_mut(4) {
            entry[3] = 255;
        }
        std::fs::write(
            directory.join("scene.png"),
            encode_grayscale(2, 1, &[0, 255]),
        )
        .unwrap();
        std::fs::write(directory.join("palette.png"), encode_palette(&rgba)).unwrap();

        let scene =
            load_png_pair(directory.join("scene.png"), directory.join("palette.png")).unwrap();
        assert_eq!(scene.image.indices, vec![0, 255]);
        assert_eq!(scene.palette.rgba[3], 255);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn time_of_day_timeline_validates_wrap_and_authored_byte_flooring() {
        let mut rgba = vec![255u8; 2 * 256 * 4];
        rgba[0..4].copy_from_slice(&[10, 20, 30, 255]);
        rgba[(256 * 4)..(256 * 4 + 4)].copy_from_slice(&[20, 10, 40, 255]);
        let timeline = TimeOfDayPalette::new(
            2,
            rgba,
            vec![
                TimeOfDayKeyframe {
                    civil_seconds: 10,
                    row: 0,
                },
                TimeOfDayKeyframe {
                    civil_seconds: 20,
                    row: 1,
                },
            ],
        )
        .unwrap();
        assert_eq!(timeline.sample_rgb(0, 15), [15, 15, 35]);
        assert_eq!(timeline.sample_rgb(0, 86_399), [10, 19, 30]);
        assert!(
            TimeOfDayPalette::new(
                2,
                timeline.rgba.clone(),
                vec![TimeOfDayKeyframe {
                    civil_seconds: 20,
                    row: 0,
                }]
            )
            .is_err()
        );
        assert!(
            TimeOfDayPalette::new(
                2,
                timeline.rgba.clone(),
                vec![
                    TimeOfDayKeyframe {
                        civil_seconds: 20,
                        row: 0,
                    },
                    TimeOfDayKeyframe {
                        civil_seconds: 20,
                        row: 1,
                    },
                ]
            )
            .is_err()
        );
        assert!(
            TimeOfDayPalette::new(
                1,
                vec![255; 256 * 4],
                (0..=MAX_TIME_OF_DAY_KEYFRAMES)
                    .map(|civil_seconds| TimeOfDayKeyframe {
                        civil_seconds: civil_seconds as u32,
                        row: 0,
                    })
                    .collect(),
            )
            .is_err()
        );
    }

    #[test]
    fn solar_timeline_resolves_event_offsets_wrap_and_fallbacks() {
        use crate::environment::{CivilDate, SolarEvents};

        let today = CivilDate::new(2024, 6, 21)
            .unwrap()
            .unix_seconds_at_midnight();
        let previous = today - i64::from(CIVIL_SECONDS_PER_DAY);
        let next = today + i64::from(CIVIL_SECONDS_PER_DAY);
        let events = [
            SolarEvents::normal(previous + 6 * 3_600, previous + 18 * 3_600),
            SolarEvents::normal(today + 6 * 3_600, today + 18 * 3_600),
            SolarEvents::normal(next + 6 * 3_600, next + 18 * 3_600),
        ];
        let timeline = TimeOfDayPalette::new_solar(
            4,
            vec![255; 4 * 256 * 4],
            vec![
                SolarTimeOfDayKeyframe {
                    event: SolarEvent::Sunrise,
                    offset_minutes: -30,
                    row: 0,
                },
                SolarTimeOfDayKeyframe {
                    event: SolarEvent::Sunrise,
                    offset_minutes: 30,
                    row: 1,
                },
                SolarTimeOfDayKeyframe {
                    event: SolarEvent::Sunset,
                    offset_minutes: -30,
                    row: 2,
                },
                SolarTimeOfDayKeyframe {
                    event: SolarEvent::Sunset,
                    offset_minutes: 30,
                    row: 3,
                },
            ],
        )
        .unwrap();
        assert_eq!(
            timeline.resolve_solar_sample(today + 6 * 3_600, &events),
            Some(ResolvedPaletteSample::Rows {
                lower_row: 0,
                upper_row: 1,
                amount: 0.5
            })
        );
        assert_eq!(
            timeline.resolve_solar_sample(today, &events),
            Some(ResolvedPaletteSample::Rows {
                lower_row: 3,
                upper_row: 0,
                amount: 0.5
            })
        );
        let polar = [events[0], SolarEvents::polar_day(), events[2]];
        assert_eq!(
            timeline.resolve_solar_sample(today, &polar),
            Some(ResolvedPaletteSample::Base)
        );
        let duplicate = TimeOfDayPalette::new_solar(
            2,
            vec![255; 2 * 256 * 4],
            vec![
                SolarTimeOfDayKeyframe {
                    event: SolarEvent::Sunrise,
                    offset_minutes: 0,
                    row: 0,
                },
                SolarTimeOfDayKeyframe {
                    event: SolarEvent::Sunrise,
                    offset_minutes: 0,
                    row: 1,
                },
            ],
        )
        .unwrap();
        assert_eq!(
            duplicate.resolve_solar_sample(today, &events),
            Some(ResolvedPaletteSample::Base)
        );
        assert!(
            TimeOfDayPalette::new_solar(
                1,
                vec![255; 256 * 4],
                (0..=MAX_TIME_OF_DAY_KEYFRAMES)
                    .map(|_| SolarTimeOfDayKeyframe {
                        event: SolarEvent::Sunrise,
                        offset_minutes: 0,
                        row: 0,
                    })
                    .collect(),
            )
            .is_err()
        );
    }

    #[test]
    fn solar_timeline_covers_event_boundaries_daily_movement_and_unsafe_fallbacks() {
        use crate::environment::{CivilDate, SolarEvents};

        let date = CivilDate::new(2024, 6, 21).unwrap();
        let midnight = date.unix_seconds_at_midnight();
        let event = |date: CivilDate, sunrise_hour: i64, sunset_hour: i64| {
            let start = date.unix_seconds_at_midnight();
            SolarEvents::normal(start + sunrise_hour * 3_600, start + sunset_hour * 3_600)
        };
        let events = [
            event(date.add_days(-1).unwrap(), 6, 18),
            event(date, 6, 18),
            event(date.add_days(1).unwrap(), 6, 18),
        ];
        let timeline = TimeOfDayPalette::new_solar(
            4,
            vec![255; 4 * 256 * 4],
            vec![
                SolarTimeOfDayKeyframe {
                    event: SolarEvent::Sunrise,
                    offset_minutes: -30,
                    row: 0,
                },
                SolarTimeOfDayKeyframe {
                    event: SolarEvent::Sunrise,
                    offset_minutes: 30,
                    row: 1,
                },
                SolarTimeOfDayKeyframe {
                    event: SolarEvent::Sunset,
                    offset_minutes: -30,
                    row: 2,
                },
                SolarTimeOfDayKeyframe {
                    event: SolarEvent::Sunset,
                    offset_minutes: 30,
                    row: 3,
                },
            ],
        )
        .unwrap();

        let cases = [
            (midnight + 5 * 3_600 + 30 * 60, 0, 1, 0.0),
            (midnight + 6 * 3_600, 0, 1, 0.5),
            (midnight + 6 * 3_600 + 30 * 60, 1, 2, 0.0),
            (midnight + 17 * 3_600 + 30 * 60, 2, 3, 0.0),
            (midnight + 18 * 3_600, 2, 3, 0.5),
            (midnight + 18 * 3_600 + 30 * 60, 3, 0, 0.0),
        ];
        for (utc_seconds, lower_row, upper_row, amount) in cases {
            assert_eq!(
                timeline.resolve_solar_sample(utc_seconds, &events),
                Some(ResolvedPaletteSample::Rows {
                    lower_row,
                    upper_row,
                    amount,
                }),
                "unexpected sample at UTC second {utc_seconds}"
            );
        }

        let later = date.add_days(1).unwrap();
        let later_midnight = later.unix_seconds_at_midnight();
        let later_events = [
            event(date, 6, 18),
            event(later, 7, 17),
            event(later.add_days(1).unwrap(), 7, 17),
        ];
        assert_eq!(
            timeline.resolve_solar_sample(midnight + 6 * 3_600 + 30 * 60, &events),
            Some(ResolvedPaletteSample::Rows {
                lower_row: 1,
                upper_row: 2,
                amount: 0.0,
            })
        );
        assert_eq!(
            timeline.resolve_solar_sample(later_midnight + 6 * 3_600 + 30 * 60, &later_events),
            Some(ResolvedPaletteSample::Rows {
                lower_row: 0,
                upper_row: 1,
                amount: 0.0,
            }),
            "the same civil clock time should track the later sunrise"
        );

        for unsafe_events in [
            [events[0], SolarEvents::polar_day(), events[2]],
            [events[0], SolarEvents::polar_night(), events[2]],
            [events[0], SolarEvents::unavailable(), events[2]],
            [
                events[0],
                SolarEvents::normal(midnight + 6 * 3_600, i64::MAX),
                events[2],
            ],
        ] {
            assert_eq!(
                timeline.resolve_solar_sample(midnight + 12 * 3_600, &unsafe_events),
                Some(ResolvedPaletteSample::Base)
            );
        }
        assert_eq!(
            timeline.resolve_solar_sample(i64::MIN, &events),
            Some(ResolvedPaletteSample::Base)
        );
        assert_eq!(
            timeline.resolve_solar_sample(i64::MAX, &events),
            Some(ResolvedPaletteSample::Base)
        );
    }

    #[test]
    fn solar_manifest_schema_v2_is_parsed_and_validated() {
        let directory =
            std::env::temp_dir().join(format!("worldwall-s06-time-of-day-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir(&directory).unwrap();
        let rgba = vec![255u8; 2 * 256 * 4];
        std::fs::write(
            directory.join("time_of_day.png"),
            encode_rgba(256, 2, &rgba),
        )
        .unwrap();
        std::fs::write(
            directory.join("time_of_day.toml"),
            "schema_version = 1\natlas = \"time_of_day.png\"\n\n[[keyframe]]\ncivil_seconds = 0\nrow = 0\n\n[[keyframe]]\ncivil_seconds = 43200\nrow = 1\n",
        )
        .unwrap();
        let timeline = load_time_of_day(directory.join("time_of_day.toml")).unwrap();
        assert_eq!(timeline.rows, 2);
        assert_eq!(timeline.keyframes.len(), 2);
        assert!(timeline.solar_keyframes.is_none());

        std::fs::write(
            directory.join("time_of_day.png"),
            encode_rgba(256, 4, &vec![255; 4 * 256 * 4]),
        )
        .unwrap();
        std::fs::write(
            directory.join("time_of_day.toml"),
            "schema_version = 2\nbasis = \"solar\"\natlas = \"time_of_day.png\"\n\n[[keyframe]]\nevent = \"sunrise\"\noffset_minutes = -30\nrow = 0\n\n[[keyframe]]\nevent = \"sunset\"\noffset_minutes = 30\nrow = 3\n",
        )
        .unwrap();
        let solar = load_time_of_day(directory.join("time_of_day.toml")).unwrap();
        assert_eq!(solar.keyframe_count(), 2);
        assert!(solar.keyframes.is_empty());
        assert_eq!(
            solar.solar_keyframes.as_ref().unwrap()[0].offset_minutes,
            -30
        );

        std::fs::write(
            directory.join("time_of_day.toml"),
            "schema_version = 2\nbasis = \"solar\"\natlas = \"time_of_day.png\"\n\n[[keyframe]]\nevent = \"sunrise\"\noffset_minutes = 721\nrow = 0\n\n[[keyframe]]\nevent = \"sunset\"\noffset_minutes = 30\nrow = 3\n",
        )
        .unwrap();
        assert!(load_time_of_day(directory.join("time_of_day.toml")).is_err());
        for malformed in [
            "schema_version = 2\natlas = \"time_of_day.png\"\n\n[[keyframe]]\nevent = \"sunrise\"\noffset_minutes = 0\nrow = 0\n\n[[keyframe]]\nevent = \"sunset\"\noffset_minutes = 0\nrow = 1\n",
            "schema_version = 2\nbasis = \"civil\"\natlas = \"time_of_day.png\"\n\n[[keyframe]]\nevent = \"sunrise\"\noffset_minutes = 0\nrow = 0\n\n[[keyframe]]\nevent = \"sunset\"\noffset_minutes = 0\nrow = 1\n",
            "schema_version = 2\nbasis = \"solar\"\natlas = \"time_of_day.png\"\n\n[[keyframe]]\nevent = \"noon\"\noffset_minutes = 0\nrow = 0\n\n[[keyframe]]\nevent = \"sunset\"\noffset_minutes = 0\nrow = 1\n",
            "schema_version = 2\nbasis = \"solar\"\natlas = \"time_of_day.png\"\n\n[[keyframe]]\nevent = \"sunrise\"\nrow = 0\n\n[[keyframe]]\nevent = \"sunset\"\noffset_minutes = 0\nrow = 1\n",
            "schema_version = 2\nbasis = \"solar\"\natlas = \"time_of_day.png\"\n\n[[keyframe]]\nevent = \"sunrise\"\noffset_minutes = 0\noffset_minutes = 1\nrow = 0\n\n[[keyframe]]\nevent = \"sunset\"\noffset_minutes = 0\nrow = 1\n",
            "schema_version = 2\nbasis = \"solar\"\natlas = \"time_of_day.png\"\n\n[[keyframe]]\nevent = \"sunrise\"\noffset_minutes = 0\ncivil_seconds = 60\nrow = 0\n\n[[keyframe]]\nevent = \"sunset\"\noffset_minutes = 0\nrow = 1\n",
        ] {
            assert!(parse_time_of_day_manifest(malformed).is_err());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn scene_rejects_corrupt_and_oversized_encoded_input_before_decode() {
        assert!(decode_index_png(b"not a PNG").is_err());
        let error = decode_index_png(&vec![0; MAX_ENCODED_BYTES + 1]).unwrap_err();
        assert!(error.to_string().contains("encoded size"));
    }

    #[test]
    fn cover_mapping_is_centered_nearest_and_clamped() {
        assert_eq!(cover_source_coord((2, 2), (4, 4), (0, 0)), (0, 0));
        assert_eq!(cover_source_coord((2, 2), (4, 4), (1, 0)), (0, 0));
        assert_eq!(cover_source_coord((2, 2), (4, 4), (2, 3)), (1, 1));
        assert_eq!(cover_source_coord((4, 2), (2, 2), (0, 0)), (1, 0));
        assert_eq!(cover_source_coord((3, 2), (4, 2), (3, 1)), (2, 1));
        assert_eq!(cover_source_coord((2, 4), (2, 2), (0, 0)), (0, 1));
        assert_eq!(cover_source_coord((2, 4), (2, 2), (1, 1)), (1, 2));
    }
}
