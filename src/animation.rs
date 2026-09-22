//! Versioned palette animation authoring and GPU metadata.
//!
//! The authoring format is deliberately small and strict.  Parsing and all
//! validation happen at scene load; the renderer receives immutable per-index
//! metadata and only updates one bounded animation-clock uniform per frame.

use std::{error::Error, fmt, fs, path::Path};

pub const ANIMATION_SCHEMA_VERSION: u32 = 1;
const MAX_SPEED_CYCLES_PER_SECOND: f32 = 16.0;
const PALETTE_ENTRIES: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub groups: Vec<AnimationGroup>,
    pub metadata: [GpuPaletteMetadata; PALETTE_ENTRIES],
    pub cycle_indices: [u32; PALETTE_ENTRIES],
    effects: [PaletteEffect; PALETTE_ENTRIES],
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationGroup {
    pub name: String,
    pub effect: EffectKind,
    pub members: Vec<u8>,
    pub speed_cycles_per_second: f32,
    pub amplitude: f32,
    pub phase: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    Static,
    HueCycle,
    BrightnessPulse,
    PaletteCycle,
    BlendShift,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PaletteEffect {
    Static,
    Hue {
        speed: f32,
        amplitude: f32,
        phase: f32,
    },
    Pulse {
        speed: f32,
        amplitude: f32,
        phase: f32,
    },
    Cycle {
        offset: u32,
        count: u32,
        ordinal: u32,
        speed: f32,
        phase: f32,
    },
    BlendShift {
        offset: u32,
        count: u32,
        ordinal: u32,
        speed: f32,
        phase: f32,
    },
}

/// One 32-byte read-only storage-buffer entry.  Every palette slot has one,
/// including static slots, so shader indexing never depends on optional data.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuPaletteMetadata {
    pub effect: u32,
    pub group_offset: u32,
    pub member_count: u32,
    pub ordinal: u32,
    pub speed: f32,
    pub amplitude: f32,
    pub phase: f32,
    pub reserved: f32,
}

