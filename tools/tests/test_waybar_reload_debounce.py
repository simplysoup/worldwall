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
    def test_one_reload_becomes_due_after_a_quiet_period(self):
        debouncer = WaybarReloadDebouncer(16.0)

        self.assertFalse(debouncer.take_if_due(100.0))
        debouncer.palette_changed(100.0)
        self.assertFalse(debouncer.take_if_due(115.999))
        self.assertTrue(debouncer.take_if_due(116.0))
        self.assertFalse(debouncer.take_if_due(132.0))

    def test_each_palette_change_restarts_the_quiet_period(self):
        debouncer = WaybarReloadDebouncer(16.0)

        debouncer.palette_changed(100.0)
        debouncer.palette_changed(108.0)
        self.assertFalse(debouncer.take_if_due(123.999))
        self.assertTrue(debouncer.take_if_due(124.0))
        self.assertFalse(debouncer.take_if_due(140.0))

    def test_invalid_delay_is_rejected(self):
        for delay in (0.0, -1.0, float("inf"), float("nan")):
            with self.subTest(delay=delay), self.assertRaises(ValueError):
                WaybarReloadDebouncer(delay)


if __name__ == "__main__":
    unittest.main(verbosity=2)
