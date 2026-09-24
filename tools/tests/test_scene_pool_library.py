import hashlib
import os
import struct
import tempfile
import unittest
import zlib
from pathlib import Path

import tools.scene_pool_library as module
from tools.scene_pool_library import (
    MAX_CYCLES,
    PoolError,
    build_entries,
    main,
    parse_catalog,
    parse_sidecar,
    render_catalog,
    validate_library,
    validate_sidecar,
)


def png_bytes(width, height, color_type, rows):
    raw = b"".join(b"\x00" + row for row in rows)
    header = struct.pack(">IIBBBBB", width, height, 8, color_type, 0, 0, 0)

    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFF_FFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


SAMPLE_SCENE_PNG = png_bytes(2, 1, 0, [b"\x00\x01"])
SAMPLE_PALETTE_PNG = png_bytes(256, 1, 6, [bytes(range(256)) * 4])
ATLAS_PNG = png_bytes(256, 2, 6, [bytes(range(256)) * 4] * 2)

SOURCE_URL = "https://example.invalid/capture/SAMPLE.LBM.json"
SOURCE_PAGE = "https://example.invalid/capture/"

CYCLES_TEXT = (
    "schema_version = 1\n"
    'source_format = "canvascycle-legacy-object-literal-v1"\n'
    'reverse_semantics = "unknown"\n'
    'rate_semantics = "unknown"\n'
    "\n[[cycle]]\nreverse = 2\nrate = 2457\nlow = 10\nhigh = 14\n"
    "\n[[cycle]]\nreverse = 0\nrate = 3003\nlow = 20\nhigh = 24\n"
)

ANIMATION_TEXT = 'schema_version = 1\n\n[[group]]\nname = "wave"\neffect = "hue_cycle"\n'


def write_package(
    root,
    name="heat",
    scene_id="canvascycle-v25heat",
    *,
    with_cycles=True,
    with_animation=False,
    with_time_of_day=False,
    with_family=False,
):
    if with_animation and with_cycles:
        raise AssertionError("test helper keeps animation and source cycles exclusive")
    package = root / name
    package.mkdir(parents=True)
    (package / "scene.png").write_bytes(SAMPLE_SCENE_PNG)
    (package / "palette.png").write_bytes(SAMPLE_PALETTE_PNG)
    manifest = [
        "schema_version = 1",
        f'id = "{scene_id}"',
        'image = "scene.png"',
        'palette = "palette.png"',
    ]
    if with_animation:
        manifest.append('animation = "animation.toml"')
    if with_time_of_day:
        manifest.append('time_of_day = "time_of_day.toml"')
    manifest.append('times = ["day"]')
    if with_family:
        manifest.append('family = "v29-seascape"')
        manifest.append('temperature_band = "mild"')
    (package / "scene.toml").write_text("\n".join(manifest) + "\n")

    if with_animation:
        (package / "animation.toml").write_text(ANIMATION_TEXT)
    if with_time_of_day:
        (package / "time_of_day.toml").write_text(
            'schema_version = 1\natlas = "time_of_day_atlas.png"\n'
            "\n[[keyframe]]\ncivil_seconds = 0\nrow = 0\n"
        )
        (package / "time_of_day_atlas.png").write_bytes(ATLAS_PNG)
    if with_cycles:
        (package / "source-cycles.toml").write_text(CYCLES_TEXT)

    cycle_lines = (
        'cycle_file = "source-cycles.toml"\ncycle_semantics = "unmapped-source-native"\n'
        if with_cycles
        else ""
    )
    provenance = (
        "schema_version = 1\n"
        f'scene_id = "{scene_id}"\n'
        'source_format = "canvascycle-legacy-object-literal-v1"\n'
        'source_filename = "SAMPLE.LBM"\n'
        f'source_sha256 = "{hashlib.sha256(b"sample-source").hexdigest()}"\n'
        f'source_url = "{SOURCE_URL}"\n'
        f'source_page = "{SOURCE_PAGE}"\n'
        'source_title = "Desert Heat Wave - Day (SAMPLE)"\n'
        'license_state = "unreviewed"\n'
        + cycle_lines
    )
    (package / "provenance.toml").write_text(provenance)
    return package


