#!/usr/bin/env python3
"""Import one explicitly captured CanvasCycle LBM object as a private package.

This tool reads local bytes only. It never fetches or evaluates JavaScript.
Source cycle fields are parsed as data and mapped to authored-byte
`blend_shift` animation groups using the established Effect Games `Palette`
cycle semantics (FM-0013): the source advances `rate * speed_adjust /
CYCLE_SPEED` palette positions per second, the range length sets complete
rotations, the site's Blend mode is exactly Worldwall's authored-byte
`blend_shift`, and source reverse mode `2` is the reversed direction (signed
speed). Unrepresented source modes are rejected, never guessed. The capture
file remains the raw record. Generated packages are local inspection data,
not licensed assets.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import shutil
import stat
import struct
import sys
import tempfile
import zlib
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlsplit

MAX_CAPTURE_BYTES = 16 * 1024 * 1024
MAX_PIXELS = 16 * 1024 * 1024
MAX_CYCLES = 256
PALETTE_ENTRIES = 256

# Effect Games `Palette` cycle semantics (FM-0013): the source advances
# `rate * source_speed_adjust / SOURCE_CYCLE_SPEED_BASE` palette positions per
# second, the member count sets complete rotations, the site's Blend mode is
# Worldwall's authored-byte blend_shift, and source reverse mode 2 is the
# reversed direction (signed speed).
SOURCE_CYCLE_SPEED_BASE = 280.0
DEFAULT_SOURCE_SPEED_ADJUST = 1.0


class CaptureError(ValueError):
    """The local capture or requested package is malformed or unsafe."""


@dataclass(frozen=True)
class SourceCycle:
    reverse: int
    rate: int
    low: int
    high: int


@dataclass(frozen=True)
class Capture:
    filename: str
    width: int
    height: int
    colors: tuple[tuple[int, int, int], ...]
    pixels: bytes
    cycles: tuple[SourceCycle, ...]


class _LiteralReader:
    """Parser for the capture's deliberately small JavaScript-data subset."""

    def __init__(self, source: str):
        self.source = source
        self.position = 0

    def parse_capture(self) -> Capture:
        self._space()
        self._expect("{")
        fields: dict[str, object] = {}
        allowed = {"filename", "width", "height", "colors", "pixels", "cycles"}
        while True:
            self._space()
            if self._take("}"):
                break
            key = self._identifier()
            if key not in allowed:
                self._fail(f"unsupported source field {key!r}")
            if key in fields:
                self._fail(f"duplicate source field {key!r}")
            self._space()
            self._expect(":")
            self._space()
            if key == "filename":
                fields[key] = self._string()
            elif key in ("width", "height"):
                fields[key] = self._integer(minimum=1, maximum=0xFFFF_FFFF)
            elif key == "colors":
                fields[key] = self._colors()
            elif key == "pixels":
                fields[key] = self._pixels()
            else:
                fields[key] = self._cycles()
            self._space()
            if self._take(","):
                self._space()
                if self._take("}"):
                    break
                continue
            self._expect("}")
            break

        self._space()
        self._take(";")
        self._space()
        if self.position != len(self.source):
            self._fail("unexpected trailing source text")

        required = {"filename", "width", "height", "colors", "pixels"}
        missing = required - fields.keys()
        if missing:
            raise CaptureError(f"missing source fields: {', '.join(sorted(missing))}")

        filename = fields["filename"]
        width = fields["width"]
        height = fields["height"]
        colors = fields["colors"]
        pixels = fields["pixels"]
        cycles = fields.get("cycles", ())
        assert isinstance(filename, str)
        assert isinstance(width, int) and isinstance(height, int)
        assert isinstance(colors, tuple) and isinstance(pixels, bytes)
        assert isinstance(cycles, tuple)

        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,254}", filename):
            raise CaptureError("source filename is not a safe basename")
        pixel_count = width * height
        if pixel_count > MAX_PIXELS:
            raise CaptureError(f"source dimensions exceed {MAX_PIXELS} pixels")
        if len(pixels) != pixel_count:
            raise CaptureError(
                f"pixel count {len(pixels)} does not match dimensions {width}x{height}"
            )
        if len(colors) != PALETTE_ENTRIES:
            raise CaptureError(
                f"source palette must contain {PALETTE_ENTRIES} RGB entries"
            )
        return Capture(filename, width, height, colors, pixels, cycles)

    def _colors(self) -> tuple[tuple[int, int, int], ...]:
        self._expect("[")
        result: list[tuple[int, int, int]] = []
        self._space()
        if self._take("]"):
            return ()
        while True:
            if len(result) >= PALETTE_ENTRIES:
                self._fail(f"palette exceeds {PALETTE_ENTRIES} entries")
            self._expect("[")
            red = self._byte()
            self._expect(",")
            green = self._byte()
            self._expect(",")
            blue = self._byte()
            self._space()
            self._expect("]")
            result.append((red, green, blue))
            self._space()
            if self._take(","):
                self._space()
                if self._take("]"):
                    break
                continue
            self._expect("]")
            break
        return tuple(result)

    def _pixels(self) -> bytes:
        self._expect("[")
        pixels = bytearray()
        self._space()
        if self._take("]"):
            return b""
        while True:
            if len(pixels) >= MAX_PIXELS:
                self._fail(f"pixel array exceeds {MAX_PIXELS} values")
            pixels.append(self._byte())
            self._space()
            if self._take(","):
                self._space()
                if self._take("]"):
                    break
                continue
            self._expect("]")
            break
        return bytes(pixels)

    def _cycles(self) -> tuple[SourceCycle, ...]:
        self._expect("[")
        cycles: list[SourceCycle] = []
        self._space()
        if self._take("]"):
            return ()
        while True:
            if len(cycles) >= MAX_CYCLES:
                self._fail(f"cycle list exceeds {MAX_CYCLES} entries")
            self._expect("{")
            fields: dict[str, int] = {}
            while True:
                self._space()
                if self._take("}"):
                    break
                key = self._identifier()
                if key not in {"reverse", "rate", "low", "high"}:
                    self._fail(f"unsupported cycle field {key!r}")
                if key in fields:
                    self._fail(f"duplicate cycle field {key!r}")
                self._space()
                self._expect(":")
                self._space()
                if key in {"reverse", "rate"}:
                    fields[key] = self._integer(minimum=-(2**31), maximum=2**31 - 1)
                else:
                    fields[key] = self._integer(minimum=0, maximum=255)
                self._space()
                if self._take(","):
                    self._space()
                    if self._take("}"):
                        break
                    continue
                self._expect("}")
                break
            if set(fields) != {"reverse", "rate", "low", "high"}:
                self._fail("cycle entry requires reverse, rate, low, and high")
            if fields["low"] > fields["high"]:
                self._fail("cycle range must not descend")
            cycles.append(SourceCycle(**fields))
            self._space()
            if self._take(","):
                self._space()
                if self._take("]"):
                    break
                continue
            self._expect("]")
            break

        # Inert slots (zero rate or single-member ranges, e.g. the engine's
        # unused cycles) occupy nothing and are skipped by the animation
        # mapping; only active real ranges can overlap.
        active = sorted(
            (
                cycle
                for cycle in cycles
                if cycle.rate != 0 and cycle.low < cycle.high
            ),
            key=lambda cycle: (cycle.low, cycle.high),
        )
        for previous, current in zip(active, active[1:]):
            # Conservatively treat both authored endpoints as occupied; an
            # ambiguous shared index must not become overlapping GPU metadata.
            if current.low <= previous.high:
                raise CaptureError("source cycle ranges overlap")
        return tuple(cycles)

    def _byte(self) -> int:
        return self._integer(minimum=0, maximum=255)

    def _integer(self, *, minimum: int, maximum: int) -> int:
        start = self.position
        if self._take("-"):
            if minimum >= 0:
                self._fail("negative integer is not permitted here")
        digit_start = self.position
        while self.position < len(self.source) and "0" <= self.source[self.position] <= "9":
            self.position += 1
        if digit_start == self.position:
            self._fail("expected an integer literal")
        if self.position < len(self.source) and self.source[self.position] in ".eE+_":
            self._fail("only decimal integer literals are accepted")
        try:
            value = int(self.source[start : self.position], 10)
        except ValueError as error:
            raise CaptureError("invalid integer literal") from error
        if not minimum <= value <= maximum:
            self._fail(f"integer is outside {minimum}..={maximum}")
        return value

    def _string(self) -> str:
        if self.position >= len(self.source) or self.source[self.position] not in "'\"":
            self._fail("expected a quoted string literal")
        quote = self.source[self.position]
        self.position += 1
        result: list[str] = []
        while self.position < len(self.source):
            char = self.source[self.position]
            self.position += 1
            if char == quote:
                value = "".join(result)
                if any(ord(char) < 0x20 for char in value):
                    self._fail("control characters are not accepted in strings")
                return value
            if char == "\\":
                if self.position >= len(self.source):
                    self._fail("unterminated string escape")
                escaped = self.source[self.position]
                self.position += 1
                if escaped not in {"\\", "'", '"'}:
                    self._fail("unsupported string escape")
                result.append(escaped)
            else:
                result.append(char)
            if len(result) > 255:
                self._fail("source string exceeds 255 characters")
        self._fail("unterminated string literal")

    def _identifier(self) -> str:
        start = self.position
        if start >= len(self.source) or not self.source[start].isascii() or not (
            self.source[start].isalpha() or self.source[start] in "_$"
        ):
            self._fail("expected an object key identifier")
        self.position += 1
        while self.position < len(self.source) and (
            self.source[self.position].isascii()
            and (self.source[self.position].isalnum() or self.source[self.position] in "_$")
        ):
            self.position += 1
        return self.source[start : self.position]

    def _space(self) -> None:
        while self.position < len(self.source) and self.source[self.position] in " \t\r\n":
            self.position += 1

    def _expect(self, expected: str) -> None:
        self._space()
        if not self._take(expected):
            self._fail(f"expected {expected!r}")

    def _take(self, expected: str) -> bool:
        if self.source.startswith(expected, self.position):
            self.position += len(expected)
            return True
        return False

    def _fail(self, message: str) -> None:
        raise CaptureError(f"offset {self.position}: {message}")


