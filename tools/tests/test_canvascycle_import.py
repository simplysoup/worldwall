import hashlib
import os
import struct
import tempfile
import unittest
import zlib
from pathlib import Path

from tools.import_canvascycle_capture import (
    MAX_CAPTURE_BYTES,
    MAX_CYCLES,
    MAX_PIXELS,
    CaptureError,
    parse_capture,
    read_capture,
    materialize_package,
)


def sample_source(*, pixels="0,1", cycles=""):
    colors = ",".join(
        f"[{index % 256},{(index + 1) % 256},{(index + 2) % 256}]"
        for index in range(256)
    )
    return (
        "{filename:'sample.LBM',width:2,height:1,"
        f"colors:[{colors}],cycles:[{cycles}],pixels:[{pixels}]}}"
    )


def decode_png(path):
    data = Path(path).read_bytes()
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise AssertionError("bad PNG signature")
    offset = 8
    image_data = bytearray()
    header = None
    while offset < len(data):
        length = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        crc = struct.unpack_from(">I", data, offset + 8 + length)[0]
        if crc != zlib.crc32(kind + payload) & 0xFFFF_FFFF:
            raise AssertionError("bad PNG chunk checksum")
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", payload)
        elif kind == b"IDAT":
            image_data.extend(payload)
        elif kind == b"IEND":
            break
        offset += 12 + length
    if header is None:
        raise AssertionError("missing IHDR")
    width, height, depth, color_type, compression, filtering, interlace = header
    if depth != 8 or compression or filtering or interlace:
        raise AssertionError("unsupported PNG encoding")
    return width, height, color_type, zlib.decompress(image_data)