impl GpuPaletteMetadata {
    pub const fn static_entry() -> Self {
        Self {
            effect: 0,
            group_offset: 0,
            member_count: 1,
            ordinal: 0,
            speed: 0.0,
            amplitude: 0.0,
            phase: 0.0,
            reserved: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationError(String);

impl fmt::Display for AnimationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for AnimationError {}

impl Animation {
    pub fn static_default() -> Self {
        Self {
            groups: Vec::new(),
            metadata: [GpuPaletteMetadata::static_entry(); PALETTE_ENTRIES],
            cycle_indices: [0; PALETTE_ENTRIES],
            effects: [PaletteEffect::Static; PALETTE_ENTRIES],
        }
    }

    pub fn from_toml(input: &str) -> Result<Self, AnimationError> {
        let mut schema_version = None;
        let mut groups = Vec::new();
        let mut current: Option<GroupBuilder> = None;

        for (line_number, raw_line) in input.lines().enumerate() {
            let line_number = line_number + 1;
            let line = raw_line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            if line == "[[group]]" {
                if let Some(builder) = current.take() {
                    groups.push(builder.finish(line_number)?);
                }
                current = Some(GroupBuilder::default());
                continue;
            }

            let (key, value) = line.split_once('=').ok_or_else(|| {
                AnimationError(format!("line {line_number}: expected key = value"))
            })?;
            let key = key.trim();
            let value = value.trim();
            if let Some(group) = current.as_mut() {
                group.assign(key, value, line_number)?;
            } else if key == "schema_version" {
                if schema_version.is_some() {
                    return Err(AnimationError(format!(
                        "line {line_number}: duplicate top-level key {key:?}"
                    )));
                }
                schema_version = Some(parse_u32(value, line_number, key)?);
            } else {
                return Err(AnimationError(format!(
                    "line {line_number}: unknown top-level key {key:?}"
                )));
            }
        }
        if let Some(builder) = current {
            groups.push(builder.finish(input.lines().count().max(1))?);
        }

        match schema_version {
            Some(ANIMATION_SCHEMA_VERSION) => Self::build(groups),
            Some(version) => Err(AnimationError(format!(
                "unsupported animation schema_version {version}; expected {ANIMATION_SCHEMA_VERSION}"
            ))),
            None => Err(AnimationError(
                "animation schema_version is required".into(),
            )),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, AnimationError> {
        let path = path.as_ref();
        let input = fs::read_to_string(path).map_err(|error| {
            AnimationError(format!("reading {} failed: {error}", path.display()))
        })?;
        Self::from_toml(&input)
    }

    fn build(groups: Vec<AnimationGroup>) -> Result<Self, AnimationError> {
        let mut metadata = [GpuPaletteMetadata::static_entry(); PALETTE_ENTRIES];
        let mut cycle_indices = [0u32; PALETTE_ENTRIES];
        let mut effects = [PaletteEffect::Static; PALETTE_ENTRIES];
        let mut occupied = [false; PALETTE_ENTRIES];
        let mut names = std::collections::HashSet::new();
        let mut cycle_offset = 0usize;

        for group in &groups {
            if !names.insert(group.name.clone()) {
                return Err(AnimationError(format!(
                    "animation group name {:?} is duplicated",
                    group.name
                )));
            }
            if !group.speed_cycles_per_second.is_finite()
                || group.speed_cycles_per_second.abs() > MAX_SPEED_CYCLES_PER_SECOND
            {
                return Err(AnimationError(format!(
                    "group {:?} speed must be finite and within +/-{MAX_SPEED_CYCLES_PER_SECOND} cycles/s",
                    group.name
                )));
            }
            if !group.amplitude.is_finite() || !(0.0..=1.0).contains(&group.amplitude) {
                return Err(AnimationError(format!(
                    "group {:?} amplitude must be finite and within 0..=1",
                    group.name
                )));
            }
            if !group.phase.is_finite() || !(0.0..1.0).contains(&group.phase) {
                return Err(AnimationError(format!(
                    "group {:?} phase must be finite and within [0, 1)",
                    group.name
                )));
            }
            if group.members.is_empty() {
                return Err(AnimationError(format!(
                    "group {:?} must contain at least one palette index",
                    group.name
                )));
            }
            if matches!(
                group.effect,
                EffectKind::PaletteCycle | EffectKind::BlendShift
            ) && group.members.len() < 2
            {
                return Err(AnimationError(format!(
                    "palette-cycle/blend-shift group {:?} must contain at least two ordered indices",
                    group.name
                )));
            }
            if group.effect == EffectKind::Static
                && (group.speed_cycles_per_second != 0.0 || group.amplitude != 0.0)
            {
                return Err(AnimationError(format!(
                    "static group {:?} cannot have speed or amplitude",
                    group.name
                )));
            }

            let offset = if matches!(
                group.effect,
                EffectKind::PaletteCycle | EffectKind::BlendShift
            ) {
                let end = cycle_offset
                    .checked_add(group.members.len())
                    .ok_or_else(|| AnimationError("cycle metadata size overflow".into()))?;
                if end > PALETTE_ENTRIES {
                    return Err(AnimationError(
                        "ordered palette-cycle metadata exceeds 256 entries".into(),
                    ));
                }
                for (ordinal, index) in group.members.iter().enumerate() {
                    cycle_indices[cycle_offset + ordinal] = u32::from(*index);
                }
                let offset = cycle_offset;
                cycle_offset = end;
                offset
            } else {
                0
            };

            for (ordinal, index) in group.members.iter().enumerate() {
                let slot = usize::from(*index);
                if occupied[slot] {
                    return Err(AnimationError(format!(
                        "animation groups overlap at palette index {index}"
                    )));
                }
                occupied[slot] = true;
                let effect = match group.effect {
                    EffectKind::Static => PaletteEffect::Static,
                    EffectKind::HueCycle => PaletteEffect::Hue {
                        speed: group.speed_cycles_per_second,
                        amplitude: group.amplitude,
                        phase: group.phase,
                    },
                    EffectKind::BrightnessPulse => PaletteEffect::Pulse {
                        speed: group.speed_cycles_per_second,
                        amplitude: group.amplitude,
                        phase: group.phase,
                    },
                    EffectKind::PaletteCycle => PaletteEffect::Cycle {
                        offset: offset as u32,
                        count: group.members.len() as u32,
                        ordinal: ordinal as u32,
                        speed: group.speed_cycles_per_second,
                        phase: group.phase,
                    },
                    EffectKind::BlendShift => PaletteEffect::BlendShift {
                        offset: offset as u32,
                        count: group.members.len() as u32,
                        ordinal: ordinal as u32,
                        speed: group.speed_cycles_per_second,
                        phase: group.phase,
                    },
                };
                effects[slot] = effect;
                metadata[slot] = metadata_for(effect);
            }
        }

        Ok(Self {
            groups,
            metadata,
            cycle_indices,
            effects,
        })
    }

    pub fn metadata_bytes(&self) -> Vec<u8> {
        let mut bytes =
            Vec::with_capacity(PALETTE_ENTRIES * std::mem::size_of::<GpuPaletteMetadata>());
        for entry in self.metadata {
            for value in [
                entry.effect,
                entry.group_offset,
                entry.member_count,
                entry.ordinal,
            ] {
                bytes.extend_from_slice(&value.to_ne_bytes());
            }
            for value in [entry.speed, entry.amplitude, entry.phase, entry.reserved] {
                bytes.extend_from_slice(&value.to_ne_bytes());
            }
        }
        bytes
    }

    pub fn cycle_index_bytes(&self) -> Vec<u8> {
        self.cycle_indices
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect()
    }

    /// CPU reference for shader vectors.  `time_seconds` is intentionally a
    /// bounded monotonic phase supplied by the renderer, not civil time.
    #[cfg(test)]
    pub fn apply_palette(&self, palette: &[u8; 256 * 4], time_seconds: f32) -> [u8; 256 * 4] {
        let mut output = *palette;
        for index in 0..PALETTE_ENTRIES {
            let offset = index * 4;
            if let PaletteEffect::BlendShift {
                offset: group_offset,
                count,
                ordinal,
                speed,
                phase,
            } = self.effects[index]
            {
                let [red, green, blue] = blend_shift_bytes(
                    palette,
                    &self.cycle_indices,
                    GpuPaletteMetadata {
                        group_offset,
                        member_count: count,
                        ordinal,
                        speed,
                        phase,
                        ..GpuPaletteMetadata::static_entry()
                    },
                    time_seconds,
                );
                output[offset] = red;
                output[offset + 1] = green;
                output[offset + 2] = blue;
                output[offset + 3] = 255;
                continue;
            }
            let color = [
                srgb_to_linear(f32::from(palette[offset]) / 255.0),
                srgb_to_linear(f32::from(palette[offset + 1]) / 255.0),
                srgb_to_linear(f32::from(palette[offset + 2]) / 255.0),
            ];
            let animated = match self.effects[index] {
                PaletteEffect::Static => color,
                PaletteEffect::Hue {
                    speed,
                    amplitude,
                    phase,
                } => hue_cycle(color, phase + time_seconds * speed, amplitude),
                PaletteEffect::Pulse {
                    speed,
                    amplitude,
                    phase,
                } => {
                    let factor = (1.0 + amplitude * (TAU * (phase + time_seconds * speed)).sin())
                        .clamp(0.0, 2.0);
                    [color[0] * factor, color[1] * factor, color[2] * factor]
                }
                PaletteEffect::Cycle {
                    offset,
                    count,
                    ordinal,
                    speed,
                    phase,
                } => {
                    let cycle = (phase + time_seconds * speed).rem_euclid(1.0);
                    let position = ordinal as f32 + cycle;
                    let lower = position.floor() as u32 % count;
                    let upper = (lower + 1) % count;
                    let t = position.fract();
                    let lower_index = self.cycle_indices[offset as usize + lower as usize] as usize;
                    let upper_index = self.cycle_indices[offset as usize + upper as usize] as usize;
                    let lower_offset = lower_index * 4;
                    let upper_offset = upper_index * 4;
                    let lower_color = [
                        srgb_to_linear(f32::from(palette[lower_offset]) / 255.0),
                        srgb_to_linear(f32::from(palette[lower_offset + 1]) / 255.0),
                        srgb_to_linear(f32::from(palette[lower_offset + 2]) / 255.0),
                    ];
                    let upper_color = [
                        srgb_to_linear(f32::from(palette[upper_offset]) / 255.0),
                        srgb_to_linear(f32::from(palette[upper_offset + 1]) / 255.0),
                        srgb_to_linear(f32::from(palette[upper_offset + 2]) / 255.0),
                    ];
                    [
                        lower_color[0] + (upper_color[0] - lower_color[0]) * t,
                        lower_color[1] + (upper_color[1] - lower_color[1]) * t,
                        lower_color[2] + (upper_color[2] - lower_color[2]) * t,
                    ]
                }
                PaletteEffect::BlendShift { .. } => unreachable!("handled above"),
            };
            output[offset] = linear_to_srgb_byte(animated[0]);
            output[offset + 1] = linear_to_srgb_byte(animated[1]);
            output[offset + 2] = linear_to_srgb_byte(animated[2]);
            output[offset + 3] = 255;
        }
        output
    }
}

#[cfg(test)]
fn blend_shift_bytes(
    palette: &[u8; 256 * 4],
    cycle_indices: &[u32; PALETTE_ENTRIES],
    metadata: GpuPaletteMetadata,
    time_seconds: f32,
) -> [u8; 3] {
    let cycle = (metadata.phase + time_seconds * metadata.speed).rem_euclid(1.0);
    let amount = cycle * metadata.member_count as f32;
    let shift = amount.floor() as u32 % metadata.member_count;
    // Classic BlendShift quantizes its fractional amount to 1/100 before
    // flooring each byte-channel fade, matching Palette.PRECISION = 100.
    let fraction = (amount.fract() * 100.0).floor() / 100.0;
    let current = (metadata.ordinal + metadata.member_count - shift) % metadata.member_count;
    let previous = (current + metadata.member_count - 1) % metadata.member_count;
    let current_index = cycle_indices[(metadata.group_offset + current) as usize] as usize;
    let previous_index = cycle_indices[(metadata.group_offset + previous) as usize] as usize;
    let current_offset = current_index * 4;
    let previous_offset = previous_index * 4;
    [
        blend_shift_channel(palette[current_offset], palette[previous_offset], fraction),
        blend_shift_channel(
            palette[current_offset + 1],
            palette[previous_offset + 1],
            fraction,
        ),
        blend_shift_channel(
            palette[current_offset + 2],
            palette[previous_offset + 2],
            fraction,
        ),
    ]
}

#[cfg(test)]
fn blend_shift_channel(source: u8, destination: u8, fraction: f32) -> u8 {
    (f32::from(source) + (f32::from(destination) - f32::from(source)) * fraction)
        .floor()
        .clamp(0.0, 255.0) as u8
}

fn metadata_for(effect: PaletteEffect) -> GpuPaletteMetadata {
    match effect {
        PaletteEffect::Static => GpuPaletteMetadata::static_entry(),
        PaletteEffect::Hue {
            speed,
            amplitude,
            phase,
        } => GpuPaletteMetadata {
            effect: 1,
            speed,
            amplitude,
            phase,
            ..GpuPaletteMetadata::static_entry()
        },
        PaletteEffect::Pulse {
            speed,
            amplitude,
            phase,
        } => GpuPaletteMetadata {
            effect: 2,
            speed,
            amplitude,
            phase,
            ..GpuPaletteMetadata::static_entry()
        },
        PaletteEffect::Cycle {
            offset,
            count,
            ordinal,
            speed,
            phase,
        } => GpuPaletteMetadata {
            effect: 3,
            group_offset: offset,
            member_count: count,
            ordinal,
            speed,
            phase,
            ..GpuPaletteMetadata::static_entry()
        },
        PaletteEffect::BlendShift {
            offset,
            count,
            ordinal,
            speed,
            phase,
        } => GpuPaletteMetadata {
            effect: 4,
            group_offset: offset,
            member_count: count,
            ordinal,
            speed,
            phase,
            ..GpuPaletteMetadata::static_entry()
        },
    }
}

#[derive(Default)]
struct GroupBuilder {
    name: Option<String>,
    effect: Option<EffectKind>,
    range: Option<(u8, u8)>,
    indices: Option<Vec<u8>>,
    speed: Option<f32>,
    amplitude: Option<f32>,
    phase: Option<f32>,
}

impl GroupBuilder {
    fn assign(&mut self, key: &str, value: &str, line: usize) -> Result<(), AnimationError> {
        let duplicate = match key {
            "name" => self.name.is_some(),
            "effect" => self.effect.is_some(),
            "range" => self.range.is_some(),
            "indices" => self.indices.is_some(),
            "speed" => self.speed.is_some(),
            "amplitude" => self.amplitude.is_some(),
            "phase" => self.phase.is_some(),
            _ => false,
        };
        if duplicate {
            return Err(AnimationError(format!(
                "line {line}: duplicate animation group key {key:?}"
            )));
        }
        match key {
            "name" => self.name = Some(parse_string(value, line, key)?),
            "effect" => self.effect = Some(parse_effect(value, line)?),
            "range" => self.range = Some(parse_range(value, line)?),
            "indices" => self.indices = Some(parse_indices(value, line)?),
            "speed" => self.speed = Some(parse_float(value, line, key)?),
            "amplitude" => self.amplitude = Some(parse_float(value, line, key)?),
            "phase" => self.phase = Some(parse_float(value, line, key)?),
            _ => {
                return Err(AnimationError(format!(
                    "line {line}: unknown animation group key {key:?}"
                )));
            }
        }
        Ok(())
    }

    fn finish(self, line: usize) -> Result<AnimationGroup, AnimationError> {
        let name = self
            .name
            .filter(|name| !name.is_empty())
            .ok_or_else(|| AnimationError(format!("line {line}: group name is required")))?;
        let effect = self
            .effect
            .ok_or_else(|| AnimationError(format!("group {name:?}: effect is required")))?;
        if self.range.is_some() && self.indices.is_some() {
            return Err(AnimationError(format!(
                "group {name:?}: range and indices are mutually exclusive"
            )));
        }
        let members = match (self.range, self.indices) {
            (Some((first, last)), None) => (first..=last).collect(),
            (None, Some(indices)) => indices,
            (None, None) => {
                return Err(AnimationError(format!(
                    "group {name:?}: one range or indices list is required"
                )));
            }
            (Some(_), Some(_)) => unreachable!(),
        };
        let mut seen = [false; PALETTE_ENTRIES];
        for index in &members {
            let slot = usize::from(*index);
            if seen[slot] {
                return Err(AnimationError(format!(
                    "group {name:?}: indices list contains duplicate {index}"
                )));
            }
            seen[slot] = true;
        }
        Ok(AnimationGroup {
            name,
            effect,
            members,
            speed_cycles_per_second: self.speed.unwrap_or(0.0),
            amplitude: self.amplitude.unwrap_or(match effect {
                EffectKind::HueCycle
                | EffectKind::BrightnessPulse
                | EffectKind::PaletteCycle
                | EffectKind::BlendShift => 1.0,
                EffectKind::Static => 0.0,
            }),
            phase: self.phase.unwrap_or(0.0),
        })
    }
}

fn parse_string(value: &str, line: usize, key: &str) -> Result<String, AnimationError> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .filter(|value| !value.contains('"') && !value.contains('\n'))
        .map(str::to_owned)
        .ok_or_else(|| AnimationError(format!("line {line}: {key} must be a quoted string")))
}

fn parse_u32(value: &str, line: usize, key: &str) -> Result<u32, AnimationError> {
    value
        .parse::<u32>()
        .map_err(|_| AnimationError(format!("line {line}: {key} must be an unsigned integer")))
}

fn parse_float(value: &str, line: usize, key: &str) -> Result<f32, AnimationError> {
    let value = value
        .parse::<f32>()
        .map_err(|_| AnimationError(format!("line {line}: {key} must be a finite number")))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AnimationError(format!("line {line}: {key} must be finite")))
    }
}

fn parse_effect(value: &str, line: usize) -> Result<EffectKind, AnimationError> {
    match parse_string(value, line, "effect")?.as_str() {
        "static" => Ok(EffectKind::Static),
        "hue_cycle" => Ok(EffectKind::HueCycle),
        "brightness_pulse" => Ok(EffectKind::BrightnessPulse),
        "palette_cycle" => Ok(EffectKind::PaletteCycle),
        "blend_shift" => Ok(EffectKind::BlendShift),
        other => Err(AnimationError(format!(
            "line {line}: unsupported effect {other:?}"
        ))),
    }
}

fn parse_range(value: &str, line: usize) -> Result<(u8, u8), AnimationError> {
    let values = parse_array(value, line, "range")?;
    if values.len() != 2 {
        return Err(AnimationError(format!(
            "line {line}: range must contain exactly [first, last]"
        )));
    }
    let first = parse_index(values[0], line)?;
    let last = parse_index(values[1], line)?;
    if first > last {
        return Err(AnimationError(format!(
            "line {line}: range first index must not exceed last index"
        )));
    }
    Ok((first, last))
}

fn parse_indices(value: &str, line: usize) -> Result<Vec<u8>, AnimationError> {
    let values = parse_array(value, line, "indices")?;
    if values.is_empty() {
        return Err(AnimationError(format!(
            "line {line}: indices cannot be empty"
        )));
    }
    values
        .into_iter()
        .map(|value| parse_index(value, line))
        .collect()
}

fn parse_array<'a>(value: &'a str, line: usize, key: &str) -> Result<Vec<&'a str>, AnimationError> {
    let inner = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| AnimationError(format!("line {line}: {key} must be an array")))?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(inner.split(',').map(str::trim).collect())
}

