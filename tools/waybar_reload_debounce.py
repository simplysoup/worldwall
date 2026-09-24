"""Small monotonic trailing-edge debounce for Waybar palette reloads."""

from __future__ import annotations

import math


class WaybarReloadDebouncer:
    """Coalesce repeated palette changes into one reload after a quiet period."""

    def __init__(self, delay_seconds: float) -> None:
        if not math.isfinite(delay_seconds) or delay_seconds <= 0:
            raise ValueError("Waybar reload debounce must be finite and positive")
        self.delay_seconds = delay_seconds
        self._deadline: float | None = None

    def palette_changed(self, now: float) -> None:
        self._deadline = now + self.delay_seconds

    def take_if_due(self, now: float) -> bool:
        if self._deadline is None or now < self._deadline:
            return False
        self._deadline = None
        return True
