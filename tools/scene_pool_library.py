#!/usr/bin/env python3
"""Validate the S12 scene-pool library catalog and pool/rules sidecar.

Local, data-only, and bounded. This tool reads only explicitly supplied local
files, never fetches network data, never executes source code, and never
modifies packages. `--check-only` validates and summarizes without writing.
`--write-catalog` rebuilds <library>/library.toml from the complete packages in
the library root after full validation and replaces the catalog atomically; a
failure preserves any previous catalog. The schemas are documented in
docs/scene-pools.md. Runtime consumption is S12-AC4/AC5 work and is not
implemented here.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import stat
import struct
import sys
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlsplit

MAX_FILE_BYTES = 16 * 1024 * 1024
MAX_PACKAGES = 256
MAX_FILES_PER_PACKAGE = 32
MAX_CYCLES = 256
MAX_PIXELS = 16 * 1024 * 1024
MAX_KEYFRAMES = 256
MAX_GROUPS = 256
PALETTE_ENTRIES = 256

ID_RE = re.compile(r"[a-z0-9][a-z0-9-]{0,63}")
NAME_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,254}")
SHA256_RE = re.compile(r"[0-9a-f]{64}")
LICENSE_ID_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9.+-]{0,127}")

TIME_VALUES = ("dawn", "day", "dusk", "night", "polar_day", "polar_night")
WEATHER_VALUES = ("clear", "cloudy", "rain", "snow", "storm", "fog")
LICENSE_STATES = ("unreviewed", "reviewed-granted", "reviewed-withheld")
CYCLE_SEMANTICS = ("unmapped-source-native",)
SOURCE_COLLECTIONS = {"canvascycle-legacy-object-literal-v1": "canvascycle"}

CATALOG_NAME = "library.toml"
CATALOG_KEY_ORDER = (
    "id",
    "package",
    "source_collection",
    "world",
    "appearance",
    "display_name",
    "image",
    "image_sha256",
    "palette",
    "palette_sha256",
    "animation",
    "animation_sha256",
    "time_of_day",
    "time_of_day_sha256",
    "time_of_day_atlas",
    "time_of_day_atlas_sha256",
    "source_cycles",
    "source_cycles_sha256",
    "cycle_semantics",
    "source_format",
    "source_filename",
    "source_sha256",
    "source_url",
    "source_page",
    "source_title",
    "license_state",
    "license_id",
)
CATALOG_REQUIRED = {
    "id",
    "package",
    "source_collection",
    "world",
    "appearance",
    "image",
    "image_sha256",
    "palette",
    "palette_sha256",
    "source_format",
    "source_sha256",
    "source_url",
    "source_page",
    "source_title",
    "license_state",
}
CATALOG_OPTIONAL = {
    "display_name",
    "animation",
    "animation_sha256",
    "time_of_day",
    "time_of_day_sha256",
    "time_of_day_atlas",
    "time_of_day_atlas_sha256",
    "source_cycles",
    "source_cycles_sha256",
    "cycle_semantics",
    "source_filename",
    "license_id",
}
ASSET_KEYS = ("image", "palette", "animation", "time_of_day", "time_of_day_atlas", "source_cycles")

PROVENANCE_REQUIRED = {
    "schema_version",
    "scene_id",
    "source_format",
    "source_sha256",
    "source_url",
    "source_page",
    "source_title",
    "license_state",
}
PROVENANCE_OPTIONAL = {"source_filename", "license_id", "cycle_file", "cycle_semantics"}

SCENE_MANIFEST_KEYS = {
    "schema_version",
    "id",
    "image",
    "palette",
    "animation",
    "time_of_day",
    "times",
    "weather",
    "seasons",
    "family",
    "temperature_band",
}
TIME_OF_DAY_KEYS = {"schema_version", "basis", "atlas", "keyframe"}
ANIMATION_KEYS = {"schema_version", "group"}
SOURCE_CYCLES_KEYS = {
    "schema_version",
    "source_format",
    "reverse_semantics",
    "rate_semantics",
    "cycle",
}
CYCLE_KEYS = {"reverse", "rate", "low", "high"}

POOL_KEYS = {"id", "source", "enabled", "scene_ids"}
RULE_KEYS = {
    "scene_id",
    "enabled",
    "time_of_day",
    "temperature_min_c",
    "temperature_max_c",
    "weather_any",
}


class PoolError(ValueError):
    """A manifest, sidecar, or package is malformed or unsafe."""


@dataclass(frozen=True)
class Pool:
    pool_id: str
    source: str
    enabled: bool
    scene_ids: tuple[str, ...]


@dataclass(frozen=True)
class Rule:
    scene_id: str
    enabled: bool
    times: tuple[str, ...] | None
    temperature_min_c: float | None
    temperature_max_c: float | None
    weather: tuple[str, ...] | None


def _fail(where: str, message: str) -> None:
    raise PoolError(f"{where}: {message}")


# ---------------------------------------------------------------------------
# Strict TOML value helpers


def _require_keys(table: dict, required: set[str], optional: set[str], where: str) -> None:
    unknown = sorted(set(table) - required - optional)
    if unknown:
        _fail(where, f"unknown key(s): {', '.join(unknown)}")
    missing = sorted(required - set(table))
    if missing:
        _fail(where, f"missing key(s): {', '.join(missing)}")


def _string(table: dict, key: str, where: str) -> str:
    value = table.get(key)
    if not isinstance(value, str):
        _fail(f"{where}.{key}", "must be a string")
    return value


def _bool(table: dict, key: str, where: str) -> bool:
    value = table.get(key)
    if not isinstance(value, bool):
        _fail(f"{where}.{key}", "must be a boolean")
    return value


def _integer(table: dict, key: str, where: str, minimum: int, maximum: int) -> int:
    value = table.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        _fail(f"{where}.{key}", "must be an integer")
    if not minimum <= value <= maximum:
        _fail(f"{where}.{key}", f"must be in {minimum}..={maximum}")
    return value


def _number(table: dict, key: str, where: str, minimum: float, maximum: float) -> float:
    value = table.get(key)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        _fail(f"{where}.{key}", "must be a number")
    value = float(value)
    if not math.isfinite(value):
        _fail(f"{where}.{key}", "must be finite")
    if not minimum <= value <= maximum:
        _fail(f"{where}.{key}", f"must be in {minimum}..={maximum}")
    return value


def _string_list(
    table: dict,
    key: str,
    where: str,
    *,
    allowed: tuple[str, ...] | None = None,
) -> tuple[str, ...] | None:
    if key not in table:
        return None
    value = table[key]
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        _fail(f"{where}.{key}", "must be an array of strings")
    if not value:
        _fail(f"{where}.{key}", "must not be empty")
    if len(value) != len(set(value)):
        _fail(f"{where}.{key}", "must not contain duplicates")
    if allowed is not None:
        for item in value:
            if item not in allowed:
                _fail(f"{where}.{key}", f"unsupported value {item!r}")
    return tuple(value)


def _tag_list(value: object, where: str) -> None:
    if not isinstance(value, list) or not all(
        isinstance(item, str) and item and len(item) <= 32 for item in value
    ):
        _fail(where, "must be an array of short non-empty strings")
    if len(value) > 32:
        _fail(where, "has too many values")


def _id(value: str, where: str) -> str:
    if not ID_RE.fullmatch(value):
        _fail(where, "must be 1-64 lowercase ASCII letters, digits, or hyphens")
    return value


def _name(value: str, where: str) -> str:
    if not NAME_RE.fullmatch(value):
        _fail(where, "is not a safe file name")
    return value


def _digest(value: str, where: str) -> str:
    if not SHA256_RE.fullmatch(value):
        _fail(where, "must be 64 lowercase hexadecimal characters")
    return value


def _url(value: str, where: str) -> str:
    if (
        not value
        or len(value) > 2048
        or "\\" in value
        or any(char.isspace() or ord(char) < 0x20 or 0x7F <= ord(char) <= 0x9F for char in value)
    ):
        _fail(where, "must be an HTTPS URL without whitespace or controls")
    try:
        parsed = urlsplit(value)
        hostname = parsed.hostname
        _ = parsed.port
        value.encode("utf-8")
    except (UnicodeEncodeError, ValueError) as error:
        _fail(where, f"is not a valid HTTPS URL: {error}")
    if parsed.scheme.lower() != "https" or not hostname or parsed.username or parsed.password:
        _fail(where, "must be an HTTPS URL without user information")
    return value


def _title(value: str, where: str) -> str:
    try:
        value.encode("utf-8")
    except UnicodeEncodeError:
        _fail(where, "is not valid Unicode text")
    if len(value) > 200 or any(ord(char) < 0x20 or 0x7F <= ord(char) <= 0x9F for char in value):
        _fail(where, "is too long or contains control characters")
    return value


# ---------------------------------------------------------------------------
# Path-safe, bounded file access


def _safe_relative(ref: str, where: str) -> tuple[str, ...]:
    if not ref or "\\" in ref or ref.startswith("/") or ref.endswith("/"):
        _fail(where, "must be a relative path without backslashes")
    parts = tuple(ref.split("/"))
    for part in parts:
        _name(part, where)
    return parts


def _walk_dir(base: Path, parts: tuple[str, ...], where: str) -> Path:
    current = base
    for part in parts:
        current = current / part
        try:
            info = current.lstat()
        except OSError as error:
            _fail(where, f"unavailable: {error}")
        if stat.S_ISLNK(info.st_mode):
            _fail(where, "symlinked path components are not allowed")
    if not current.is_dir():
        _fail(where, "must name a directory")
    return current


def _read_regular(path: Path, where: str) -> bytes:
    try:
        info = path.lstat()
    except OSError as error:
        _fail(where, f"unavailable: {error}")
    if stat.S_ISLNK(info.st_mode) or not stat.S_ISREG(info.st_mode):
        _fail(where, "must be a regular non-symlink file")
    if info.st_size > MAX_FILE_BYTES:
        _fail(where, f"exceeds the {MAX_FILE_BYTES}-byte file limit")
    flags = (
        os.O_RDONLY
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0)
        | getattr(os, "O_NONBLOCK", 0)
    )
    try:
        descriptor = os.open(path, flags)
        with os.fdopen(descriptor, "rb") as stream:
            opened = os.fstat(stream.fileno())
            if not stat.S_ISREG(opened.st_mode):
                _fail(where, "must be a regular non-symlink file")
            if opened.st_size > MAX_FILE_BYTES:
                _fail(where, f"exceeds the {MAX_FILE_BYTES}-byte file limit")
            raw = stream.read(MAX_FILE_BYTES + 1)
    except PoolError:
        raise
    except OSError as error:
        _fail(where, f"read failed: {error}")
    if len(raw) > MAX_FILE_BYTES:
        _fail(where, f"exceeds the {MAX_FILE_BYTES}-byte file limit")
    return raw


def _read_text(path: Path, where: str) -> str:
    try:
        return _read_regular(path, where).decode("utf-8")
    except UnicodeDecodeError:
        _fail(where, "must be UTF-8")


def _load_toml(path: Path, where: str) -> dict:
    try:
        return tomllib.loads(_read_text(path, where))
    except tomllib.TOMLDecodeError as error:
        _fail(where, f"invalid TOML: {error}")


def _table_list(document: dict, key: str, where: str) -> list[dict]:
    value = document.get(key, [])
    if not isinstance(value, list) or not all(isinstance(item, dict) for item in value):
        _fail(f"{where}.{key}", "must be an array of tables")
    return value


def _png_header(data: bytes, where: str) -> tuple[int, int, int, int]:
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        _fail(where, "is not a PNG file")
    if len(data) < 8 + 8 + 13 + 4:
        _fail(where, "PNG is truncated")
    length = struct.unpack_from(">I", data, 8)[0]
    if data[12:16] != b"IHDR" or length != 13:
        _fail(where, "PNG must start with a standard IHDR chunk")
    width, height, depth, color_type, compression, filtering, interlace = struct.unpack_from(
        ">IIBBBBB", data, 16
    )
    if compression != 0 or filtering != 0 or interlace not in (0, 1):
        _fail(where, "uses an unsupported PNG encoding")
    return width, height, depth, color_type


# ---------------------------------------------------------------------------
# Catalog model


def parse_catalog(text: str) -> list[dict[str, str]]:
    where = CATALOG_NAME
    try:
        document = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        _fail(where, f"invalid TOML: {error}")
    _require_keys(document, {"schema_version", "scene"}, set(), where)
    if document["schema_version"] != 1:
        _fail(where, "schema_version must be 1")
    entries: list[dict[str, str]] = []
    seen: set[str] = set()
    for index, raw in enumerate(_table_list(document, "scene", where)):
        entry_where = f"{where}[[scene]] #{index + 1}"
        if not all(isinstance(value, str) for value in raw.values()):
            _fail(entry_where, "all catalog values must be strings")
        _require_keys(raw, CATALOG_REQUIRED, CATALOG_OPTIONAL, entry_where)
        entry = dict(raw)
        _id(entry["id"], f"{entry_where}.id")
        if entry["id"] in seen:
            _fail(entry_where, f"duplicate scene ID {entry['id']!r}")
        seen.add(entry["id"])
        _safe_relative(entry["package"], f"{entry_where}.package")
        _id(entry["source_collection"], f"{entry_where}.source_collection")
        for key in ("world", "appearance"):
            if entry[key] != "unknown":
                _id(entry[key], f"{entry_where}.{key}")
        if "display_name" in entry:
            name = entry["display_name"]
            if not name.strip() or '"' in name or "\\" in name:
                _fail(
                    entry_where,
                    "display_name must be non-empty plain text without quotes or backslashes",
                )
            _title(name, f"{entry_where}.display_name")
        if entry["appearance"] != "unknown" and entry["world"] == "unknown":
            _fail(entry_where, "appearance requires a known world")
        for key in ASSET_KEYS:
            if (key in entry) != (f"{key}_sha256" in entry):
                _fail(entry_where, f"{key} and {key}_sha256 must be declared together")
            if key in entry:
                _name(entry[key], f"{entry_where}.{key}")
                _digest(entry[f"{key}_sha256"], f"{entry_where}.{key}_sha256")
        if "animation" in entry and "source_cycles" in entry:
            _fail(entry_where, "animation and source_cycles are mutually exclusive")
        tod_keys = ("time_of_day", "time_of_day_sha256", "time_of_day_atlas", "time_of_day_atlas_sha256")
        if len({key in entry for key in tod_keys}) != 1:
            _fail(entry_where, "time_of_day, its atlas, and both digests must be declared together")
        if ("source_cycles" in entry) != ("cycle_semantics" in entry):
            _fail(entry_where, "cycle_semantics must be declared exactly with source_cycles")
        if "cycle_semantics" in entry and entry["cycle_semantics"] not in CYCLE_SEMANTICS:
            _fail(entry_where, f"unsupported cycle_semantics {entry['cycle_semantics']!r}")
        if entry["source_format"] not in SOURCE_COLLECTIONS:
            _fail(entry_where, f"unsupported source_format {entry['source_format']!r}")
        if SOURCE_COLLECTIONS[entry["source_format"]] != entry["source_collection"]:
            _fail(entry_where, "source_collection does not match source_format")
        if "source_filename" in entry:
            _name(entry["source_filename"], f"{entry_where}.source_filename")
        _digest(entry["source_sha256"], f"{entry_where}.source_sha256")
        _url(entry["source_url"], f"{entry_where}.source_url")
        _url(entry["source_page"], f"{entry_where}.source_page")
        _title(entry["source_title"], f"{entry_where}.source_title")
        if entry["license_state"] not in LICENSE_STATES:
            _fail(entry_where, f"unsupported license_state {entry['license_state']!r}")
        if entry["license_state"] == "reviewed-granted":
            if "license_id" not in entry:
                _fail(entry_where, "reviewed-granted requires license_id")
            if not LICENSE_ID_RE.fullmatch(entry["license_id"]):
                _fail(entry_where, "license_id is not a valid identifier")
        elif "license_id" in entry:
            _fail(entry_where, "license_id is only allowed with reviewed-granted")
        entries.append(entry)
    if not entries:
        _fail(where, "must list at least one scene")
    return entries


def render_catalog(entries: list[dict[str, str]]) -> str:
    lines = ["schema_version = 1", ""]
    for entry in entries:
        lines.append("[[scene]]")
        for key in CATALOG_KEY_ORDER:
            if key in entry:
                lines.append(f"{key} = {json.dumps(entry[key], ensure_ascii=False)}")
        lines.append("")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Library validation


def _validate_scene_manifest(document: dict, entry: dict[str, str]) -> None:
    where = f"{entry['id']}/scene.toml"
    _require_keys(
        document,
        {"schema_version", "id", "image", "palette"},
        SCENE_MANIFEST_KEYS - {"schema_version", "id", "image", "palette"},
        where,
    )
    if document["schema_version"] not in (1, 2):
        _fail(where, "schema_version must be 1 or 2")
    if _string(document, "id", where) != entry["id"]:
        _fail(where, "id does not match the catalog entry")
    for key in ("image", "palette"):
        if _string(document, key, where) != entry[key]:
            _fail(where, f"{key} does not match the catalog entry")
    for key in ("animation", "time_of_day"):
        if (key in document) != (key in entry):
            _fail(where, f"{key} declaration disagrees with the catalog entry")
        if key in entry and _string(document, key, where) != entry[key]:
            _fail(where, f"{key} does not match the catalog entry")
    for key in ("times", "weather", "seasons"):
        if key in document:
            _tag_list(document[key], f"{where}.{key}")
    for key in ("family", "temperature_band"):
        if key in document:
            _string(document, key, where)


def _validate_provenance(document: dict, entry: dict[str, str]) -> None:
    where = f"{entry['id']}/provenance.toml"
    _require_keys(document, PROVENANCE_REQUIRED, PROVENANCE_OPTIONAL, where)
    if document["schema_version"] != 1:
        _fail(where, "schema_version must be 1")
    if _string(document, "scene_id", where) != entry["id"]:
        _fail(where, "scene_id does not match the catalog entry")
    for key in ("source_format", "source_sha256", "source_url", "source_page", "source_title", "license_state"):
        if key not in entry or _string(document, key, where) != entry[key]:
            _fail(where, f"{key} does not match the catalog entry")
    mapped = {"cycle_file": "source_cycles", "cycle_semantics": "cycle_semantics"}
    for key in ("source_filename", "license_id", "cycle_file", "cycle_semantics"):
        entry_key = mapped.get(key, key)
        if (key in document) != (entry_key in entry):
            _fail(where, f"{key} declaration disagrees with the catalog entry")
        if key in document and _string(document, key, where) != entry[entry_key]:
            _fail(where, f"{key} does not match the catalog entry")


def _validate_source_cycles(text: str, entry: dict[str, str]) -> int:
    where = f"{entry['id']}/{entry['source_cycles']}"
    try:
        document = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        _fail(where, f"invalid TOML: {error}")
    _require_keys(document, SOURCE_CYCLES_KEYS - {"cycle"}, {"cycle"}, where)
    if document["schema_version"] != 1:
        _fail(where, "schema_version must be 1")
    if document["source_format"] != "canvascycle-legacy-object-literal-v1":
        _fail(where, "unsupported source_format")
    for key in ("reverse_semantics", "rate_semantics"):
        if document[key] != "unknown":
            _fail(where, f"{key} must be unknown until source semantics are established")
    cycles = _table_list(document, "cycle", where)
    if not cycles:
        _fail(where, "must contain at least one cycle record")
    if len(cycles) > MAX_CYCLES:
        _fail(where, f"exceeds {MAX_CYCLES} cycle records")
    ranges: list[tuple[int, int]] = []
    for index, raw in enumerate(cycles):
        cycle_where = f"{where}[[cycle]] #{index + 1}"
        _require_keys(raw, CYCLE_KEYS, set(), cycle_where)
        _integer(raw, "reverse", cycle_where, -(2**31), 2**31 - 1)
        _integer(raw, "rate", cycle_where, -(2**31), 2**31 - 1)
        low = _integer(raw, "low", cycle_where, 0, 255)
        high = _integer(raw, "high", cycle_where, 0, 255)
        if low >= high:
            _fail(cycle_where, "cycle range must contain at least two palette indices")
        ranges.append((low, high))
    ordered = sorted(ranges)
    for previous, current in zip(ordered, ordered[1:]):
        # Both authored endpoints are treated as occupied, matching the
        # importer: an ambiguous shared index must not become GPU metadata.
        if current[0] <= previous[1]:
            _fail(where, "source cycle ranges overlap")
    return len(cycles)


def _validate_animation(text: str, entry: dict[str, str]) -> int:
    where = f"{entry['id']}/{entry['animation']}"
    try:
        document = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        _fail(where, f"invalid TOML: {error}")
    _require_keys(document, ANIMATION_KEYS, set(), where)
    if document["schema_version"] != 1:
        _fail(where, "schema_version must be 1")
    groups = _table_list(document, "group", where)
    if not groups:
        _fail(where, "must contain at least one [[group]]")
    if len(groups) > MAX_GROUPS:
        _fail(where, f"exceeds {MAX_GROUPS} groups")
    # Full effect semantics stay with the accepted runtime loader (S04).
    return len(groups)


def _validate_time_of_day(document: dict, entry: dict[str, str]) -> None:
    where = f"{entry['id']}/{entry['time_of_day']}"
    _require_keys(
        document,
        {"schema_version", "atlas"},
        TIME_OF_DAY_KEYS - {"schema_version", "atlas"},
        where,
    )
    if document["schema_version"] not in (1, 2):
        _fail(where, "schema_version must be 1 or 2")
    atlas = _name(_string(document, "atlas", where), f"{where}.atlas")
    if atlas != entry["time_of_day_atlas"]:
        _fail(where, "atlas does not match the catalog entry")
    keyframes = _table_list(document, "keyframe", where)
    if not keyframes:
        _fail(where, "must contain at least one [[keyframe]]")
    if len(keyframes) > MAX_KEYFRAMES:
        _fail(where, f"exceeds {MAX_KEYFRAMES} keyframes")


def validate_library(root: Path, entries: list[dict[str, str]]) -> dict[str, int]:
    stats = {"packages": 0, "files": 0, "cycles": 0, "groups": 0}
    for entry in entries:
        where = entry["id"]
        package_dir = _walk_dir(
            root, _safe_relative(entry["package"], f"{where}.package"), f"{where}.package"
        )
        try:
            children = sorted(os.scandir(package_dir), key=lambda item: item.name)
        except OSError as error:
            _fail(where, f"listing package failed: {error}")
        if not children or len(children) > MAX_FILES_PER_PACKAGE:
            _fail(where, f"package must contain 1..{MAX_FILES_PER_PACKAGE} files")
        declared = {entry[key] for key in ASSET_KEYS if key in entry}
        declared |= {"scene.toml", "provenance.toml"}
        names = set()
        for child in children:
            info = child.stat(follow_symlinks=False)
            if stat.S_ISLNK(info.st_mode) or not stat.S_ISREG(info.st_mode):
                _fail(f"{where}/{child.name}", "packages contain regular non-symlink files only")
            names.add(child.name)
        if names != declared:
            extras = sorted(names - declared)
            missing = sorted(declared - names)
            _fail(
                where,
                "package file set does not match the declared assets"
                + (f"; unlisted: {', '.join(extras)}" if extras else "")
                + (f"; missing: {', '.join(missing)}" if missing else ""),
            )

        _validate_scene_manifest(_load_toml(package_dir / "scene.toml", f"{where}/scene.toml"), entry)
        _validate_provenance(_load_toml(package_dir / "provenance.toml", f"{where}/provenance.toml"), entry)

        for key in ASSET_KEYS:
            if key not in entry:
                continue
            asset_where = f"{where}/{entry[key]}"
            raw = _read_regular(package_dir / entry[key], asset_where)
            if hashlib.sha256(raw).hexdigest() != entry[f"{key}_sha256"]:
                _fail(asset_where, "SHA-256 does not match the catalog entry")
            if key == "image":
                width, height, depth, color_type = _png_header(raw, asset_where)
                if depth != 8 or color_type not in (0, 3):
                    _fail(asset_where, "scene PNG must be 8-bit grayscale or indexed")
                if width < 1 or height < 1 or width * height > MAX_PIXELS:
                    _fail(asset_where, f"scene PNG exceeds {MAX_PIXELS} pixels")
            elif key == "palette":
                header = _png_header(raw, asset_where)
                if header != (PALETTE_ENTRIES, 1, 8, 6):
                    _fail(asset_where, "palette PNG must be 256 x 1 8-bit RGBA")
            elif key == "source_cycles":
                stats["cycles"] += _validate_source_cycles(_read_text(package_dir / entry[key], asset_where), entry)
            elif key == "animation":
                stats["groups"] += _validate_animation(_read_text(package_dir / entry[key], asset_where), entry)
            elif key == "time_of_day":
                _validate_time_of_day(_load_toml(package_dir / entry[key], asset_where), entry)
        stats["packages"] += 1
        stats["files"] += len(children)
    return stats


# ---------------------------------------------------------------------------
# Sidecar model


def parse_sidecar(text: str) -> tuple[list[Pool], list[Rule]]:
    where = "scene-pools.toml"
    try:
        document = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        _fail(where, f"invalid TOML: {error}")
    _require_keys(document, {"schema_version", "pool"}, {"scene_rule"}, where)
    if document["schema_version"] != 1:
        _fail(where, "schema_version must be 1")

    pools: list[Pool] = []
    pool_ids: set[str] = set()
    membership: dict[str, str] = {}
    for index, raw in enumerate(_table_list(document, "pool", where)):
        pool_where = f"{where}[[pool]] #{index + 1}"
        _require_keys(raw, POOL_KEYS, set(), pool_where)
        pool_id = _id(_string(raw, "id", pool_where), f"{pool_where}.id")
        if pool_id in pool_ids:
            _fail(pool_where, f"duplicate pool ID {pool_id!r}")
        pool_ids.add(pool_id)
        source = _string(raw, "source", pool_where)
        if source not in ("library", "registry"):
            _fail(f"{pool_where}.source", "must be library or registry")
        enabled = _bool(raw, "enabled", pool_where)
        scene_ids = _string_list(raw, "scene_ids", pool_where) or ()
        for scene_id in scene_ids:
            _id(scene_id, f"{pool_where}.scene_ids")
            if scene_id in membership:
                _fail(
                    pool_where,
                    f"scene {scene_id!r} is already claimed by pool {membership[scene_id]!r}",
                )
            membership[scene_id] = pool_id
        pools.append(Pool(pool_id, source, enabled, tuple(scene_ids)))
    if not pools:
        _fail(where, "must define at least one [[pool]]")

    rules: list[Rule] = []
    rule_ids: set[str] = set()
    for index, raw in enumerate(_table_list(document, "scene_rule", where)):
        rule_where = f"{where}[[scene_rule]] #{index + 1}"
        _require_keys(raw, {"scene_id", "enabled"}, RULE_KEYS - {"scene_id", "enabled"}, rule_where)
        scene_id = _id(_string(raw, "scene_id", rule_where), f"{rule_where}.scene_id")
        if scene_id in rule_ids:
            _fail(rule_where, f"duplicate rule for scene {scene_id!r}")
        rule_ids.add(scene_id)
        if scene_id not in membership:
            _fail(rule_where, f"scene {scene_id!r} does not belong to any pool")
        enabled = _bool(raw, "enabled", rule_where)
        times = _string_list(raw, "time_of_day", rule_where, allowed=TIME_VALUES)
        weather = _string_list(raw, "weather_any", rule_where, allowed=WEATHER_VALUES)
        minimum = (
            _number(raw, "temperature_min_c", rule_where, -100.0, 100.0)
            if "temperature_min_c" in raw
            else None
        )
        maximum = (
            _number(raw, "temperature_max_c", rule_where, -100.0, 100.0)
            if "temperature_max_c" in raw
            else None
        )
        if minimum is not None and maximum is not None and minimum > maximum:
            _fail(rule_where, "temperature_min_c must not exceed temperature_max_c")
        rules.append(Rule(scene_id, enabled, times, minimum, maximum, weather))
    return pools, rules


def validate_sidecar(pools: list[Pool], rules: list[Rule], catalog_ids: set[str]) -> dict[str, int]:
    for pool in pools:
        if pool.source == "library":
            for scene_id in pool.scene_ids:
                if scene_id not in catalog_ids:
                    _fail(f"pool {pool.pool_id!r}", f"library scene {scene_id!r} is not in {CATALOG_NAME}")
    return {"pools": len(pools), "rules": len(rules)}


# ---------------------------------------------------------------------------
# Catalog generation


def build_entries(root: Path) -> list[dict[str, str]]:
    """Rebuild catalog entries from complete packages, preserving relations."""
    previous: dict[str, tuple[str, str]] = {}
    catalog_path = root / CATALOG_NAME
    if os.path.lexists(catalog_path):
        for entry in parse_catalog(_read_text(catalog_path, CATALOG_NAME)):
            previous[entry["id"]] = (
                entry["world"],
                entry["appearance"],
                entry.get("display_name"),
            )

    try:
        children = sorted(os.scandir(root), key=lambda item: item.name)
    except OSError as error:
        _fail(CATALOG_NAME, f"listing library root failed: {error}")
    packages = []
    for child in children:
        if child.name.startswith(".") or child.name == CATALOG_NAME:
            continue
        info = child.stat(follow_symlinks=False)
        if stat.S_ISREG(info.st_mode) and not stat.S_ISLNK(info.st_mode):
            continue  # unrelated regular files (for example a sidecar) are ignored
        packages.append(child)
    if not packages:
        _fail(CATALOG_NAME, "library root contains no package directories")
    if len(packages) > MAX_PACKAGES:
        _fail(CATALOG_NAME, f"library exceeds {MAX_PACKAGES} packages")

    entries: list[dict[str, str]] = []
    seen: set[str] = set()
    for child in packages:
        where = child.name
        _name(child.name, where)
        info = child.stat(follow_symlinks=False)
        if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode):
            _fail(where, "library root may contain only package directories")
        package_dir = Path(child.path)
        manifest = _load_toml(package_dir / "scene.toml", f"{where}/scene.toml")
        provenance = _load_toml(package_dir / "provenance.toml", f"{where}/provenance.toml")
        _require_keys(
            manifest,
            {"schema_version", "id", "image", "palette"},
            SCENE_MANIFEST_KEYS - {"schema_version", "id", "image", "palette"},
            f"{where}/scene.toml",
        )
        _require_keys(provenance, PROVENANCE_REQUIRED, PROVENANCE_OPTIONAL, f"{where}/provenance.toml")

        entry: dict[str, str] = {}
        scene_id = _id(_string(manifest, "id", f"{where}/scene.toml.id"), f"{where}/scene.toml.id")
        if scene_id in seen:
            _fail(where, f"duplicate scene ID {scene_id!r}")
        seen.add(scene_id)
        entry["id"] = scene_id
        entry["package"] = child.name
        world, appearance, display_name = previous.get(
            scene_id, ("unknown", "unknown", None)
        )
        entry["world"], entry["appearance"] = world, appearance
        if display_name:
            entry["display_name"] = display_name

        for key in ("image", "palette"):
            entry[key] = _name(_string(manifest, key, f"{where}/scene.toml.{key}"), f"{where}/scene.toml.{key}")
        for key in ("animation", "time_of_day"):
            if key in manifest:
                entry[key] = _name(
                    _string(manifest, key, f"{where}/scene.toml.{key}"), f"{where}/scene.toml.{key}"
                )

        if "time_of_day" in entry:
            tod = _load_toml(package_dir / entry["time_of_day"], f"{where}/{entry['time_of_day']}")
            _require_keys(
                tod,
                {"schema_version", "atlas"},
                TIME_OF_DAY_KEYS - {"schema_version", "atlas"},
                f"{where}/{entry['time_of_day']}",
            )
            entry["time_of_day_atlas"] = _name(_string(tod, "atlas", f"{where}.atlas"), f"{where}.atlas")
        if "cycle_file" in provenance:
            entry["source_cycles"] = _name(
                _string(provenance, "cycle_file", f"{where}/provenance.cycle_file"),
                f"{where}/provenance.cycle_file",
            )
            entry["cycle_semantics"] = _string(provenance, "cycle_semantics", f"{where}/provenance.cycle_semantics")

        for key in (
            "source_format",
            "source_filename",
            "source_sha256",
            "source_url",
            "source_page",
            "source_title",
            "license_state",
            "license_id",
        ):
            if key in provenance:
                entry[key] = _string(provenance, key, f"{where}/provenance.{key}")
        if entry.get("source_format") not in SOURCE_COLLECTIONS:
            _fail(f"{where}/provenance.source_format", "is not a supported source format")
        entry["source_collection"] = SOURCE_COLLECTIONS[entry["source_format"]]

        for key in ASSET_KEYS:
            if key in entry:
                raw = _read_regular(package_dir / entry[key], f"{where}/{entry[key]}")
                entry[f"{key}_sha256"] = hashlib.sha256(raw).hexdigest()
        entries.append(entry)

    entries.sort(key=lambda entry: entry["id"])
    return entries


def write_catalog(path: Path, text: str) -> None:
    descriptor, temporary = tempfile.mkstemp(prefix=f".{CATALOG_NAME}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(text.encode("utf-8"))
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    except Exception:
        if os.path.lexists(temporary):
            os.unlink(temporary)
        raise


# ---------------------------------------------------------------------------
# CLI


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", required=True, type=Path, help="explicit library root directory")
    parser.add_argument("--sidecar", type=Path, help="optional pool/rules sidecar to validate")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check-only", action="store_true", help="validate and summarize; write nothing")
    mode.add_argument("--write-catalog", action="store_true", help="rebuild library.toml after validation")
    args = parser.parse_args(argv)

    try:
        root = args.library
        try:
            root_metadata = root.lstat()
        except OSError as error:
            raise PoolError(f"library root is unavailable: {error}") from error
        if stat.S_ISLNK(root_metadata.st_mode) or not stat.S_ISDIR(root_metadata.st_mode):
            raise PoolError("library root must be an existing non-symlink directory")
        root = root.resolve(strict=True)

        if args.write_catalog:
            text = render_catalog(build_entries(root))
            entries = parse_catalog(text)
            wrote: str | None = text
        else:
            entries = parse_catalog(_read_text(root / CATALOG_NAME, CATALOG_NAME))
            wrote = None
        stats = validate_library(root, entries)

        detail = (
            f"packages={stats['packages']} files={stats['files']} "
            f"source_cycles={stats['cycles']} animation_groups={stats['groups']}"
        )
        if args.sidecar:
            pools, rules = parse_sidecar(_read_text(args.sidecar, "scene-pools.toml"))
            counts = validate_sidecar(pools, rules, {entry["id"] for entry in entries})
            detail += f" pools={counts['pools']} rules={counts['rules']}"
        if wrote is not None:
            write_catalog(root / CATALOG_NAME, wrote)
            print(f"wrote {root / CATALOG_NAME} with {len(entries)} scene(s)")
        else:
            print(f"validated {len(entries)} catalog scene(s)")
        print(detail)
        print("license states and unmapped cycle semantics are recorded, never upgraded")
        return 0
    except PoolError as error:
        print(f"scene-pool-library: {error}", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"scene-pool-library: I/O failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
