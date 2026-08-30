// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

pub const BUTTON_REMAP_SIZE: usize = 128;
pub const BUTTON_REMAP: [u8; BUTTON_REMAP_SIZE] = [
    11, 21, 31, 41, 51, 61, 71, 81, 12, 22, 32, 42, 52, 62, 72, 82, 13, 23, 33, 43, 53, 63, 73, 83,
    14, 24, 34, 44, 54, 64, 74, 84, 15, 25, 35, 45, 55, 65, 75, 85, 16, 26, 36, 46, 56, 66, 76, 86,
    17, 27, 37, 47, 57, 67, 77, 87, 18, 28, 38, 48, 58, 68, 78, 88, 91, 92, 80, 70, 60, 50, 40, 30,
    20, 10, 101, 1, 0, 0, 0, 90, 93, 94, 0, 0, 0, 0, 102, 2, 103, 3, 104, 4, 0, 0, 0, 0, 95, 96, 0,
    0, 0, 0, 105, 5, 106, 6, 107, 7, 0, 0, 0, 0, 97, 98, 89, 79, 69, 59, 49, 39, 29, 19, 108, 8, 0,
    0, 0, 0,
];

pub const LED_REMAP_SIZE: usize = 110;
pub const LED_REMAP: [u8; LED_REMAP_SIZE] = [
    53, 26, 50, 51, 52, 77, 78, 79, 107, 0xFF, 23, 24, 44, 45, 46, 71, 72, 73, 104, 105, 21, 22,
    41, 42, 43, 68, 69, 70, 102, 103, 18, 19, 20, 39, 40, 66, 67, 99, 100, 101, 15, 16, 17, 37, 38,
    64, 65, 96, 97, 98, 12, 13, 14, 35, 36, 62, 63, 93, 94, 95, 9, 10, 11, 33, 34, 60, 61, 90, 91,
    92, 6, 7, 8, 31, 32, 58, 59, 87, 88, 89, 3, 4, 5, 29, 30, 56, 57, 84, 85, 86, 0, 1, 2, 27, 28,
    54, 55, 81, 82, 83, 0xFF, 25, 47, 48, 49, 74, 75, 76, 106, 0xFF,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_exposed_physical_control_has_one_unique_logical_index() {
        let mut seen = [false; 109];
        for (control, &mapped) in BUTTON_REMAP.iter().enumerate() {
            let logical = if mapped != 0 {
                Some(mapped as usize)
            } else if control == 78 {
                Some(0)
            } else {
                None
            };

            if let Some(logical) = logical {
                assert!(logical < seen.len());
                assert!(!seen[logical], "logical index {logical} is mapped twice");
                seen[logical] = true;
            }
        }

        for (logical, mapped) in seen.into_iter().enumerate() {
            // XY index 9 is outside the 8-column surface. Indexes 99 and 100
            // are likewise sentinels between the regular and mirror ranges.
            let expected = logical != 9 && logical != 99 && logical != 100;
            assert_eq!(
                mapped, expected,
                "unexpected mapping for logical index {logical}"
            );
        }
    }
}
