from __future__ import annotations

import importlib.machinery
import importlib.util
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

WATCHER_PATH = Path(__file__).resolve().parents[1] / "worldwall-theme-watch"
loader = importlib.machinery.SourceFileLoader("worldwall_theme_watch_under_test", str(WATCHER_PATH))
spec = importlib.util.spec_from_loader(loader.name, loader)
watcher = importlib.util.module_from_spec(spec)
assert spec and spec.loader
sys.modules[spec.name] = watcher
spec.loader.exec_module(watcher)


class WallpaperTerminalThemeTests(unittest.TestCase):
    def test_primary_surface_stays_close_to_current_dark_theme_and_tints_to_wallpaper(self):
        palette = [(14, 45, 18)] * 256
        surface = watcher.choose_wallpaper_surface(palette, Counter({0: 1000}))
        current = watcher.TERMINAL_BASE_BACKGROUND

        self.assertNotEqual(surface, current)
        self.assertLess(sum(abs(surface[i] - current[i]) for i in range(3)), 20)
        self.assertGreater(surface[1], surface[0])
        self.assertLess(watcher.relative_luminance(surface), 0.10)

    def test_bright_wallpaper_without_shadow_colors_keeps_current_dark_surface(self):
        palette = [(238, 202, 132)] * 256
        surface = watcher.choose_wallpaper_surface(palette, Counter({0: 1000}))
        self.assertLessEqual(watcher.relative_luminance(surface), 0.10)
        self.assertLess(
            sum(abs(surface[i] - watcher.TERMINAL_BASE_BACKGROUND[i]) for i in range(3)),
            28,
        )

    def test_theme_keeps_current_foreground_and_meets_wcag_on_dark_surface(self):
        palette = [(12, 4, 20)] * 256
        palette[1] = (20, 40, 55)
        palette[2] = (0, 220, 180)
        palette[3] = (100, 90, 180)
        counts = Counter({0: 80_000, 1: 3_000, 2: 800, 3: 2_000})
        theme = watcher.build_theme(palette, counts, sum(counts.values()))
        self.assertLess(
            sum(abs(watcher.hex_rgb(theme["foreground"])[i] - watcher.TERMINAL_BASE_FOREGROUND[i]) for i in range(3)),
            60,
        )
        self.assertGreaterEqual(float(theme["terminal_min_wcag"]), 4.5)
        self.assertGreaterEqual(float(theme["palette_min_wcag"]), 4.5)
        self.assertEqual(theme["palette_target_wcag"], "4.5")

    def test_vim_window_separators_use_the_waybar_outline_color(self):
        color_keys = (
            "background", "foreground", "accent", "secondary", "bright_accent", "muted",
            "cursorline", "statusline", "pumenu", "selection", "outline", "ansi_green",
            "ansi_yellow", "ansi_bright_red", "ansi_bright_yellow", "ansi_bright_green",
            "cursor", "search", "search_emphasis",
        )
        theme = {key: "#73bb63" for key in color_keys}
        script = watcher.render_vim(theme)
        self.assertIn("highlight VertSplit guifg=#73bb63", script)
        self.assertIn("highlight WinSeparator guifg=#73bb63", script)

    def test_hyprland_active_border_matches_waybar_outline_color_and_opacity(self):
        theme = {"outline": "#73bb63"}
        with patch.dict(watcher.os.environ, {"HYPRLAND_INSTANCE_SIGNATURE": "test"}), patch.object(
            watcher.subprocess, "run", return_value=SimpleNamespace(returncode=0, stdout="", stderr="")
        ) as run:
            watcher.update_hyprland_border(theme)

        self.assertIn(
            'general.col.active_border"] = "rgba(73bb639e)"',
            run.call_args.args[0][2],
        )

    def test_alacritty_runtime_update_applies_current_window_opacity(self):
        color_keys = (
            "background", "foreground", "cursor", "selection", "ansi_red", "ansi_green",
            "ansi_yellow", "secondary", "ansi_magenta", "accent", "ansi_white",
            "ansi_bright_black", "ansi_bright_red", "ansi_bright_green", "ansi_bright_yellow",
            "bright_secondary", "ansi_bright_magenta", "bright_accent", "ansi_bright_white",
        )
        theme = {key: "#73bb63" for key in color_keys}
        with tempfile.TemporaryDirectory() as directory:
            runtime = Path(directory)
            (runtime / "Alacritty-test.sock").touch()
            with patch.object(watcher, "RUNTIME_HOME", runtime), patch.object(
                watcher.subprocess, "run", return_value=SimpleNamespace(returncode=0, stdout="", stderr="")
            ) as run:
                watcher.update_alacritty_runtime(theme)

        self.assertIn("window.opacity=0.90", run.call_args.args[0])


class WaybarReloadPolicyTests(unittest.TestCase):
    def setUp(self):
        self.debouncer = watcher.WaybarReloadDebouncer(16.0)

    def test_scene_poll_uses_only_the_status_command(self):
        status = SimpleNamespace(stdout='{"ok":true,"data":{"scene":"scene-a"}}')
        with patch.object(watcher.subprocess, "run", return_value=status) as run:
            self.assertEqual(watcher.read_current_scene_id(), "scene-a")

        self.assertEqual(run.call_args.args[0], [str(watcher.WWCTL), "status"])

    def test_theme_refresh_requires_an_active_scene_id_change(self):
        self.assertFalse(watcher.scene_theme_refresh_due(None, "scene-a"))
        self.assertFalse(watcher.scene_theme_refresh_due("scene-a", "scene-a"))
        self.assertTrue(watcher.scene_theme_refresh_due("scene-a", "scene-b"))

    def test_scene_shift_reloads_changed_theme_immediately(self):
        self.assertTrue(
            watcher.waybar_reload_needed(
                self.debouncer,
                scene_shifted=True,
                palette_changed=True,
                now=100.0,
            )
        )
        self.assertFalse(self.debouncer.has_pending)

    def test_scene_shift_flushes_a_pending_palette_reload_even_without_new_css(self):
        self.assertTrue(self.debouncer.palette_changed(100.0))
        self.assertFalse(self.debouncer.palette_changed(108.0))

        self.assertTrue(
            watcher.waybar_reload_needed(
                self.debouncer,
                scene_shifted=True,
                palette_changed=False,
                now=109.0,
            )
        )
        self.assertFalse(self.debouncer.has_pending)
        self.assertFalse(self.debouncer.take_if_due(124.0))

    def test_unchanged_scene_without_css_or_pending_change_does_not_reload(self):
        self.assertFalse(
            watcher.waybar_reload_needed(
                self.debouncer,
                scene_shifted=False,
                palette_changed=False,
                now=100.0,
            )
        )


if __name__ == "__main__":
    unittest.main()
