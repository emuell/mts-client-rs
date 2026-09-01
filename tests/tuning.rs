//! Tests for the TuningMap, using custom maps only (no Client).

use mts_client_rs::TuningMap;

// -------------------------------------------------------------------------------------------------

fn cents(from: f64, to: f64) -> f64 {
    1200.0 * (to / from).log2()
}

fn twelve_tet_frequency_for_key(key: u8) -> f64 {
    440.0 * ((key as f64 - 69.0) / 12.0).exp2()
}

fn twelve_tet_key_for_frequency(frequency: f64) -> f64 {
    69.0 + 12.0 * (frequency / 440.0).log2()
}

/// A map built from a synthetic scale, so the tests need neither `libMTS` nor a master.
fn map_from(key_frequency: impl FnMut(u8) -> Option<f64>) -> TuningMap {
    let mut map = TuningMap::new();
    map.update_with(key_frequency);
    map
}

/// `divisions` equal divisions of the octave, laid out from key 0 upwards with A4 = 440 Hz.
fn equal_division_map(divisions: f64) -> TuningMap {
    map_from(|key| Some(440.0 * ((key as f64 - 69.0) / divisions).exp2()))
}

/// Every `keys_per_step`th key mapped, the rest filtered out. 12-TET pitches throughout, so
/// only the keyboard map is sparse.
fn sparse_map(keys_per_step: u8) -> TuningMap {
    map_from(|key| (key % keys_per_step == 0).then(|| twelve_tet_frequency_for_key(key)))
}

// -------------------------------------------------------------------------------------------------

#[test]
fn plain_12_tet_is_inactive() {
    let map = TuningMap::new();
    assert!(!map.is_active());
    assert_eq!(map.step_count(), 128);
    assert_eq!(map.step_distance(60, 67), 7);
    assert_eq!(map.retuning_in_semitones(60, 3.5), 0.0);
    // An inactive map returns 12-TET tunings.
    assert!((map.frequency(69, 0.0) - 440.0).abs() < 1e-9);
    assert!((map.frequency(69, 12.0) - 880.0).abs() < 1e-9);
    assert!((map.frequency(69, -0.5) - 440.0 * (-0.5f64 / 12.0).exp2()).abs() < 1e-9);
}

#[test]
fn a_retuned_scale_is_active() {
    let map = equal_division_map(24.0);
    assert!(map.is_active());
    assert_eq!(map.step_count(), 128);
    // A sparse keyboard map is active too, even though every pitch it keeps is plain 12-TET.
    let map = sparse_map(2);
    assert!(map.is_active());
    assert_eq!(map.step_count(), 64);
}

#[test]
fn whole_modulation_lands_on_the_next_scale_step() {
    let map = equal_division_map(24.0);
    // One step of 24-EDO is half a 12-TET semitone.
    assert!((cents(map.frequency(69, 0.0), map.frequency(69, 1.0)) - 50.0).abs() < 1e-9);
    assert!((map.frequency(69, 1.0) - map.frequency(70, 0.0)).abs() < 1e-9);
    assert!((map.frequency(69, -3.0) - map.frequency(66, 0.0)).abs() < 1e-9);
    // 24 steps make the octave, not 12.
    assert!((map.frequency(69, 24.0) - 880.0).abs() < 1e-9);
    // The same holds when the *keyboard map* is what is sparse.
    let map = sparse_map(3);
    assert_eq!(map.step_count(), 43);
    assert!((map.frequency(60, 1.0) - map.frequency(63, 0.0)).abs() < 1e-9);
    assert!((map.frequency(60, -2.0) - map.frequency(54, 0.0)).abs() < 1e-9);
}

#[test]
fn fractional_modulation_interpolates_evenly_in_cents() {
    let map = sparse_map(3);
    let step = cents(map.frequency(60, 0.0), map.frequency(60, 1.0));
    assert!((step - 300.0).abs() < 1e-9);
    for (fraction, expected) in [(0.25, 75.0), (0.5, 150.0), (0.75, 225.0)] {
        let moved = cents(map.frequency(60, 0.0), map.frequency(60, fraction));
        assert!(
            (moved - expected).abs() < 1e-9,
            "{fraction} -> {moved} cents"
        );
    }
}

#[test]
fn modulation_is_symmetric_around_a_note() {
    let map = equal_division_map(19.0);
    let center = map.frequency(60, 0.0);
    for depth in [0.1, 0.5, 1.0, 2.5, 7.0] {
        let up = cents(center, map.frequency(60, depth));
        let down = cents(map.frequency(60, -depth), center);
        assert!((up - down).abs() < 1e-9, "depth {depth}: {up} vs {down}");
    }
}