class CanvasCycleCaptureTests(unittest.TestCase):
    def test_parses_literal_capture_as_data_and_preserves_raw_cycles(self):
        source = sample_source(cycles="{reverse:2,rate:2457,low:10,high:14}")
        capture = parse_capture(source)
        self.assertEqual(capture.filename, "sample.LBM")
        self.assertEqual((capture.width, capture.height), (2, 1))
        self.assertEqual(capture.pixels, bytes([0, 1]))
        self.assertEqual(len(capture.colors), 256)
        self.assertEqual(
            (
                capture.cycles[0].reverse,
                capture.cycles[0].rate,
                capture.cycles[0].low,
                capture.cycles[0].high,
            ),
            (2, 2457, 10, 14),
        )

    def test_rejects_executable_expressions_and_trailing_code(self):
        source = sample_source().replace("pixels:[0,1]", "pixels:alert(1)")
        with self.assertRaises(CaptureError):
            parse_capture(source)
        with self.assertRaises(CaptureError):
            parse_capture(sample_source() + ";launch()")

    def test_rejects_duplicate_and_unknown_object_fields(self):
        with self.assertRaisesRegex(CaptureError, "duplicate source field"):
            parse_capture(sample_source().replace("width:2", "width:2,width:2"))
        with self.assertRaisesRegex(CaptureError, "unsupported source field"):
            parse_capture(sample_source().replace("width:2", "constructor:2,width:2"))

    def test_rejects_bad_dimensions_pixels_and_palette(self):
        with self.assertRaisesRegex(CaptureError, "does not match dimensions"):
            parse_capture(sample_source(pixels="0"))
        with self.assertRaisesRegex(CaptureError, "outside 0..=255"):
            parse_capture(sample_source(pixels="0,256"))
        with self.assertRaisesRegex(CaptureError, "256 RGB entries"):
            parse_capture("{filename:'a.LBM',width:1,height:1,colors:[],pixels:[0]}")
        with self.assertRaises(CaptureError):
            parse_capture(sample_source().replace("width:2", "width:0"))

    def test_rejects_excessive_dimensions_and_cycle_count(self):
        too_wide = sample_source().replace("width:2", f"width:{MAX_PIXELS + 1}")
        with self.assertRaisesRegex(CaptureError, "dimensions exceed"):
            parse_capture(too_wide)
        cycles = ",".join(
            "{reverse:2,rate:1,low:0,high:1}" for _ in range(MAX_CYCLES + 1)
        )
        with self.assertRaisesRegex(CaptureError, "cycle list exceeds"):
            parse_capture(sample_source(cycles=cycles))

    def test_rejects_overlapping_or_malformed_cycles(self):
        overlap = "{reverse:2,rate:1,low:10,high:14},{reverse:2,rate:2,low:14,high:18}"
        with self.assertRaisesRegex(CaptureError, "overlap"):
            parse_capture(sample_source(cycles=overlap))
        with self.assertRaisesRegex(CaptureError, "outside 0..=255"):
            parse_capture(sample_source(cycles="{reverse:2,rate:1,low:10,high:256}"))
        with self.assertRaisesRegex(CaptureError, "requires reverse"):
            parse_capture(sample_source(cycles="{reverse:2,rate:1,low:10}"))

    def test_read_capture_rejects_symlinks_and_oversized_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "capture.txt"
            source.write_text(sample_source(), encoding="utf-8")
            link = root / "link.txt"
            link.symlink_to(source)
            with self.assertRaisesRegex(CaptureError, "non-symlink"):
                read_capture(link)
            oversized = root / "large.txt"
            with oversized.open("wb") as stream:
                stream.truncate(MAX_CAPTURE_BYTES + 1)
            with self.assertRaisesRegex(CaptureError, "exceeds"):
                read_capture(oversized)

    def test_materialized_package_round_trips_indices_palette_and_provenance(self):
        capture = parse_capture(sample_source(cycles="{reverse:2,rate:2457,low:10,high:14}"))
        digest = hashlib.sha256(b"explicit local capture").hexdigest()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "private-package"
            materialize_package(
                capture,
                digest,
                output,
                "canvascycle-sample-day",
                "https://example.invalid/assets/sample.LBM.json",
                "https://example.invalid/canvascycle/",
                "Sample — Day",
            )
            self.assertEqual(output.stat().st_mode & 0o777, 0o700)
            self.assertEqual((output / "scene.png").stat().st_mode & 0o777, 0o600)
            width, height, color_type, raw = decode_png(output / "scene.png")
            self.assertEqual((width, height, color_type, raw), (2, 1, 0, b"\x00\x00\x01"))
            width, height, color_type, raw = decode_png(output / "palette.png")
            expected_palette = bytes(
                channel
                for index in range(256)
                for channel in (index, (index + 1) % 256, (index + 2) % 256, 255)
            )
            self.assertEqual((width, height, color_type), (256, 1, 6))
            self.assertEqual(raw, b"\x00" + expected_palette)
            scene_manifest = (output / "scene.toml").read_text(encoding="utf-8")
            self.assertIn("schema_version = 1\n", scene_manifest)
            self.assertIn('image = "scene.png"', scene_manifest)
            provenance = (output / "provenance.toml").read_text(encoding="utf-8")
            self.assertIn(f'source_sha256 = "{digest}"', provenance)
            self.assertIn('license_state = "unreviewed"', provenance)
            self.assertIn('cycle_semantics = "unmapped-source-native"', provenance)
            cycles = (output / "source-cycles.toml").read_text(encoding="utf-8")
            self.assertIn('rate_semantics = "unknown"', cycles)
            self.assertIn("rate = 2457", cycles)
            self.assertFalse((output / "animation.toml").exists())
            self.assertFalse(
                any(
                    key in provenance
                    for key in ("time_of_day =", "weather =", "temperature =")
                )
            )

    def test_materializer_rejects_malformed_provenance_without_traceback_or_output(self):
        capture = parse_capture(sample_source())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bad_urls = (
                "https://[::1",
                "https://example.test:bad/",
                "https://example.test/a b",
            )
            for index, bad_url in enumerate(bad_urls):
                output = root / f"bad-url-{index}"
                with self.subTest(url=bad_url), self.assertRaises(CaptureError):
                    materialize_package(
                        capture,
                        "a" * 64,
                        output,
                        "canvascycle-sample-day",
                        bad_url,
                        "https://example.test/page",
                        "Sample",
                    )
                self.assertFalse(output.exists())

            with self.assertRaisesRegex(CaptureError, "valid Unicode"):
                materialize_package(
                    capture,
                    "a" * 64,
                    root / "bad-title",
                    "canvascycle-sample-day",
                    "https://example.test/source",
                    "https://example.test/page",
                    "invalid-\ud800-title",
                )
            self.assertFalse((root / "bad-title").exists())

    def test_materializer_refuses_existing_output(self):
        capture = parse_capture(sample_source())
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "existing"
            output.mkdir()
            marker = output / "keep.txt"
            marker.write_text("retain", encoding="utf-8")
            with self.assertRaisesRegex(CaptureError, "already exists"):
                materialize_package(
                    capture,
                    "a" * 64,
                    output,
                    "canvascycle-sample-day",
                    "https://example.invalid/source",
                    "https://example.invalid/page",
                    "Sample",
                )
            self.assertEqual(marker.read_text(encoding="utf-8"), "retain")

    @unittest.skipUnless(
        Path(__file__).resolve().parents[2]
        .joinpath("fixtures/untracked/effectgames/canvascycle-v25heat/V25HEAT.LBM.json")
        .is_file(),
        "private local CanvasCycle capture is not present in this checkout",
    )
    def test_available_private_v25heat_capture_is_bounded_and_complete(self):
        path = (
            Path(__file__).resolve().parents[2]
            / "fixtures/untracked/effectgames/canvascycle-v25heat/V25HEAT.LBM.json"
        )
        capture, digest = read_capture(path)
        self.assertEqual(digest, "2d0ee7fa2d83ab986514e76db05a88f6735a6186dde28fb6466a6fd45ed2305f")
        self.assertEqual(
            (capture.filename, capture.width, capture.height), ("V25HEAT.LBM", 640, 480)
        )
        self.assertEqual(len(capture.colors), 256)
        self.assertEqual(len(capture.pixels), 640 * 480)
        self.assertEqual(len(capture.cycles), 16)


if __name__ == "__main__":
    unittest.main()