def parse_capture(source: str) -> Capture:
    """Parse the supported literal-data subset without evaluating source code."""
    return _LiteralReader(source).parse_capture()


def read_capture(path: Path) -> tuple[Capture, str]:
    try:
        metadata = path.lstat()
    except OSError as error:
        raise CaptureError(f"capture is unavailable: {error}") from error
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise CaptureError("capture path must be a regular non-symlink file")
    if metadata.st_size > MAX_CAPTURE_BYTES:
        raise CaptureError(f"capture exceeds {MAX_CAPTURE_BYTES} bytes")
    flags = (
        os.O_RDONLY
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0)
        | getattr(os, "O_NONBLOCK", 0)
    )
    try:
        descriptor = os.open(path, flags)
        with os.fdopen(descriptor, "rb") as stream:
            opened_metadata = os.fstat(stream.fileno())
            if not stat.S_ISREG(opened_metadata.st_mode):
                raise CaptureError("capture path must be a regular non-symlink file")
            if opened_metadata.st_size > MAX_CAPTURE_BYTES:
                raise CaptureError(f"capture exceeds {MAX_CAPTURE_BYTES} bytes")
            raw = stream.read(MAX_CAPTURE_BYTES + 1)
    except CaptureError:
        raise
    except OSError as error:
        raise CaptureError(f"reading capture failed: {error}") from error
    if len(raw) > MAX_CAPTURE_BYTES:
        raise CaptureError(f"capture exceeds {MAX_CAPTURE_BYTES} bytes")
    try:
        source = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise CaptureError("capture must be UTF-8") from error
    return parse_capture(source), hashlib.sha256(raw).hexdigest()