fn parse_index(value: &str, line: usize) -> Result<u8, AnimationError> {
    let value = value.parse::<u16>().map_err(|_| {
        AnimationError(format!(
            "line {line}: palette indices must be integers 0..=255"
        ))
    })?;
    u8::try_from(value).map_err(|_| {
        AnimationError(format!(
            "line {line}: palette index {value} is outside 0..=255"
        ))
    })
}

#[cfg(test)]
const TAU: f32 = std::f32::consts::TAU;

#[cfg(test)]
fn hue_cycle(color: [f32; 3], phase: f32, amplitude: f32) -> [f32; 3] {
    let [hue, saturation, value] = rgb_to_hsv(color);
    hsv_to_rgb([
        ((hue + amplitude * phase).rem_euclid(1.0)),
        saturation,
        value,
    ])
}

#[cfg(test)]
fn rgb_to_hsv([red, green, blue]: [f32; 3]) -> [f32; 3] {
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == red {
        ((green - blue) / delta / 6.0).rem_euclid(1.0)
    } else if max == green {
        ((blue - red) / delta + 2.0) / 6.0
    } else {
        ((red - green) / delta + 4.0) / 6.0
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    [hue, saturation, max]
}

#[cfg(test)]
fn hsv_to_rgb([hue, saturation, value]: [f32; 3]) -> [f32; 3] {
    let scaled = hue.rem_euclid(1.0) * 6.0;
    let sector = scaled.floor() as u32;
    let fraction = scaled.fract();
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * fraction);
    let t = value * (1.0 - saturation * (1.0 - fraction));
    match sector % 6 {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        _ => [value, p, q],
    }
}

