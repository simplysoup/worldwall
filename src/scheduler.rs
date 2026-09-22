//! Fake-clock-friendly callback plus monotonic-deadline pacing.
//!
//! A compositor callback is a readiness signal, not an FPS limiter.  After a
//! frame, the next deadline is based on the actual submission time.  A delayed
//! wake schedules one frame only; it never replays missed intervals.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fps {
    Fifteen,
    Thirty,
}

impl Fps {
    pub const fn interval_nanos(self) -> u64 {
        match self {
            Self::Fifteen => 66_666_667,
            Self::Thirty => 33_333_333,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameScheduler {
    fps: Fps,
    callback_ready: bool,
    bootstrap_pending: bool,
    next_deadline_ns: Option<u64>,
    submitted_frames: u64,
    paused: bool,
}

impl FrameScheduler {
    pub fn new(fps: Fps) -> Self {
        Self {
            fps,
            callback_ready: false,
            bootstrap_pending: true,
            next_deadline_ns: None,
            submitted_frames: 0,
            paused: false,
        }
    }

    #[cfg(test)]
    pub fn set_fps(&mut self, fps: Fps, now_ns: u64) {
        self.fps = fps;
        if let Some(deadline) = self.next_deadline_ns {
            self.next_deadline_ns = Some(deadline.min(now_ns.saturating_add(fps.interval_nanos())));
        }
    }

    pub fn on_callback(&mut self) {
        if !self.paused {
            self.callback_ready = true;
        }
    }

    /// Stop deadline and callback-driven submissions while the session is
    /// asleep. Resume starts a fresh bootstrap frame instead of replaying
    /// deadlines accumulated during sleep.
    pub fn pause(&mut self) {
        self.paused = true;
        self.callback_ready = false;
        self.bootstrap_pending = false;
        self.next_deadline_ns = None;
    }

    pub fn resume(&mut self) {
        self.paused = false;
        self.callback_ready = false;
        self.bootstrap_pending = true;
        self.next_deadline_ns = None;
    }

    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Reconfiguration needs one valid frame even if the compositor has not
    /// delivered the callback belonging to the previous extent yet.
    pub fn on_reconfigure(&mut self) {
        self.bootstrap_pending = true;
    }

    #[cfg(test)]
    pub fn deadline_ns(&self) -> Option<u64> {
        self.next_deadline_ns
    }

    #[cfg(test)]
    pub fn submitted_frames(&self) -> u64 {
        self.submitted_frames
    }

    #[cfg(test)]
    pub fn callback_ready(&self) -> bool {
        self.callback_ready
    }

    pub fn is_due(&self, now_ns: u64) -> bool {
        !self.paused
            && (self.bootstrap_pending
                || (self.callback_ready
                    && self
                        .next_deadline_ns
                        .is_some_and(|deadline| now_ns >= deadline)))
    }

    /// Mark one submission.  Returns false if the callback/deadline gate was
    /// not satisfied, making accidental catch-up submissions test-visible.
    pub fn submit_if_due(&mut self, now_ns: u64) -> bool {
        if self.paused || !self.is_due(now_ns) {
            return false;
        }
        self.bootstrap_pending = false;
        self.callback_ready = false;
        self.next_deadline_ns = Some(now_ns.saturating_add(self.fps.interval_nanos()));
        self.submitted_frames = self.submitted_frames.saturating_add(1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_does_not_wait_for_a_callback() {
        let mut scheduler = FrameScheduler::new(Fps::Thirty);
        assert!(scheduler.submit_if_due(0));
        assert_eq!(scheduler.submitted_frames(), 1);
        assert!(!scheduler.callback_ready());
    }

    #[test]
    fn callback_before_deadline_is_not_enough() {
        let mut scheduler = FrameScheduler::new(Fps::Thirty);
        assert!(scheduler.submit_if_due(0));
        scheduler.on_callback();
        assert!(!scheduler.submit_if_due(1));
        assert!(!scheduler.submit_if_due(33_333_332));
        assert!(scheduler.submit_if_due(33_333_333));
    }

    #[test]
    fn withheld_callback_blocks_deadline_without_spin_or_submission() {
        let mut scheduler = FrameScheduler::new(Fps::Thirty);
        assert!(scheduler.submit_if_due(100));
        let deadline = scheduler.deadline_ns().unwrap();
        assert!(!scheduler.is_due(deadline));
        assert!(!scheduler.submit_if_due(deadline + 10_000_000));
        assert_eq!(scheduler.submitted_frames(), 1);
    }

    #[test]
    fn delayed_callback_produces_one_frame_without_catch_up_burst() {
        let mut scheduler = FrameScheduler::new(Fps::Thirty);
        assert!(scheduler.submit_if_due(0));
        scheduler.on_callback();
        assert!(scheduler.submit_if_due(500_000_000));
        assert_eq!(scheduler.submitted_frames(), 2);
        assert_eq!(scheduler.deadline_ns(), Some(533_333_333));
        assert!(!scheduler.submit_if_due(500_000_001));
        assert_eq!(scheduler.submitted_frames(), 2);
    }

    #[test]
    fn fifteen_fps_interval_is_used_after_reconfiguration() {
        let mut scheduler = FrameScheduler::new(Fps::Thirty);
        assert!(scheduler.submit_if_due(0));
        scheduler.set_fps(Fps::Fifteen, 1);
        scheduler.on_callback();
        assert!(scheduler.submit_if_due(66_666_667));
        assert_eq!(scheduler.deadline_ns(), Some(133_333_334));
    }

    #[test]
    fn resize_has_a_bootstrap_path() {
        let mut scheduler = FrameScheduler::new(Fps::Thirty);
        assert!(scheduler.submit_if_due(0));
        scheduler.on_reconfigure();
        assert!(scheduler.submit_if_due(1));
        assert_eq!(scheduler.submitted_frames(), 2);
    }

    #[test]
    fn pause_discards_sleep_deadline_and_resume_bootstraps_once() {
        let mut scheduler = FrameScheduler::new(Fps::Thirty);
        assert!(scheduler.submit_if_due(0));
        scheduler.on_callback();
        scheduler.pause();
        assert!(scheduler.is_paused());
        assert!(!scheduler.is_due(u64::MAX));
        assert!(!scheduler.submit_if_due(u64::MAX));
        scheduler.resume();
        assert!(!scheduler.is_paused());
        assert!(scheduler.submit_if_due(1));
        assert_eq!(scheduler.submitted_frames(), 2);
    }
}