def _toml_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def _png_chunk(kind: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFF_FFFF)
    )


def _png(width: int, height: int, color_type: int, rows: list[bytes]) -> bytes:
    if len(rows) != height:
        raise CaptureError("internal PNG row count mismatch")
    raw = b"".join(b"\x00" + row for row in rows)
    header = struct.pack(">IIBBBBB", width, height, 8, color_type, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + _png_chunk(b"IHDR", header)
        + _png_chunk(b"IDAT", zlib.compress(raw, 9))
        + _png_chunk(b"IEND", b"")
    )


def _write_private(path: Path, data: bytes) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def _validate_id(scene_id: str) -> None:
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", scene_id):
        raise CaptureError("scene ID must be 1-64 lowercase ASCII letters, digits, or hyphens")


def _validate_url(value: str, label: str) -> None:
    if (
        not isinstance(value, str)
        or not value
        or len(value) > 2048
        or "\\" in value
        or any(
            char.isspace() or ord(char) < 0x20 or 0x7F <= ord(char) <= 0x9F
            for char in value
        )
    ):
        raise CaptureError(f"{label} must be an HTTPS URL without whitespace or controls")
    try:
        parsed = urlsplit(value)
        hostname = parsed.hostname
        _ = parsed.port  # Validate the port syntax even though it is not otherwise used.
        value.encode("utf-8")
    except (UnicodeEncodeError, ValueError) as error:
        raise CaptureError(f"{label} is not a valid HTTPS URL") from error
    if (
        parsed.scheme.lower() != "https"
        or not hostname
        or parsed.username is not None
        or parsed.password is not None
    ):
        raise CaptureError(f"{label} must be an HTTPS URL without user information")


