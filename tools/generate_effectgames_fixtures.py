#!/usr/bin/env python3
"""Materialize local, untracked Effect Games reference packages.

This is a provenance tool, not a build step.  It consumes browser-captured
``fetch.json`` responses below ``fixtures/untracked/effectgames`` and writes
only local inspection fixtures.  The source files and generated PNGs must not
be committed or published without an independent license review.

No third-party Python packages are required.  The browser capture may be the
JSON wrapper written by agent-browser (``text``/``result``) or a raw script.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import struct
import sys
import zlib
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

CATALOG = "https://www.effectgames.com/demos/worlds/scenes.js"
ROOT = Path(__file__).resolve().parents[1] / "fixtures/untracked/effectgames"
# Effect Games cycle rates are palette-shift rates, not full-cycle rates.
# The site's speed-4 control multiplies its rate before dividing by CYCLE_SPEED
# (280), then the range length determines complete cycles per second.
SOURCE_CYCLE_SPEED_ADJUST = 4.0
SOURCE_CYCLE_SPEED_BASE = 280.0


@dataclass(frozen=True)
class Variant:
    package_id: str
    family_id: str
    variant_id: str
    catalog_name: str
    month: str
    script: str
    weather: str
    source_file: str
    source_url: str
    time_window: str | None = None
    remap: tuple[tuple[int, tuple[int, int, int]], ...] = ()


# The inventory mirrors fixtures/effectgames-reference-manifest.toml.  Keeping
# the source URL here makes the generator independently auditable and prevents
# a package from silently being sourced from an unrelated response file.
V = Variant
VARIANTS: tuple[Variant, ...] = (
    V("v26-winter-forest", "v26-winter-forest", "clear", "V26", "01January", "janclrscpt", "clear", "v26-winter-forest/fetch.json", "https://www.effectgames.com/demos/worlds/cache/V26-01January-janclrscpt.js"),
    V("v26-winter-forest--snow", "v26-winter-forest", "snow", "V26", "01January", "jansnowscpt", "snow", "v26-winter-forest-snow-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V26SNOW-01January-jansnowscpt.js"),
    V("v19-mountain-stream", "v19-mountain-stream", "clear", "V19", "02February", "febclrscpt", "clear", "v19-mountain-stream/fetch.json", "https://www.effectgames.com/demos/worlds/cache/V19-02February-febclrscpt.js"),
    V("v19-mountain-stream--cloudy", "v19-mountain-stream", "cloudy", "V19", "02February", "febcldyscpt", "cloudy", "v19-mountain-stream-cloudy-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V19-02February-febcldyscpt.js"),
    V("vw3b-monolith-plains--clear", "vw3b-monolith-plains", "clear", "VW3BASIC", "03March", "MARCLRSCPT", "clear", "vw3b-monolith-plains-clear-fetch.json", "https://www.effectgames.com/demos/worlds/cache/VW3BASIC-03March-MARCLRSCPT.js"),
    V("v30-deep-forest--clear", "v30-deep-forest", "clear", "V30", "04April", "aprclrscpt", "clear", "v30-deep-forest-clear-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V30-04April-aprclrscpt.js"),
    V("v30-deep-forest--rain", "v30-deep-forest", "rain", "V30", "04April", "aprrainscpt", "rain", "v30-deep-forest-rain-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V30RAIN-04April-aprrainscpt.js"),
    V("v08-jungle-waterfall--clear", "v08-jungle-waterfall", "clear", "V08", "05May", "MAYCLRSCPT", "clear", "v08-jungle-waterfall-clear-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V08-05May-MAYCLRSCPT.js"),
    V("v08-jungle-waterfall--cloudy", "v08-jungle-waterfall", "cloudy", "V08", "05May", "MAYCLDYSCPT", "cloudy", "v08-jungle-waterfall-cloudy-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V08-05May-MAYCLDYSCPT.js"),
    V("v08-jungle-waterfall--rain", "v08-jungle-waterfall", "rain", "V08", "05May", "MAYRAINSCPT", "rain", "v08-jungle-waterfall-rain-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V08RAIN-05May-MAYRAINSCPT.js"),
    V("v20joe-crystal-caves--clear", "v20joe-crystal-caves", "clear", "V20JOE", "06June", "jundayscpt", "clear", "v20joe-crystal-caves-clear-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V20JOE-06June-jundayscpt.js"),
    V("v25-desert--clear", "v25-desert", "clear", "V25", "07July", "julyclearscpt", "clear", "v25-desert-clear-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V25-07July-julyclearscpt.js"),
    V("v25-desert--cloudy", "v25-desert", "cloudy", "V25", "07July", "julycloudyscpt", "cloudy", "v25-desert-cloudy-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V25-07July-julycloudyscpt.js"),
    V("coral-aquarius--clear", "coral-aquarius", "clear", "CORAL", "08August", "augclrscpt", "clear", "coral-aquarius-clear-fetch.json", "https://www.effectgames.com/demos/worlds/cache/CORAL-08August-augclrscpt.js", remap=((0, (0, 0, 0)),)),
    V("v29-seascape", "v29-seascape", "clear", "V29", "09September", "SEPTCLRCUMSCPT", "clear", "v29-seascape/fetch.json", "https://www.effectgames.com/demos/worlds/cache/V29-09September-SEPTCLRCUMSCPT.js", remap=((252, (11, 11, 11)),)),
    V("v29-seascape--cloudy", "v29-seascape", "cloudy", "V29", "09September", "SEPTCLDYSCPT", "cloudy", "v29-seascape-cloudy-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V29-09September-SEPTCLDYSCPT.js", remap=((252, (11, 11, 11)),)),
    V("v05-haunted-ruins--early-clear", "v05-haunted-ruins", "early-clear", "V05AM/V05RAIN", "10October", "octbegclrscpt", "clear", "v05-haunted-ruins-early-clear-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V05AM-10October-octbegclrscpt.js", "early", ((254, (0, 0, 0)), (0, (11, 11, 11)))),
    V("v05-haunted-ruins--late-clear", "v05-haunted-ruins", "late-clear", "V05AM/V05RAIN", "10October", "octendclrscpt", "clear", "v05-haunted-ruins-late-clear-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V05AM-10October-octendclrscpt.js", "late", ((254, (0, 0, 0)), (0, (11, 11, 11)))),
    V("v05-haunted-ruins--rain", "v05-haunted-ruins", "rain", "V05AM/V05RAIN", "10October", "octrainscpt", "rain", "v05-haunted-ruins-rain-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V05RAIN-10October-octrainscpt.js", "late", ((254, (0, 0, 0)), (0, (11, 11, 11)))),
    V("v16-mirror-pond--clear", "v16-mirror-pond", "clear", "V16/V16RAIN", "11November", "novclrscpt", "clear", "v16-mirror-pond-clear-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V16-11November-novclrscpt.js"),
    V("v16-mirror-pond--rain", "v16-mirror-pond", "rain", "V16/V16RAIN", "11November", "novrainscpt", "rain", "v16-mirror-pond-rain-fetch.json", "https://www.effectgames.com/demos/worlds/cache/V16RAIN-11November-novrainscpt.js"),
)


def extract_array(source: str, marker: str, start: int = 0) -> list:
    position = source.find(marker, start)
    if position < 0:
        raise ValueError(f"missing {marker}")
    begin = source.find("[", position + len(marker))
    if begin < 0:
        raise ValueError(f"missing array after {marker}")
    depth = 0
    for index in range(begin, len(source)):
        if source[index] == "[":
            depth += 1
        elif source[index] == "]":
            depth -= 1
            if depth == 0:
                return json.loads(source[begin : index + 1])
    raise ValueError(f"unterminated array after {marker}")


def extract_object(source: str, marker: str) -> str:
    position = source.find(marker)
    if position < 0:
        raise ValueError(f"missing {marker}")
    begin = source.find("{", position + len(marker) - 1)
    depth = 0
    quote = None
    escaped = False
    for index in range(begin, len(source)):
        char = source[index]
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            continue
        if char in "'\"":
            quote = char
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[begin + 1 : index]
    raise ValueError(f"unterminated object after {marker}")


def source_text(path: Path) -> str:
    value = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(value, dict):
        value = value.get("result", value.get("text"))
    if not isinstance(value, str):
        raise ValueError(f"{path}: expected a browser text result")
    return value


def png_chunk(kind: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)


def write_png(path: Path, width: int, height: int, color_type: int, rows: Iterable[bytes]) -> None:
    raw = b"".join(b"\x00" + row for row in rows)
    header = struct.pack(">IIBBBBB", width, height, 8, color_type, 0, 0, 0)
    payload = b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", header)
    payload += png_chunk(b"IDAT", zlib.compress(raw, 9)) + png_chunk(b"IEND", b"")
    path.write_bytes(payload)


def palette_bytes(colors: list[list[int]], remap: tuple[tuple[int, tuple[int, int, int]], ...]) -> bytes:
    if len(colors) != 256 or any(len(color) != 3 for color in colors):
        raise ValueError(f"expected 256 RGB palette entries, got {len(colors)}")
    replacements = dict(remap)
    return b"".join(bytes((*replacements.get(index, tuple(color)), 255)) for index, color in enumerate(colors))


def parse_source(source: str, variant: Variant) -> tuple[int, int, bytes, bytes, list[tuple[int, bytes]], list[tuple[int, int, int, int]]]:
    base = source.find("base:{")
    if base < 0:
        raise ValueError("missing base scene object")
    dimensions = re.search(r"width:(\d+),height:(\d+)", source[base:])
    if not dimensions:
        raise ValueError("missing base dimensions")
    width, height = map(int, dimensions.groups())
    colors = extract_array(source, "colors:", base)
    pixels = extract_array(source, "pixels:", base)
    if len(pixels) != width * height:
        raise ValueError(f"pixel count {len(pixels)} != {width}x{height}")
    base_palette = palette_bytes(colors, variant.remap)
    cycles: list[tuple[int, int, int, int]] = []
    cycles_position = source.find("cycles:[", base)
    pixels_position = source.find("],pixels:", cycles_position) if cycles_position >= 0 else -1
    if cycles_position >= 0 and pixels_position >= 0:
        cycles_text = source[cycles_position:pixels_position]
        cycles = [
            tuple(map(int, match))
            for match in re.findall(
                r"reverse:(-?\d+),rate:(-?\d+),low:(\d+),high:(\d+)",
                cycles_text,
            )
        ]

    timelines: list[tuple[int, bytes]] = []
    timeline_position = source.find("timeline:{")
    palettes_position = source.find("palettes:{")
    if timeline_position >= 0 and palettes_position >= 0:
        timeline = extract_object(source, "timeline:{")
        for seconds, key in re.findall(r"[\"'](\d+)[\"']\s*:\s*[\"']([^\"']+)[\"']", timeline):
            key_pattern = rf"[\"']{re.escape(key)}[\"']\s*:\s*\{{"
            match = re.search(key_pattern, source[palettes_position:])
            if not match:
                raise ValueError(f"timeline palette {key!r} is missing")
            palette_start = palettes_position + match.start()
            colors = extract_array(source, "colors:", palette_start)
            timelines.append((int(seconds), palette_bytes(colors, variant.remap)))
    return width, height, bytes(pixels), base_palette, timelines, cycles


def package_dir(variant: Variant) -> Path:
    return ROOT / variant.package_id


def source_path(variant: Variant) -> Path:
    path = ROOT / variant.source_file
    if path.is_file():
        return path
    # Existing packages store the browser capture inside their package dir;
    # new captures are initially written at the fixture root.
    alternate = ROOT / Path(variant.source_file).name
    if alternate.is_file():
        return alternate
    raise FileNotFoundError(path)


def write_package(variant: Variant, check_only: bool = False) -> None:
    source_path_value = source_path(variant)
    source = source_text(source_path_value)
    width, height, pixels, palette, timelines, cycles = parse_source(source, variant)
    directory = package_dir(variant)
    if check_only:
        if not (directory / "scene.png").is_file() or not (directory / "palette.png").is_file():
            raise ValueError(f"{variant.package_id}: generated PNGs are missing")
        return
    directory.mkdir(parents=True, exist_ok=True)
    if source_path_value != directory / "fetch.json":
        shutil.copyfile(source_path_value, directory / "fetch.json")
    write_png(directory / "scene.png", width, height, 0, (pixels[row * width : (row + 1) * width] for row in range(height)))
    write_png(directory / "palette.png", 256, 1, 6, (palette,))
    active_cycles = [
        (reverse, rate, low, high)
        for reverse, rate, low, high in cycles
        if rate != 0 and low < high
    ]
    animation_path = directory / "animation.toml"
    if active_cycles:
        animation_lines = [
            "schema_version = 1",
            f"# source-speed-adjust = {SOURCE_CYCLE_SPEED_ADJUST:g}",
            f"# source-cycle-speed-base = {SOURCE_CYCLE_SPEED_BASE:g}",
            "",
        ]
        for number, (reverse, rate, low, high) in enumerate(active_cycles, 1):
            member_count = high - low + 1
            speed = (
                rate * SOURCE_CYCLE_SPEED_ADJUST
                / SOURCE_CYCLE_SPEED_BASE
                / member_count
            )
            if reverse == 2:
                speed = -speed
            animation_lines.extend([
                "[[group]]",
                f'name = "catalog-cycle-{number:02d}"',
                'effect = "blend_shift"',
                f"range = [{low}, {high}]",
                f"speed = {speed:.6f}",
                "amplitude = 1.0",
                "phase = 0.0",
                "",
            ])
        animation_path.write_text("\n".join(animation_lines), encoding="utf-8")
    source_note = [
        f"Fetched source: {variant.source_url}",
        f"Catalog: {CATALOG}",
        f"Catalog name: {variant.catalog_name}",
        f"Family ID: {variant.family_id}",
        f"Variant ID: {variant.variant_id}",
        f"Month: {variant.month}",
        f"Weather: {variant.weather}",
        f"Dimensions: {width}x{height}",
        f"Source-specific remap: {dict(variant.remap) if variant.remap else 'none'}",
        f"Active source palette cycles: {len(active_cycles)}",
        f"Source cycle mapping: speed-adjust {SOURCE_CYCLE_SPEED_ADJUST:g}, base {SOURCE_CYCLE_SPEED_BASE:g}, range-length normalized, authored-byte BlendShift.",
        "This matches the inspected site's default Blend mode and speed control for source reverse modes 0/2; it is not a claim for unrepresented source modes.",
        "Local untracked inspection fixture; do not publish or commit artwork without license review.",
    ]
    (directory / "SOURCE.txt").write_text("\n".join(source_note) + "\n", encoding="utf-8")
    manifest = [
        "schema_version = 1",
        f'id = "{variant.package_id}"',
        'image = "scene.png"',
        'palette = "palette.png"',
        f'weather = ["{variant.weather}"]',
        'times = []',
        'seasons = []',
    ]
    if animation_path.is_file():
        manifest.insert(3, 'animation = "animation.toml"')
    if timelines:
        atlas = b"".join(row for _, row in timelines)
        write_png(directory / "time_of_day.png", 256, len(timelines), 6, (atlas[row * 1024 : (row + 1) * 1024] for row in range(len(timelines))))
        manifest.insert(3, 'time_of_day = "time_of_day.toml"')
        timeline_lines = ["schema_version = 1", 'atlas = "time_of_day.png"', ""]
        for row, (seconds, _) in enumerate(timelines):
            timeline_lines.extend(["[[keyframe]]", f"civil_seconds = {seconds}", f"row = {row}", ""])
        (directory / "time_of_day.toml").write_text("\n".join(timeline_lines), encoding="utf-8")
    (directory / "scene.toml").write_text("\n".join(manifest) + "\n", encoding="utf-8")


def write_registry() -> None:
    scene_ids = ", ".join(f'"{variant.package_id}"' for variant in VARIANTS)
    lines = [
        "schema_version = 1",
        'fallback = "v26-winter-forest"',
        f"scenes = [{scene_ids}]",
    ]
    (ROOT / "registry.toml").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    global ROOT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="local untracked fixture root")
    parser.add_argument("--check-only", action="store_true", help="parse every source and require generated packages")
    args = parser.parse_args()
    ROOT = args.root.resolve()
    missing = []
    for variant in VARIANTS:
        try:
            write_package(variant, args.check_only)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            missing.append(f"{variant.package_id}: {error}")
    if not args.check_only:
        write_registry()
    if missing:
        print("Effect Games fixture generation failed:", file=sys.stderr)
        print("\n".join(missing), file=sys.stderr)
        return 1
    print(f"validated {len(VARIANTS)} catalog-listed variants; V12 winter manor remains source-unavailable")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