def sidecar_text(
    *,
    pool_id="pool-a",
    source="library",
    scene_ids=("canvascycle-v25heat",),
    rule=True,
    rule_scene=None,
    rule_extra="",
    second_pool=None,
):
    ids = ", ".join(f'"{item}"' for item in scene_ids)
    text = (
        "schema_version = 1\n\n"
        "[[pool]]\n"
        f'id = "{pool_id}"\n'
        f'source = "{source}"\n'
        "enabled = true\n"
        f"scene_ids = [{ids}]\n"
    )
    if second_pool is not None:
        other_id, other_source, other_scenes = second_pool
        other_ids = ", ".join(f'"{item}"' for item in other_scenes)
        text += (
            "\n[[pool]]\n"
            f'id = "{other_id}"\n'
            f'source = "{other_source}"\n'
            "enabled = true\n"
            f"scene_ids = [{other_ids}]\n"
        )
    if rule:
        target = rule_scene if rule_scene is not None else scene_ids[0]
        text += (
            "\n[[scene_rule]]\n"
            f'scene_id = "{target}"\n'
            "enabled = true\n"
            f"{rule_extra}"
        )
    return text


class CatalogRoundTripTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)

    def test_generated_catalog_round_trips_with_provenance(self):
        write_package(self.root)
        entries = build_entries(self.root)
        self.assertEqual(len(entries), 1)
        entry = entries[0]
        self.assertEqual(entry["id"], "canvascycle-v25heat")
        self.assertEqual(entry["package"], "heat")
        self.assertEqual(entry["source_collection"], "canvascycle")
        self.assertEqual(entry["source_url"], SOURCE_URL)
        self.assertEqual(entry["source_title"], "Desert Heat Wave - Day (SAMPLE)")
        self.assertEqual(entry["license_state"], "unreviewed")
        self.assertEqual(entry["cycle_semantics"], "unmapped-source-native")
        self.assertNotIn("license_id", entry)
        # Relationships are never inferred from the source title.
        self.assertEqual(entry["world"], "unknown")
        self.assertEqual(entry["appearance"], "unknown")

        text = render_catalog(entries)
        self.assertEqual(render_catalog(parse_catalog(text)), text)
        stats = validate_library(self.root, parse_catalog(text))
        self.assertEqual(stats["packages"], 1)
        self.assertEqual(stats["cycles"], 2)

    def test_regeneration_is_stable_and_preserves_curated_relations(self):
        write_package(self.root)
        text = render_catalog(build_entries(self.root))
        curated = parse_catalog(text)
        curated[0]["world"] = "desert-world"
        curated[0]["appearance"] = "midday"
        curated[0]["display_name"] = "Desert Heat (day)"
        (self.root / "library.toml").write_text(render_catalog(curated))

        rebuilt = build_entries(self.root)
        self.assertEqual(rebuilt[0]["world"], "desert-world")
        self.assertEqual(rebuilt[0]["appearance"], "midday")
        self.assertEqual(rebuilt[0]["display_name"], "Desert Heat (day)")
        self.assertEqual(rebuilt[0]["image_sha256"], curated[0]["image_sha256"])
        self.assertEqual(render_catalog(parse_catalog(render_catalog(rebuilt))), render_catalog(rebuilt))

    def test_invalid_display_names_are_rejected(self):
        write_package(self.root)
        entries = build_entries(self.root)
        for bad in ("", "  ", "a" * 201, 'has "quotes"', "line\nbreak", "back\\slash"):
            entries[0]["display_name"] = bad
            with self.subTest(name=bad), self.assertRaises(PoolError):
                parse_catalog(render_catalog(entries))

    def test_adaptive_registry_metadata_is_not_copied_into_the_catalog(self):
        write_package(self.root, with_family=True)
        text = render_catalog(build_entries(self.root))
        self.assertNotIn("family", text)
        self.assertNotIn("temperature_band", text)
        self.assertNotIn("v29-seascape", text)
        validate_library(self.root, parse_catalog(text))

    def test_animation_and_time_of_day_assets_are_complete(self):
        write_package(
            self.root,
            "animated",
            "sample-animated",
            with_cycles=False,
            with_animation=True,
            with_time_of_day=True,
        )
        entries = build_entries(self.root)
        entry = entries[0]
        self.assertEqual(entry["animation"], "animation.toml")
        self.assertEqual(entry["time_of_day"], "time_of_day.toml")
        self.assertEqual(entry["time_of_day_atlas"], "time_of_day_atlas.png")
        self.assertNotIn("source_cycles", entry)
        validate_library(self.root, parse_catalog(render_catalog(entries)))


class PathSafetyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)
        self.package = write_package(self.root)
        self.entry = build_entries(self.root)[0]
        self.text = render_catalog([self.entry])

    def test_rejects_escaping_package_paths(self):
        for bad in ("../outside", "/etc", "a/../../b", "sub/../heat", "bad\\name", "./heat"):
            with self.subTest(package=bad):
                entry = dict(self.entry)
                entry["package"] = bad
                with self.assertRaises(PoolError):
                    parse_catalog(render_catalog([entry]))

    def test_rejects_symlinked_package(self):
        outside = Path(self.temporary.name + "-outside")
        outside.mkdir()
        (outside / "scene.toml").write_text("schema_version = 1\n")
        os.symlink(outside, self.root / "linked")
        entry = dict(self.entry)
        entry["package"] = "linked"
        with self.assertRaises(PoolError) as caught:
            validate_library(self.root, parse_catalog(render_catalog([entry])))
        self.assertIn("symlink", str(caught.exception))

    def test_rejects_symlinked_asset(self):
        outside = Path(self.temporary.name + "-outside")
        outside.mkdir()
        (outside / "scene.png").write_bytes(SAMPLE_SCENE_PNG)
        (self.package / "scene.png").unlink()
        os.symlink(outside / "scene.png", self.package / "scene.png")
        with self.assertRaises(PoolError):
            validate_library(self.root, parse_catalog(self.text))

    def test_rejects_unlisted_and_missing_package_files(self):
        (self.package / "extra.txt").write_text("smuggled")
        with self.assertRaises(PoolError) as caught:
            validate_library(self.root, parse_catalog(self.text))
        self.assertIn("unlisted", str(caught.exception))

        (self.package / "extra.txt").unlink()
        (self.package / "palette.png").unlink()
        with self.assertRaises(PoolError) as caught:
            validate_library(self.root, parse_catalog(self.text))
        self.assertIn("missing", str(caught.exception))

    def test_rejects_digest_mismatch(self):
        entry = dict(self.entry)
        entry["image_sha256"] = "0" * 64
        with self.assertRaises(PoolError) as caught:
            validate_library(self.root, parse_catalog(render_catalog([entry])))
        self.assertIn("SHA-256", str(caught.exception))

    def test_rejects_oversized_asset(self):
        original = module.MAX_FILE_BYTES
        module.MAX_FILE_BYTES = 16
        self.addCleanup(setattr, module, "MAX_FILE_BYTES", original)
        with self.assertRaises(PoolError):
            validate_library(self.root, parse_catalog(self.text))

    def test_rejects_malformed_scene_png(self):
        (self.package / "scene.png").write_bytes(b"not a png")
        entry = dict(build_entries(self.root)[0])
        self.assertNotEqual(entry["image_sha256"], self.entry["image_sha256"])
        with self.assertRaises(PoolError):
            validate_library(self.root, parse_catalog(render_catalog([entry])))


class DuplicateAndManifestTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)

    def test_rejects_duplicate_scene_ids_in_catalog(self):
        write_package(self.root)
        entry = build_entries(self.root)[0]
        with self.assertRaises(PoolError):
            parse_catalog(render_catalog([entry, dict(entry)]))

    def test_rejects_manifest_catalog_disagreement(self):
        package = write_package(self.root)
        entry = build_entries(self.root)[0]
        (package / "scene.toml").write_text(
            'schema_version = 1\nid = "other-id"\nimage = "scene.png"\npalette = "palette.png"\n'
        )
        with self.assertRaises(PoolError) as caught:
            validate_library(self.root, parse_catalog(render_catalog([entry])))
        self.assertIn("does not match", str(caught.exception))

    def test_rejects_animation_with_source_cycles(self):
        write_package(self.root)
        entry = dict(build_entries(self.root)[0])
        entry["animation"] = "animation.toml"
        entry["animation_sha256"] = "0" * 64
        with self.assertRaises(PoolError):
            parse_catalog(render_catalog([entry]))

    def test_rejects_license_state_inconsistencies(self):
        write_package(self.root)
        entry = dict(build_entries(self.root)[0])
        entry["license_id"] = "LicenseRef-review-1"
        with self.assertRaises(PoolError):
            parse_catalog(render_catalog([entry]))
        entry["license_state"] = "reviewed-granted"
        parse_catalog(render_catalog([entry]))
        del entry["license_id"]
        with self.assertRaises(PoolError):
            parse_catalog(render_catalog([entry]))

    def test_rejects_invalid_animation_data(self):
        package = write_package(
            self.root, "animated", "sample-animated", with_cycles=False, with_animation=True
        )
        (package / "animation.toml").write_text('schema_version = 2\n\n[[group]]\nname = "x"\n')
        entry = build_entries(self.root)[0]
        with self.assertRaises(PoolError):
            validate_library(self.root, parse_catalog(render_catalog([entry])))

        (package / "animation.toml").write_text("schema_version = 1\n")
        entry = build_entries(self.root)[0]
        with self.assertRaises(PoolError):
            validate_library(self.root, parse_catalog(render_catalog([entry])))

        (package / "animation.toml").write_text(ANIMATION_TEXT)
        validate_library(self.root, parse_catalog(render_catalog(build_entries(self.root))))

    def test_rejects_overlapping_and_malformed_source_cycles(self):
        package = write_package(self.root)
        (package / "source-cycles.toml").write_text(CYCLES_TEXT.replace("low = 20", "low = 12"))
        entry = build_entries(self.root)[0]
        with self.assertRaises(PoolError) as caught:
            validate_library(self.root, parse_catalog(render_catalog([entry])))
        self.assertIn("overlap", str(caught.exception))

        (package / "source-cycles.toml").write_text(
            CYCLES_TEXT.replace("low = 20\nhigh = 24", "low = 24\nhigh = 24")
        )
        entry = build_entries(self.root)[0]
        with self.assertRaises(PoolError):
            validate_library(self.root, parse_catalog(render_catalog([entry])))

    def test_rejects_excessive_cycle_count(self):
        package = write_package(self.root)
        records = [
            "schema_version = 1",
            'source_format = "canvascycle-legacy-object-literal-v1"',
            'reverse_semantics = "unknown"',
            'rate_semantics = "unknown"',
        ]
        for index in range(MAX_CYCLES + 1):
            low = (index * 2) % 250
            records.append(
                f"\n[[cycle]]\nreverse = 0\nrate = 1\nlow = {low}\nhigh = {low + 1}\n"
            )
        (package / "source-cycles.toml").write_text("\n".join(records))
        entry = build_entries(self.root)[0]
        with self.assertRaises(PoolError):
            validate_library(self.root, parse_catalog(render_catalog([entry])))


class SidecarTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)
        write_package(self.root)
        self.entries = build_entries(self.root)
        self.catalog_ids = {entry["id"] for entry in self.entries}

    def test_valid_sidecar_with_library_and_registry_pools(self):
        pools, rules = parse_sidecar(
            sidecar_text(
                rule_extra='time_of_day = ["day"]\nweather_any = ["clear", "cloudy"]\n'
                "temperature_min_c = 28\n"
            )
        )
        self.assertEqual(validate_sidecar(pools, rules, self.catalog_ids), {"pools": 1, "rules": 1})
        pools, rules = parse_sidecar(
            sidecar_text(scene_ids=["v29-seascape"], source="registry", rule=False)
        )
        validate_sidecar(pools, rules, self.catalog_ids)

    def test_rejects_unknown_library_scene(self):
        pools, rules = parse_sidecar(sidecar_text(scene_ids=["not-in-catalog"], rule=False))
        with self.assertRaises(PoolError):
            validate_sidecar(pools, rules, self.catalog_ids)

    def test_rejects_duplicate_pools_membership_and_rules(self):
        with self.assertRaises(PoolError):
            parse_sidecar(
                sidecar_text(pool_id="pool-a", second_pool=("pool-a", "registry", ["scene-2"]), rule=False)
            )
        with self.assertRaises(PoolError) as caught:
            parse_sidecar(
                sidecar_text(
                    pool_id="pool-a",
                    second_pool=("pool-b", "registry", ["canvascycle-v25heat"]),
                    rule=False,
                )
            )
        self.assertIn("already claimed", str(caught.exception))
        duplicate = (
            sidecar_text()
            + '\n[[scene_rule]]\nscene_id = "canvascycle-v25heat"\nenabled = true\n'
        )
        with self.assertRaises(PoolError):
            parse_sidecar(duplicate)

    def test_rejects_malformed_rules(self):
        cases = {
            "min exceeds max": "temperature_min_c = 30.0\ntemperature_max_c = 20.0\n",
            "non-finite temperature": "temperature_min_c = nan\n",
            "out-of-range temperature": "temperature_max_c = 500.0\n",
            "unknown time value": 'time_of_day = ["evening"]\n',
            "unknown weather value": 'weather_any = ["hail"]\n',
            "empty criteria list": "weather_any = []\n",
            "criteria duplicates": 'weather_any = ["clear", "clear"]\n',
            "unknown key": 'moon_phase = ["full"]\n',
        }
        for label, extra in cases.items():
            with self.subTest(case=label):
                with self.assertRaises(PoolError):
                    parse_sidecar(sidecar_text(rule_extra=extra))

    def test_rejects_rule_for_scene_outside_pools_and_bad_documents(self):
        with self.assertRaises(PoolError):
            parse_sidecar(sidecar_text(rule_scene="unpooled-scene"))
        for text in (
            sidecar_text(source="nowhere", rule=False),
            sidecar_text(rule_extra="enabled = false\n"),
            sidecar_text().replace("schema_version = 1", "schema_version = 2", 1),
            sidecar_text(rule=False).replace("[[pool]]", "[[pools]]"),
        ):
            with self.assertRaises(PoolError):
                parse_sidecar(text)

    def test_configured_criteria_are_optional_and_explicit(self):
        pools, rules = parse_sidecar(sidecar_text(rule_extra='weather_any = ["clear", "cloudy"]\n'))
        rule = rules[0]
        self.assertEqual(rule.weather, ("clear", "cloudy"))
        self.assertIsNone(rule.times)
        self.assertIsNone(rule.temperature_min_c)
        self.assertIsNone(rule.temperature_max_c)
        self.assertTrue(pools[0].enabled)
        validate_sidecar(pools, rules, self.catalog_ids)


class TransactionAndCliTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)

    def test_write_catalog_replaces_atomically_and_check_only_writes_nothing(self):
        write_package(self.root)
        self.assertEqual(main(["--library", str(self.root), "--write-catalog"]), 0)
        catalog = self.root / "library.toml"
        first = catalog.read_text()
        self.assertIn("canvascycle-v25heat", first)
        self.assertEqual(oct(catalog.stat().st_mode & 0o777), "0o600")

        before = sorted(path.name for path in self.root.iterdir())
        self.assertEqual(main(["--library", str(self.root), "--check-only"]), 0)
        self.assertEqual(catalog.read_text(), first)
        self.assertEqual(sorted(path.name for path in self.root.iterdir()), before)

    def test_failed_write_preserves_previous_catalog(self):
        write_package(self.root)
        self.assertEqual(main(["--library", str(self.root), "--write-catalog"]), 0)
        catalog = self.root / "library.toml"
        first = catalog.read_text()

        (self.root / "heat" / "palette.png").unlink()
        self.assertEqual(main(["--library", str(self.root), "--write-catalog"]), 1)
        self.assertEqual(catalog.read_text(), first)

    def test_failed_build_creates_no_catalog(self):
        write_package(self.root)
        (self.root / "heat" / "scene.toml").write_text("schema_version = 9\n")
        self.assertEqual(main(["--library", str(self.root), "--write-catalog"]), 1)
        self.assertFalse((self.root / "library.toml").exists())

    def test_invalid_sidecar_fails_before_any_write(self):
        write_package(self.root)
        sidecar = self.root / "scene-pools.toml"
        sidecar.write_text(sidecar_text(scene_ids=["not-in-catalog"], rule=False))
        before = sorted(path.name for path in self.root.iterdir())
        self.assertEqual(
            main(["--library", str(self.root), "--sidecar", str(sidecar), "--write-catalog"]), 1
        )
        self.assertEqual(sorted(path.name for path in self.root.iterdir()), before)
        self.assertFalse((self.root / "library.toml").exists())

    def test_cli_reports_counts_for_valid_sidecar(self):
        write_package(self.root)
        self.assertEqual(main(["--library", str(self.root), "--write-catalog"]), 0)
        sidecar = self.root / "scene-pools.toml"
        sidecar.write_text(sidecar_text())
        self.assertEqual(
            main(["--library", str(self.root), "--sidecar", str(sidecar), "--check-only"]), 0
        )

    def test_cli_rejects_missing_library_root(self):
        self.assertEqual(main(["--library", str(self.root / "nowhere"), "--check-only"]), 1)


if __name__ == "__main__":
    unittest.main()
