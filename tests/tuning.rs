//! Tests for the Tuning walk, using custom scales only (no Client).

use mts_client_rs::{ScaleTuning, Tuning};

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

/// `divisions` equal divisions of the octave, laid out from key 0 upwards with A4 = 440 Hz.
fn equal_division_scale(divisions: f64) -> ScaleTuning {
    ScaleTuning::from_frequencies(|key| Some(440.0 * ((key as f64 - 69.0) / divisions).exp2()))
        .with_period(12.0, divisions as u8)
}

/// Every `keys_per_step`th key mapped, the rest filtered out. 12-TET pitches throughout, so
/// only the keyboard map is sparse.
fn sparse_scale(keys_per_step: u8) -> ScaleTuning {
    ScaleTuning::from_frequencies(|key| {
        (key % keys_per_step == 0).then(|| twelve_tet_frequency_for_key(key))
    })
    .with_period(12.0, 12)
}

/// A scale that repeats every 3 keys with uneven steps, so the steps within a period differ.
fn uneven_octave_scale() -> ScaleTuning {
    ScaleTuning::from_frequencies(|key| {
        let (octave, degree) = (key / 3, key % 3);
        let ratio = [1.0, 1.25, 1.5][degree as usize];
        Some(8.0 * ratio * (octave as f64).exp2())
    })
    .with_period(12.0, 3)
}

/// A diatonic keyboard map: only the white keys are mapped, in plain 12-TET.
fn white_key_scale() -> ScaleTuning {
    ScaleTuning::from_frequencies(|key| {
        matches!(key % 12, 0 | 2 | 4 | 5 | 7 | 9 | 11).then(|| twelve_tet_frequency_for_key(key))
    })
    .with_period(12.0, 12)
}

/// A scale without a keyboard map: every second key repeats the pitch below it instead of being
/// filtered, which is how a master leaves a key out when it has no keyboard map.
fn repeated_pitch_scale() -> ScaleTuning {
    ScaleTuning::from_frequencies(|key| Some(twelve_tet_frequency_for_key(key - key % 2)))
        .with_period(12.0, 12)
}

// -------------------------------------------------------------------------------------------------

#[test]
fn plain_12_tet_is_untouched() {
    let scale = ScaleTuning::new();
    assert_eq!(scale.retuning_in_semitones(60, 0.0, 3.5), 0.0);
    assert!((scale.frequency(69, 0.0, 0.0) - 440.0).abs() < 1e-9);
    assert!((scale.frequency(69, 0.0, 12.0) - 880.0).abs() < 1e-9);
    assert!((scale.frequency(69, 12.0, 0.0) - 880.0).abs() < 1e-9);
    assert!((scale.frequency(69, 0.0, -0.5) - 440.0 * (-0.5f64 / 12.0).exp2()).abs() < 1e-9);
}

#[test]
fn whole_step_modulation_lands_on_the_next_scale_note() {
    let scale = equal_division_scale(24.0);
    // One step of 24-EDO is half a 12-TET semitone.
    assert!(
        (cents(scale.frequency(69, 0.0, 0.0), scale.frequency(69, 0.0, 1.0)) - 50.0).abs() < 1e-9
    );
    assert!((scale.frequency(69, 0.0, 1.0) - scale.frequency(70, 0.0, 0.0)).abs() < 1e-9);
    assert!((scale.frequency(69, 0.0, -3.0) - scale.frequency(66, 0.0, 0.0)).abs() < 1e-9);
    // 24 steps make the octave, not 12.
    assert!((scale.frequency(69, 0.0, 24.0) - 880.0).abs() < 1e-9);

    // The same holds when the *keyboard map* is what is sparse.
    let scale = sparse_scale(3);
    assert!((scale.frequency(60, 0.0, 1.0) - scale.frequency(63, 0.0, 0.0)).abs() < 1e-9);
    assert!((scale.frequency(60, 0.0, -2.0) - scale.frequency(54, 0.0, 0.0)).abs() < 1e-9);

    // A key offset of one key only covers a third of that step, which is the whole point of
    // having both units.
    assert!(cents(scale.frequency(60, 0.0, 1.0), scale.frequency(60, 1.0, 0.0)).abs() > 190.0);
}

#[test]
fn fractional_step_modulation_interpolates_evenly() {
    let scale = sparse_scale(3);
    let step = cents(scale.frequency(60, 0.0, 0.0), scale.frequency(60, 0.0, 1.0));
    assert!((step - 300.0).abs() < 1e-9);
    for (fraction, expected) in [(0.25, 75.0), (0.5, 150.0), (0.75, 225.0)] {
        let moved = cents(
            scale.frequency(60, 0.0, 0.0),
            scale.frequency(60, 0.0, fraction),
        );
        assert!(
            (moved - expected).abs() < 1e-9,
            "{fraction} -> {moved} cents"
        );
    }
}