#[cfg(test)]
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
fn linear_to_srgb_byte(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> &'static str {
        r#"
            schema_version = 1

            [[group]]
            name = "water"
            effect = "hue_cycle"
            range = [1, 2]
            speed = 0.25
            amplitude = 0.5
            phase = 0.125

            [[group]]
            name = "lamps"
            effect = "brightness_pulse"
            indices = [3, 7]
            speed = 0.5
            amplitude = 0.2

            [[group]]
            name = "flags"
            effect = "palette_cycle"
            indices = [9, 4, 11]
            speed = -0.25
            amplitude = 1.0
        "#
    }

    #[test]
    fn parses_groups_and_builds_immutable_metadata() {
        let animation = Animation::from_toml(sample()).unwrap();
        assert_eq!(animation.groups.len(), 3);
        assert_eq!(animation.metadata[1].effect, 1);
        assert_eq!(animation.metadata[3].effect, 2);
        assert_eq!(animation.metadata[9].effect, 3);
        assert_eq!(animation.metadata[11].ordinal, 2);
        assert_eq!(animation.cycle_indices[..3], [9, 4, 11]);
    }

    #[test]
    fn absent_animation_is_static_and_reference_palette_is_unchanged() {
        let animation = Animation::static_default();
        let mut palette = [127u8; 256 * 4];
        for entry in palette.chunks_exact_mut(4) {
            entry[3] = 255;
        }
        assert_eq!(animation.apply_palette(&palette, 999.0), palette);
    }

    #[test]
    fn malformed_and_overlapping_metadata_is_rejected() {
        let overlap = r#"
            schema_version = 1
            [[group]]
            name = "one"
            effect = "hue_cycle"
            range = [1, 2]
            [[group]]
            name = "two"
            effect = "brightness_pulse"
            indices = [2]
        "#;
        assert!(
            Animation::from_toml(overlap)
                .unwrap_err()
                .to_string()
                .contains("overlap")
        );

        let bad = sample().replace("amplitude = 0.2", "amplitude = 2.0");
        assert!(Animation::from_toml(&bad).is_err());
        let bad = sample().replace("schema_version = 1", "schema_version = 2");
        assert!(Animation::from_toml(&bad).is_err());
        let duplicate = sample().replace("name = \"water\"", "name = \"water\"\nname = \"other\"");
        assert!(
            Animation::from_toml(&duplicate)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
    }

    #[test]
    fn hue_and_brightness_reference_vectors_change_in_linear_palette_space() {
        let animation = Animation::from_toml(
            r#"
                schema_version = 1
                [[group]]
                name = "hue"
                effect = "hue_cycle"
                indices = [1]
                speed = 0.0
                amplitude = 1.0
                phase = 0.33333334
                [[group]]
                name = "pulse"
                effect = "brightness_pulse"
                indices = [2]
                speed = 0.0
                amplitude = 0.5
                phase = 0.25
            "#,
        )
        .unwrap();
        let mut palette = [0u8; 256 * 4];
        palette[4..8].copy_from_slice(&[255, 0, 0, 255]);
        palette[8..12].copy_from_slice(&[128, 128, 128, 255]);
        let output = animation.apply_palette(&palette, 0.0);
        assert!(
            output[5] > 180 && output[4] < 100,
            "hue vector: {:?}",
            &output[4..8]
        );
        assert!(output[8] > 140, "pulse vector: {:?}", &output[8..12]);
        assert_eq!(output[7], 255);
    }

    #[test]
    fn palette_cycle_uses_ordered_list_and_wraps() {
        let animation = Animation::from_toml(
            r#"
                schema_version = 1
                [[group]]
                name = "cycle"
                effect = "palette_cycle"
                indices = [1, 2]
                speed = 0.0
                phase = 0.25
            "#,
        )
        .unwrap();
        let mut palette = [0u8; 256 * 4];
        palette[4..8].copy_from_slice(&[255, 0, 0, 255]);
        palette[8..12].copy_from_slice(&[0, 0, 255, 255]);
        let output = animation.apply_palette(&palette, 0.0);
        assert!(output[4] > 150 && output[6] > 100);
        assert_eq!(output[3], 255);
    }

    #[test]
    fn blend_shift_matches_byte_space_rotation_and_fractional_fade() {
        let animation = Animation::from_toml(
            r#"
                schema_version = 1
                [[group]]
                name = "water"
                effect = "blend_shift"
                indices = [1, 2, 3]
                speed = 1.0
                phase = 0.5
            "#,
        )
        .unwrap();
        assert_eq!(animation.metadata[1].effect, 4);
        let mut palette = [0u8; 256 * 4];
        palette[4..8].copy_from_slice(&[10, 20, 30, 255]);
        palette[8..12].copy_from_slice(&[100, 110, 120, 255]);
        palette[12..16].copy_from_slice(&[200, 210, 220, 255]);
        let output = animation.apply_palette(&palette, 0.0);
        // At amount 1.5, entry 1 is halfway from original entry 3 to 2.
        assert_eq!(&output[4..8], &[150, 160, 170, 255]);
        assert_eq!(&output[8..12], &[105, 115, 125, 255]);
        assert_eq!(&output[12..16], &[55, 65, 75, 255]);
    }

    #[test]
    fn wrapped_clock_has_repeatable_long_runtime_reference() {
        let animation = Animation::from_toml(
            r#"
                schema_version = 1
                [[group]]
                name = "hue"
                effect = "hue_cycle"
                range = [0, 0]
                speed = 0.25
                amplitude = 1.0
                phase = 0.125
            "#,
        )
        .unwrap();
        let mut palette = [0u8; 256 * 4];
        palette[..4].copy_from_slice(&[255, 32, 16, 255]);
        assert_eq!(
            animation.apply_palette(&palette, 0.5),
            animation.apply_palette(&palette, 1024.5)
        );
    }

    #[test]
    fn signed_speed_and_phase_are_representable() {
        let animation = Animation::from_toml(
            r#"
                schema_version = 1
                [[group]]
                name = "reverse"
                effect = "hue_cycle"
                range = [0, 0]
                speed = -16.0
                amplitude = 1.0
                phase = 0.999
            "#,
        )
        .unwrap();
        assert_eq!(animation.metadata[0].speed, -16.0);
    }
}
