import importlib.util
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "waybar_reload_debounce.py"
SPEC = importlib.util.spec_from_file_location("waybar_reload_debounce", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
WaybarReloadDebouncer = MODULE.WaybarReloadDebouncer


class WaybarReloadDebouncerTests(unittest.TestCase):
    def test_one_palette_change_requests_an_immediate_reload(self):
        debouncer = WaybarReloadDebouncer(16.0)

        self.assertFalse(debouncer.take_if_due(100.0))
        self.assertTrue(debouncer.palette_changed(100.0))
        self.assertFalse(debouncer.has_pending)
        self.assertFalse(debouncer.take_if_due(116.0))

    def test_successive_palette_changes_coalesce_until_the_quiet_period(self):
        debouncer = WaybarReloadDebouncer(16.0)

        self.assertTrue(debouncer.palette_changed(100.0))
        self.assertFalse(debouncer.palette_changed(108.0))
        self.assertFalse(debouncer.palette_changed(112.0))
        self.assertTrue(debouncer.has_pending)
        self.assertFalse(debouncer.take_if_due(127.999))
        self.assertTrue(debouncer.take_if_due(128.0))
        self.assertFalse(debouncer.has_pending)
        self.assertFalse(debouncer.take_if_due(144.0))

    def test_a_new_change_after_quiet_period_is_immediate(self):
        debouncer = WaybarReloadDebouncer(16.0)

        self.assertTrue(debouncer.palette_changed(100.0))
        self.assertTrue(debouncer.palette_changed(116.0))
        self.assertFalse(debouncer.has_pending)

    def test_reset_cancels_pending_reload(self):
        debouncer = WaybarReloadDebouncer(16.0)

        self.assertTrue(debouncer.palette_changed(100.0))
        self.assertFalse(debouncer.palette_changed(108.0))
        debouncer.reset()

        self.assertFalse(debouncer.has_pending)
        self.assertFalse(debouncer.take_if_due(124.0))
        self.assertTrue(debouncer.palette_changed(125.0))

    def test_invalid_delay_is_rejected(self):
        for delay in (0.0, -1.0, float("inf"), float("nan")):
            with self.subTest(delay=delay), self.assertRaises(ValueError):
                WaybarReloadDebouncer(delay)


if __name__ == "__main__":
    unittest.main(verbosity=2)
