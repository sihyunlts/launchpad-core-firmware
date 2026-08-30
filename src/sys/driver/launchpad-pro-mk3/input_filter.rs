// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

pub(super) const PAD_COUNT: usize = 64;
pub(super) const PAD_PRESS_THRESHOLD: u16 = 0x0081;
pub(super) const PAD_RELEASE_THRESHOLD: u16 = 0x0008;
const PAD_RELEASE_CONFIRM_FRAMES: u8 = 3;

pub(super) struct M0InputFilter {
    last_frame_counter: Option<u16>,
    pad_release_count: [u8; PAD_COUNT],
}

impl M0InputFilter {
    pub(super) const fn new() -> Self {
        Self {
            last_frame_counter: None,
            pad_release_count: [0; PAD_COUNT],
        }
    }

    pub(super) fn reset_stream(&mut self) {
        self.last_frame_counter = None;
        self.pad_release_count.fill(0);
    }

    pub(super) fn accept_frame(&mut self, frame_counter: u16) -> bool {
        let Some(previous) = self.last_frame_counter else {
            self.last_frame_counter = Some(frame_counter);
            return true;
        };

        let delta = frame_counter.wrapping_sub(previous);
        if delta == 0 || delta >= 0x8000 {
            return false;
        }

        self.last_frame_counter = Some(frame_counter);
        true
    }

    pub(super) fn pad_state(&mut self, index: usize, was_pressed: bool, raw: u16) -> bool {
        if !was_pressed {
            self.pad_release_count[index] = 0;
            return raw >= PAD_PRESS_THRESHOLD;
        }

        if raw > PAD_RELEASE_THRESHOLD {
            self.pad_release_count[index] = 0;
            return true;
        }

        let count = self.pad_release_count[index].saturating_add(1);
        if count >= PAD_RELEASE_CONFIRM_FRAMES {
            self.pad_release_count[index] = 0;
            false
        } else {
            self.pad_release_count[index] = count;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_fresh_forward_counters() {
        let mut filter = M0InputFilter::new();

        assert!(filter.accept_frame(41));
        assert!(!filter.accept_frame(41));
        assert!(!filter.accept_frame(40));
        assert!(filter.accept_frame(42));
        assert!(!filter.accept_frame(42u16.wrapping_add(0x8000)));
    }

    #[test]
    fn accepts_counter_wrap_and_reset() {
        let mut filter = M0InputFilter::new();

        assert!(filter.accept_frame(0xffff));
        assert!(filter.accept_frame(0));
        assert!(filter.accept_frame(3));
        filter.reset_stream();
        assert!(filter.accept_frame(0));
    }

    #[test]
    fn presses_immediately_and_releases_after_three_low_frames() {
        let mut filter = M0InputFilter::new();
        let mut pressed = false;

        pressed = filter.pad_state(0, pressed, PAD_PRESS_THRESHOLD);
        assert!(pressed);
        pressed = filter.pad_state(0, pressed, PAD_RELEASE_THRESHOLD);
        assert!(pressed);
        pressed = filter.pad_state(0, pressed, PAD_RELEASE_THRESHOLD);
        assert!(pressed);
        pressed = filter.pad_state(0, pressed, PAD_RELEASE_THRESHOLD);
        assert!(!pressed);
        assert!(!filter.pad_state(0, pressed, PAD_RELEASE_THRESHOLD));
    }

    #[test]
    fn hysteresis_sample_resets_release_confirmation() {
        let mut filter = M0InputFilter::new();
        let mut pressed = filter.pad_state(7, false, PAD_PRESS_THRESHOLD);

        pressed = filter.pad_state(7, pressed, PAD_RELEASE_THRESHOLD);
        assert!(pressed);
        pressed = filter.pad_state(7, pressed, PAD_RELEASE_THRESHOLD + 1);
        assert!(pressed);
        pressed = filter.pad_state(7, pressed, PAD_RELEASE_THRESHOLD);
        assert!(pressed);
        pressed = filter.pad_state(7, pressed, PAD_RELEASE_THRESHOLD);
        assert!(pressed);
        pressed = filter.pad_state(7, pressed, PAD_RELEASE_THRESHOLD);
        assert!(!pressed);
    }

    #[test]
    fn duplicate_frames_do_not_advance_release_confirmation() {
        let mut filter = M0InputFilter::new();
        let mut pressed = filter.pad_state(12, false, PAD_PRESS_THRESHOLD);

        for counter in [10, 10, 11, 11] {
            if filter.accept_frame(counter) {
                pressed = filter.pad_state(12, pressed, PAD_RELEASE_THRESHOLD);
            }
            assert!(pressed);
        }

        assert!(filter.accept_frame(12));
        pressed = filter.pad_state(12, pressed, PAD_RELEASE_THRESHOLD);
        assert!(!pressed);
    }

    #[test]
    fn stream_reset_discards_partial_release() {
        let mut filter = M0InputFilter::new();
        let mut pressed = filter.pad_state(63, false, PAD_PRESS_THRESHOLD);

        pressed = filter.pad_state(63, pressed, PAD_RELEASE_THRESHOLD);
        assert!(pressed);
        filter.reset_stream();
        pressed = filter.pad_state(63, pressed, PAD_RELEASE_THRESHOLD);
        assert!(pressed);
        pressed = filter.pad_state(63, pressed, PAD_RELEASE_THRESHOLD);
        assert!(pressed);
        pressed = filter.pad_state(63, pressed, PAD_RELEASE_THRESHOLD);
        assert!(!pressed);
    }
}