def derive_animation(
    cycles: tuple[SourceCycle, ...], source_speed_adjust: float
) -> str | None:
    """Map active source cycles to signed blend_shift groups (FM-0013).

    Active cycles are those with a nonzero rate and a range of at least two
    palette indices. Source reverse mode 2 reverses the direction (negative
    speed); any other reverse mode is unrepresented in Worldwall and is
    rejected rather than silently mapped.
    """
    if not math.isfinite(source_speed_adjust) or source_speed_adjust <= 0:
        raise CaptureError("source speed adjust must be a positive finite number")
    active = [cycle for cycle in cycles if cycle.rate != 0 and cycle.low < cycle.high]
    for cycle in active:
        if cycle.reverse not in (0, 2):
            raise CaptureError(
                f"unsupported source reverse mode {cycle.reverse}; "
                "only modes 0 and 2 map to signed blend_shift"
            )
    if not active:
        return None
    lines = [
        "schema_version = 1",
        f"# source-speed-adjust = {source_speed_adjust:g}",
        f"# source-cycle-speed-base = {SOURCE_CYCLE_SPEED_BASE:g}",
        "# mapping = FM-0013 signed authored-byte blend_shift; source reverse mode 2 = reversed",
        "",
    ]
    for number, cycle in enumerate(active, 1):
        member_count = cycle.high - cycle.low + 1
        speed = (
            cycle.rate * source_speed_adjust / SOURCE_CYCLE_SPEED_BASE / member_count
        )
        if cycle.reverse == 2:
            speed = -speed
        lines.extend(
            [
                "[[group]]",
                f'name = "source-cycle-{number:02d}"',
                'effect = "blend_shift"',
                f"range = [{cycle.low}, {cycle.high}]",
                f"speed = {speed:.6f}",
                "amplitude = 1.0",
                "phase = 0.0",
                "",
            ]
        )
    return "\n".join(lines)