#[test]
fn steps_are_symmetric_on_a_repeated_scale() {
    // A repeated pitch is not a note of its own, so a step down has to land on a note, just as a
    // step up does.
    let scale = repeated_pitch_scale();
    assert!((scale.frequency(4, 0.0, 1.0) - scale.frequency(6, 0.0, 0.0)).abs() < 1e-9);
    assert!((scale.frequency(4, 0.0, -1.0) - scale.frequency(2, 0.0, 0.0)).abs() < 1e-9);

    let center = scale.frequency(60, 0.0, 0.0);
    for depth in [0.5, 1.0, 2.0] {
        let up = cents(center, scale.frequency(60, 0.0, depth));
        let down = cents(scale.frequency(60, 0.0, -depth), center);
        assert!((up - down).abs() < 1e-9, "depth {depth}: {up} vs {down}");
    }
}

#[test]
fn retuning_matches_the_frequency() {
    for scale in [
        equal_division_scale(24.0),
        sparse_scale(3),
        ScaleTuning::new(),
    ] {
        for key in [0, 60, 127] {
            for (key_offset, steps) in [(0.0, 0.0), (0.0, 3.0), (2.5, 0.0), (-0.5, -5.0)] {
                let retuning = scale.retuning_in_semitones(key, key_offset, steps);
                let semitones = key as f64 + key_offset + steps + retuning;
                let expected =
                    twelve_tet_key_for_frequency(scale.frequency(key, key_offset, steps));
                assert!(
                    (semitones - expected).abs() < 1e-9,
                    "key {key} + {key_offset} + {steps}: {semitones} vs {expected}"
                );
            }
        }
    }
}

#[test]
fn key_offset_keeps_the_keyboard_geometry() {
    // Only every third key is mapped, but the pitches it keeps are plain 12-TET. Interpolating
    // across the gaps must therefore reproduce 12-TET everywhere, so a key width stays a key width.
    let scale = sparse_scale(3);
    for (key, offset) in [
        (0u8, 0.0),
        (60, 0.0),
        (61, 0.0),
        (61, 0.5),
        (62, 0.0),
        (72, 0.0),
        (126, 0.0),
    ] {
        let expected = 440.0 * ((key as f64 + offset - 69.0) / 12.0).exp2();
        let moved = cents(expected, scale.frequency(key, offset, 0.0));
        assert!(
            moved.abs() < 1e-9,
            "key {key} + {offset}: off by {moved} cents"
        );
    }
    // A physical octave stays an octave, which scale steps deliberately do not.
    let octave = cents(
        scale.frequency(60, 0.0, 0.0),
        scale.frequency(60, 12.0, 0.0),
    );
    assert!((octave - 1200.0).abs() < 1e-9);
    assert!(
        (cents(
            scale.frequency(60, 0.0, 0.0),
            scale.frequency(60, 0.0, 12.0)
        ) - 3600.0)
            .abs()
            < 1e-9
    );
}

#[test]
fn step_width_follows_the_key_offset_continuously() {
    // A step is one key wide from E to F and two keys wide from F to G, so a step taken at the
    // bent position must not jump as the offset sweeps over a mapped key.
    let scale = white_key_scale();
    let mut previous = scale.frequency(60, 0.0, 1.0);
    let mut largest_jump: f64 = 0.0;
    for step in 1..=240 {
        let next = scale.frequency(60, step as f64 * 0.05, 1.0);
        largest_jump = largest_jump.max(cents(previous, next).abs());
        previous = next;
    }
    assert!(
        largest_jump < 100.0,
        "step width jumped by {largest_jump} cents"
    );
}

#[test]
fn only_the_sum_of_key_and_offset_matters() {
    // Which of the two carries a whole semitone has to be a free choice at the call site.
    for scale in [
        white_key_scale(),
        repeated_pitch_scale(),
        ScaleTuning::new(),
    ] {
        let expected = scale.frequency(60, 1.5, 0.0);
        for (key, offset) in [(61u8, 0.5), (59, 2.5), (48, 13.5), (0, 61.5)] {
            let moved = cents(expected, scale.frequency(key, offset, 0.0));
            assert!(
                moved.abs() < 1e-9,
                "key {key} + {offset}: off by {moved} cents"
            );
        }
    }
}

