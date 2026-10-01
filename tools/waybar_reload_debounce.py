"""Leading-edge reload with trailing coalescing for repeated Waybar changes."""

from __future__ import annotations

import math


class WaybarReloadDebouncer:
    """Reload the first palette change immediately; coalesce rapid repeats."""

    def __init__(self, delay_seconds: float) -> None:
        if not math.isfinite(delay_seconds) or delay_seconds <= 0:
            raise ValueError("Waybar reload debounce must be finite and positive")
        self.delay_seconds = delay_seconds
        self._last_change: float | None = None
        self._deadline: float | None = None

    @property
    def has_pending(self) -> bool:
        return self._deadline is not None

    def palette_changed(self, now: float) -> bool:
        """Return True for an immediate reload, False for a coalesced repeat."""
        if self._last_change is None or now - self._last_change >= self.delay_seconds:
            self._last_change = now
            self._deadline = None
            return True

        self._last_change = now
        self._deadline = now + self.delay_seconds
        return False

    def take_if_due(self, now: float) -> bool:
        if self._deadline is None or now < self._deadline:
            return False
        self.reset()
        return True

    def reset(self) -> None:
        """Cancel a pending coalesced reload and start a fresh change sequence."""
        self._last_change = None
        self._deadline = None
