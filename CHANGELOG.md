# Changelog

## [0.2.0] - 2026-09-14

### Added

- `Tuning` trait with `MtsTuning` (backed by `Client`) and `ScaleTuning` (standalone) implementations.

### Changed

- Pitch modulation is split into a `key_offset` applied along the key geometry (e.g. MPE pitch bend) and a `step_modulation` applied in scale steps.
- Tunings are looked up on demand instead of being cached, so there is no update step anymore.

### Removed

- `TuningMap`: use `MtsTuning` or `ScaleTuning` instead.

## [0.1.0] - 2026-09-01

- Initial release: safe `Client` bindings for the ODDSound MTS-ESP microtuning client, raw C API under `sys`, and `TuningMap` for scale-step indexed lookups.
