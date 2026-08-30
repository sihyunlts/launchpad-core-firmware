#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 ZephyrCodesStuff

meta:
  id: boot_animation
  title: Launchpad Firmware Boot Animation Binary
  file-extension: bin
  endian: le
  license: GPL-3.0-only

seq:
  - id: header
    type: header
  - id: frames
    type: boot_frame
    repeat: expr
    repeat-expr: header.num_frames
  - id: changes
    type: boot_change
    repeat: expr
    repeat-expr: header.num_changes

types:
  header:
    seq:
      - id: end_tick
        type: u2
        doc: Animation duration in 100Hz ticks
      - id: num_frames
        type: u2
        doc: Number of frame keypoints
      - id: num_changes
        type: u4
        doc: Total LED state change events across all frames

  boot_frame:
    seq:
      - id: tick
        type: u2
        doc: Timestamp in 100Hz ticks when this frame triggers
      - id: count
        type: u1
        doc: Number of LED change events in this frame
      - id: pad
        type: u1
        doc: Alignment padding byte

  boot_change:
    seq:
      - id: led
        type: u1
        doc: Novation physical LED index (0..99)
      - id: velocity
        type: u1
        doc: MIDI velocity / color palette index (0..127)