#[test]
fn a_filtered_key_inside_the_map_is_a_gap() {
    // A master may filter a single key that is not part of the repeating pattern, e.g. to switch
    // tunings with it. It is skipped as a step, and bent across as a gap.
    let scale =
        ScaleTuning::from_frequencies(|key| (key != 61).then(|| twelve_tet_frequency_for_key(key)))
            .with_period(12.0, 12);
    assert!((scale.frequency(60, 0.0, 1.0) - scale.frequency(62, 0.0, 0.0)).abs() < 1e-9);
    let bent = cents(scale.frequency(60, 0.0, 0.0), scale.frequency(60, 1.0, 0.0));
    assert!(
        (bent - 100.0).abs() < 1e-9,
        "a key width moved {bent} cents"
    );
}

#[test]
fn key_offset_applies_before_step_modulation() {
    // The offset positions the voice within the map, and the steps are walked from there. With no
    // steps that is the plain slide: press C, slide to F, release, press F, the same pitch.
    let scale = white_key_scale();
    for (key, offset, steps) in [
        (60u8, 5.0, 0.0),
        (60, 5.0, 1.0),
        (60, 7.0, -2.0),
        (48, 12.0, 3.0),
    ] {
        let landed = (key as f64 + offset) as u8;
        let moved = cents(
            scale.frequency(landed, 0.0, steps),
            scale.frequency(key, offset, steps),
        );
        assert!(
            moved.abs() < 1e-9,
            "key {key} + {offset} + {steps} steps: off by {moved} cents"
        );
    }
}

#[test]
fn modulation_past_the_ends_keeps_moving() {
    let scale = equal_division_scale(24.0);
    // 24-EDO ends at key 127, well short of 127 steps above key 60.
    let top = scale.frequency(127, 0.0, 0.0);
    let above = scale.frequency(127, 0.0, 4.0);
    assert!(above > top, "pitch froze at the top of the map");
    assert!((cents(top, above) - 4.0 * 50.0).abs() < 1e-9);

    let bottom = scale.frequency(0, 0.0, 0.0);
    let below = scale.frequency(0, 0.0, -4.0);
    assert!(below < bottom, "pitch froze at the bottom of the map");
    assert!((cents(below, bottom) - 4.0 * 50.0).abs() < 1e-9);

    // A key offset past the end keeps moving too, and a key width stays a key width: in 24-EDO
    // laid out one degree per key, five keys is five degrees, not five semitones.
    assert!((cents(top, scale.frequency(127, 5.0, 0.0)) - 250.0).abs() < 1e-9);
}

#[test]
fn period_wraps_the_scale_past_the_ends() {
    // Three uneven steps to the octave, so wrapping by the period is not the same as repeating
    // the outermost interval.
    let scale = uneven_octave_scale();
    let top = scale.frequency(127, 0.0, 0.0);
    assert!((cents(top, scale.frequency(127, 0.0, 3.0)) - 1200.0).abs() < 1e-9);
    let bottom = scale.frequency(0, 0.0, 0.0);
    assert!((cents(scale.frequency(0, 0.0, -3.0), bottom) - 1200.0).abs() < 1e-9);
    // Several periods out keeps working.
    assert!((cents(top, scale.frequency(127, 0.0, 9.0)) - 3600.0).abs() < 1e-9);
}

#[test]
fn degenerate_scales_stay_sane() {
    // Nothing mapped: no scale at all, so fall back to 12-TET rather than misbehave.
    let scale = ScaleTuning::from_frequencies(|_| None);
    assert!((scale.frequency(69, 0.0, 0.0) - 440.0).abs() < 1e-9);
    assert!((scale.frequency(69, 0.0, 1.0) - twelve_tet_frequency_for_key(70)).abs() < 1e-9);
    assert_eq!(scale.retuning_in_semitones(69, 0.5, 0.0), 0.0);

    // A single mapped note has no interval of its own: extend it in plain semitones.
    let scale =
        ScaleTuning::from_frequencies(|key| (key == 69).then(|| twelve_tet_frequency_for_key(key)));
    assert!((scale.frequency(69, 0.0, 0.0) - 440.0).abs() < 1e-9);
    let extended = cents(scale.frequency(69, 0.0, 0.0), scale.frequency(69, 2.0, 0.0));
    assert!((extended - 200.0).abs() < 1e-9);
}

#[test]
fn an_unmodulated_key_sounds_at_its_table_pitch() {
    // A master without a keyboard map leaves a key out by repeating the pitch below it. An
    // unmodulated note has nothing to interpolate, so it must sound at that repeated pitch.
    let scale = repeated_pitch_scale();
    for key in 0..=127u8 {
        let expected = twelve_tet_frequency_for_key(key - key % 2);
        let moved = cents(expected, scale.frequency(key, 0.0, 0.0));
        assert!(moved.abs() < 1e-9, "key {key}: off by {moved} cents");
    }
}