#[test]
fn retuning_in_semitones_matches_the_frequency() {
    // Across the whole MIDI range, and on an inactive map as well as an active one.
    for map in [equal_division_map(24.0), TuningMap::new()] {
        for key in [0, 60, 127] {
            for modulation in [-5.0, -0.5, 0.0, 0.25, 3.0] {
                let retuning = map.retuning_in_semitones(key, modulation);
                let semitones = key as f64 + modulation + retuning;
                let expected = twelve_tet_key_for_frequency(map.frequency(key, modulation));
                assert!(
                    (semitones - expected).abs() < 1e-9,
                    "key {key} + {modulation}: {semitones} vs {expected}"
                );
            }
        }
    }
}

#[test]
fn step_distance_covers_a_glide() {
    let map = sparse_map(3);
    assert_eq!(map.step_distance(60, 69), 3);
    assert_eq!(map.step_distance(69, 60), -3);
    assert_eq!(map.step_distance(60, 60), 0);
    // A glide from 60 to 69 has to move `step_distance` of modulation, not 9 semitones.
    assert!((map.frequency(69, -3.0) - map.frequency(60, 0.0)).abs() < 1e-9);
}

#[test]
fn unmapped_keys_resolve_to_the_step_below() {
    let map = sparse_map(3);
    assert!(map.should_filter_note(61));
    assert!(!map.should_filter_note(60));
    assert_eq!(map.step_distance(60, 61), 0);
    assert_eq!(map.step_distance(60, 62), 0);
    assert_eq!(map.step_distance(60, 63), 1);

    // Below the start of the map there is no step to resolve down to, so the first one answers.
    let map = map_from(|key| (key >= 5).then(|| twelve_tet_frequency_for_key(key)));
    assert_eq!(map.step_distance(0, 5), 0);
    assert_eq!(map.step_distance(4, 5), 0);
    assert_eq!(map.step_distance(5, 6), 1);
}

#[test]
fn repeated_frequencies_are_not_separate_steps() {
    // A master without a keyboard map repeats a frequency instead of filtering the key.
    let map = map_from(|key| Some(twelve_tet_frequency_for_key(key - key % 2)));
    assert!(map.is_active());
    assert_eq!(map.step_count(), 64);
    assert!((map.frequency(60, 1.0) - map.frequency(62, 0.0)).abs() < 1e-9);
}

#[test]
fn modulation_past_the_ends_keeps_moving() {
    let map = equal_division_map(24.0);
    // 24-EDO ends at key 127, well short of 127 steps above key 60.
    let top = map.frequency(127, 0.0);
    let above = map.frequency(127, 4.0);
    assert!(above > top, "pitch froze at the top of the map");
    assert!((cents(top, above) - 4.0 * 50.0).abs() < 1e-9);

    let bottom = map.frequency(0, 0.0);
    let below = map.frequency(0, -4.0);
    assert!(below < bottom, "pitch froze at the bottom of the map");
    assert!((cents(below, bottom) - 4.0 * 50.0).abs() < 1e-9);
}

#[test]
fn degenerate_maps_stay_sane() {
    // Everything filtered: no scale at all, so fall back to 12-TET rather than misbehave.
    let map = map_from(|_| None);
    assert_eq!(map.step_count(), 0);
    assert!((map.frequency(69, 0.0) - 440.0).abs() < 1e-9);
    assert_eq!(map.retuning_in_semitones(69, 1.0), 0.0);

    // A single mapped note has no interval of its own: extend it in plain semitones.
    let map = map_from(|key| (key == 69).then(|| twelve_tet_frequency_for_key(key)));
    assert_eq!(map.step_count(), 1);
    assert!((map.frequency(69, 0.0) - 440.0).abs() < 1e-9);
    assert!((cents(map.frequency(69, 0.0), map.frequency(69, 2.0)) - 200.0).abs() < 1e-9);
}

#[test]
fn rebuilding_replaces_the_previous_scale() {
    let mut map = TuningMap::new();
    map.update_with(|key| Some(440.0 * ((key as f64 - 69.0) / 24.0).exp2()));
    assert!(map.is_active());

    // A shorter map must not leave the longer one's steps behind.
    map.update_with(|key| (key % 3 == 0).then(|| twelve_tet_frequency_for_key(key)));
    assert_eq!(map.step_count(), 43);
    assert!((map.frequency(60, 1.0) - twelve_tet_frequency_for_key(63)).abs() < 1e-9);

    map.update_with(|key| Some(twelve_tet_frequency_for_key(key)));
    assert!(!map.is_active());
    assert_eq!(map.step_count(), 128);
}