def materialize_package(
    capture: Capture,
    source_sha256: str,
    output: Path,
    scene_id: str,
    source_url: str,
    source_page: str,
    source_title: str,
    source_speed_adjust: float = DEFAULT_SOURCE_SPEED_ADJUST,
) -> None:
    """Write a new private package directory without replacing existing data."""
    _validate_id(scene_id)
    _validate_url(source_url, "source URL")
    _validate_url(source_page, "source page")
    if (
        not isinstance(source_title, str)
        or len(source_title) > 200
        or any(ord(char) < 0x20 or 0x7F <= ord(char) <= 0x9F for char in source_title)
    ):
        raise CaptureError("source title is too long or contains control characters")
    try:
        source_title.encode("utf-8")
    except UnicodeEncodeError as error:
        raise CaptureError("source title is not valid Unicode text") from error
    if not re.fullmatch(r"[0-9a-f]{64}", source_sha256):
        raise CaptureError("source SHA-256 must be 64 lowercase hexadecimal characters")

    output = Path(output)
    if output.name in {"", ".", ".."}:
        raise CaptureError("output must name a new package directory")
    if output.is_symlink() or os.path.lexists(output):
        raise CaptureError("output already exists; refusing to overwrite it")
    parent = output.parent
    try:
        parent_metadata = parent.lstat()
        if stat.S_ISLNK(parent_metadata.st_mode) or not stat.S_ISDIR(parent_metadata.st_mode):
            raise CaptureError("output parent must be an existing non-symlink directory")
        parent = parent.resolve(strict=True)
    except OSError as error:
        raise CaptureError(f"output parent is unavailable: {error}") from error
    output = parent / output.name
    if os.path.lexists(output):
        raise CaptureError("output already exists; refusing to overwrite it")

    staging = Path(tempfile.mkdtemp(prefix=f".{output.name}.import-", dir=parent))
    try:
        palette = bytes(channel for color in capture.colors for channel in (*color, 255))
        scene_rows = [
            capture.pixels[row * capture.width : (row + 1) * capture.width]
            for row in range(capture.height)
        ]
        animation_document = derive_animation(capture.cycles, source_speed_adjust)
        palette_png = _png(256, 1, 6, [palette])
        scene_png = _png(capture.width, capture.height, 0, scene_rows)
        _write_private(staging / "scene.png", scene_png)
        _write_private(staging / "palette.png", palette_png)

        animation_field = 'animation = "animation.toml"\n' if animation_document else ""
        scene_manifest = (
            "schema_version = 1\n"
            f"id = {_toml_string(scene_id)}\n"
            'image = "scene.png"\n'
            'palette = "palette.png"\n'
            f"{animation_field}"
        )
        _write_private(staging / "scene.toml", scene_manifest.encode("utf-8"))

        provenance = [
            "schema_version = 1",
            f"scene_id = {_toml_string(scene_id)}",
            'source_format = "canvascycle-legacy-object-literal-v1"',
            f"source_filename = {_toml_string(capture.filename)}",
            f"source_sha256 = {_toml_string(source_sha256)}",
            f"source_url = {_toml_string(source_url)}",
            f"source_page = {_toml_string(source_page)}",
            f"source_title = {_toml_string(source_title)}",
            'license_state = "unreviewed"',
            "",
        ]
        _write_private(staging / "provenance.toml", "\n".join(provenance).encode("utf-8"))

        if animation_document:
            _write_private(staging / "animation.toml", animation_document.encode("utf-8"))
        os.chmod(staging, 0o700)
        if os.path.lexists(output):
            raise CaptureError("output appeared during import; refusing to overwrite it")
        os.rename(staging, output)
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise


def _summary(capture: Capture, digest: str) -> str:
    return (
        f"validated {capture.filename}: {capture.width}x{capture.height}, "
        f"palette={len(capture.colors)}, pixels={len(capture.pixels)}, "
        f"source_cycles={len(capture.cycles)}, sha256={digest}"
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path, help="explicit local capture file")
    parser.add_argument(
        "--check-only", action="store_true", help="validate and summarize; write nothing"
    )
    parser.add_argument(
        "--output", type=Path, help="new private package directory (never overwritten)"
    )
    parser.add_argument("--id", dest="scene_id", help="explicit stable local package ID")
    parser.add_argument("--source-url", help="provenance URL; never fetched")
    parser.add_argument("--source-page", help="provenance page URL; never fetched")
    parser.add_argument("--source-title", help="exact source title from local provenance notes")
    parser.add_argument(
        "--source-speed-adjust",
        type=float,
        default=DEFAULT_SOURCE_SPEED_ADJUST,
        help="source site speed-control multiplier for cycle mapping (default 1.0 = base)",
    )
    args = parser.parse_args(argv)

    try:
        capture, digest = read_capture(args.input)
        print(_summary(capture, digest))
        if args.check_only:
            if any(
                (args.output, args.scene_id, args.source_url, args.source_page, args.source_title)
            ):
                parser.error(
                    "--check-only cannot be combined with package output or provenance flags"
                )
            return 0
        if not all(
            (args.output, args.scene_id, args.source_url, args.source_page, args.source_title)
        ):
            parser.error(
                "package output requires --output, --id, and all explicit provenance flags"
            )
        materialize_package(
            capture,
            digest,
            args.output,
            args.scene_id,
            args.source_url,
            args.source_page,
            args.source_title,
            args.source_speed_adjust,
        )
        print(f"wrote private inspection package: {args.output}")
        print(
            "source cycles mapped to signed blend_shift (FM-0013); license state is unreviewed"
        )
        return 0
    except CaptureError as error:
        print(f"canvascycle-import: {error}", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"canvascycle-import: package I/O failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
