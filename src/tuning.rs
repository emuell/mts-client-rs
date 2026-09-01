//! A scale-step view of a master tuning, for clients that want to modulate pitch with tuning.

use crate::Client;

// -------------------------------------------------------------------------------------------------

/// Number of MIDI keys and thus the largest possible number of scale steps.
const KEY_COUNT: usize = 128;

/// A key counts as retuned when it is off 12-TET by more than this. Must be far below hearing.
const RETUNED_EPSILON_SEMITONES: f64 = 1e-6;

/// One semitone in `log2` frequency units.
const SEMITONE_IN_LOG2: f64 = 1.0 / 12.0;

// -------------------------------------------------------------------------------------------------

/// A snapshot of a master's keyboard map and tuning, indexed by scale steps (semitones in standard
/// tuning) rather than MIDI keys.
///
/// [`Client::retuning_in_semitones`] re-tunes a *note*, but synths add pitch modulation on top of
/// it: glide, pitch bend, an LFO on oscillator pitch. Added as plain semitones that modulation
/// moves through 12-TET, while the note it starts from sits in the master's scale.
///
/// With MPE this gets worse, because pitch-bend is how a voice reaches other notes, and bending
/// in 12-TET never arrives at that note's retuned pitch.
///
/// In the `TuningMap` one unit of modulation is one step of the master's scale, and fractions
/// of steps interpolate between neighbouring mapped steps.
///
/// Create one with [`TuningMap::new`] and refresh it with [`TuningMap::update`], then keep it for
/// the lifetime of your voice pool. The update and every query are **real-time safe**, so the map
/// can live entirely on the audio thread.
///
/// Note that a map describes a single MIDI channel. Masters using multi-channel tuning tables
/// need one map per channel; pass `None` for the channel-agnostic tuning that most clients want.
///
/// Every query takes a MIDI key in range `0..=127`. A key beyond that is a caller bug: it panics
/// in debug builds and is clamped to the last key in release ones.
///
/// # Example
///
/// ```no_run
/// use mts_client_rs::{Client, TuningMap};
///
/// # let client = Client::new().unwrap();
/// let mut tuning = TuningMap::new();
///
/// // Once per audio block, so active voices follow automated tunings.
/// tuning.update(&client, None);
///
/// // Per voice: `modulation` is the voice's total pitch offset from its key.
/// let key = 60;
/// let modulation = 2.0; // e.g. a pitch bend two scale steps (semitones in 12-TET) up
/// if !tuning.should_filter_note(key) {
///     let frequency = tuning.frequency(key, modulation);
/// }
/// ```
#[derive(Clone)]
pub struct TuningMap {
    step_log2: [f64; KEY_COUNT],
    step_of_key: [u8; KEY_COUNT],
    filtered: [bool; KEY_COUNT],
    step_count: usize,
    active: bool,
}

impl Default for TuningMap {
    fn default() -> Self {
        Self::new()
    }
}

impl TuningMap {
    /// Creates a new map of plain 12-TET with every key mapped. Call [`TuningMap::update`] to
    /// apply a tuning from an MTS master.
    pub fn new() -> Self {
        let mut map = Self {
            step_log2: [0.0; KEY_COUNT],
            step_of_key: [0; KEY_COUNT],
            filtered: [false; KEY_COUNT],
            step_count: 0,
            active: false,
        };
        map.fill(|key| Some(twelve_tet_frequency_for_key(key)));
        map
    }

    /// Re-read the master's tuning and update the map accordingly.
    ///
    /// **Real-time safe**: this only uses [`Client::should_filter_note`] and
    /// [`Client::note_to_frequency`] which are both lock-free reads.
    ///
    /// A master can retune at any time and gives no notification, so call this periodically
    /// (once per audio block, or on whatever slower control tick the client already has).
    pub fn update(&mut self, client: &Client, channel: Option<u8>) {
        self.fill(|key| {
            (!client.should_filter_note(key, channel))
                .then(|| client.note_to_frequency(key, channel))
        })
    }

    /// Rebuild the map from a custom tuning instead of a MTS master.
    ///
    /// `key_frequency` should return the key's frequency in Hz, or `None` when the key is
    /// unmapped, for every key in range 0..128.
    ///
    /// This can be useful for clients whose tuning does not come from an MTS-ESP master,
    /// e.g. a Scala file, or a tuning the host supplies.
    pub fn update_with(&mut self, key_frequency: impl FnMut(u8) -> Option<f64>) {
        self.fill(key_frequency)
    }

    /// False when the map is plain 12-TET with every key mapped to a pitch of its own, in order
    /// to simplify processing.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// How many notes the keyboard map holds. 128 for a map without gaps.
    #[inline]
    pub fn step_count(&self) -> usize {
        self.step_count
    }

    /// True when the map leaves the given `key` unmapped, in which case it should not start a
    /// voice. Equivalent of [`Client::should_filter_note`].
    #[inline]
    pub fn should_filter_note(&self, key: u8) -> bool {
        debug_assert!(key < 128, "MIDI key {key} is out of range 0..=127");
        self.filtered[key_index(key)]
    }

    /// The tuned frequency `modulation` scale steps away from `key`, in Hz.
    ///
    /// A whole `modulation` lands exactly on another note of the scale. A fraction will get
    /// interpolated geometrically between its neighbours, so a unit of modulation is a constant
    /// number of cents within a step.
    ///
    /// Past the ends of the map the outermost step interval gets repeated, so pitch modulation
    /// can keep moving past the end instead of freezing against the last mapped note.
    #[inline]
    pub fn frequency(&self, key: u8, modulation: f64) -> f64 {
        debug_assert!(key < 128, "MIDI key {key} is out of range 0..=127");
        if !self.active || self.step_count == 0 {
            return twelve_tet_frequency_for_fractional_key(key as f64 + modulation);
        }
        let position = self.step_of_key[key_index(key)] as f64 + modulation;
        let index = position.floor();
        let fraction = position - index;
        let index = index as i32;
        let lower = self.step_log2_at(index);
        let upper = self.step_log2_at(index + 1);
        (lower + (upper - lower) * fraction).exp2()
    }

    /// The offset from 12-TET in semitones, of [`TuningMap::frequency`]: what to add to
    /// `key + modulation` to reach the final tuned pitch. Zero, when the map is inactive.
    ///
    /// See also [`TuningMap::frequency`] which can be useful when e.g. driving an
    /// oscillator to initialize it with an absolute frequency.
    #[inline]
    pub fn retuning_in_semitones(&self, key: u8, modulation: f64) -> f64 {
        debug_assert!(key < 128, "MIDI key {key} is out of range 0..=127");
        if !self.active || self.step_count == 0 {
            return 0.0;
        }
        twelve_tet_key_for_frequency(self.frequency(key, modulation)) - (key as f64 + modulation)
    }

    /// How many scale steps apart two keys are. Negative when `to` is below `from`.
    ///
    /// This is the modulation distance for a glide between the two keys: the plain key
    /// distance would traverse the scale at the wrong rate, and end on the wrong note.
    #[inline]
    pub fn step_distance(&self, from: u8, to: u8) -> i32 {
        debug_assert!(from < 128, "MIDI key {from} is out of range 0..=127");
        debug_assert!(to < 128, "MIDI key {to} is out of range 0..=127");
        let from = self.step_of_key[key_index(from)] as i32;
        let to = self.step_of_key[key_index(to)] as i32;
        to - from
    }

    #[inline]
    fn step_log2_at(&self, index: i32) -> f64 {
        debug_assert!(self.step_count > 0);
        // Past either end, repeat the outermost interval rather than clamping the pitch.
        let last = self.step_count as i32 - 1;
        if index < 0 {
            self.step_log2[0] + index as f64 * self.step_interval(0)
        } else if index > last {
            self.step_log2[last as usize] + (index - last) as f64 * self.step_interval(last)
        } else {
            self.step_log2[index as usize]
        }
    }

    /// The width of the step ending at `index`, in `log2` units.
    #[inline]
    fn step_interval(&self, index: i32) -> f64 {
        // A map too small to have an interval of its own extends in plain semitones.
        if self.step_count < 2 {
            return SEMITONE_IN_LOG2;
        }
        let index = index.clamp(1, self.step_count as i32 - 1) as usize;
        self.step_log2[index] - self.step_log2[index - 1]
    }

    fn fill(&mut self, mut key_frequency: impl FnMut(u8) -> Option<f64>) {
        let twelve_tet_log2 = 440.0_f64.log2() - 69.0 * SEMITONE_IN_LOG2;
        let mut active = false;
        let mut step_count = 0;
        let mut last_frequency = f64::NAN;
        for key in 0..KEY_COUNT {
            let frequency = key_frequency(key as u8);
            self.filtered[key] = frequency.is_none();
            match frequency {
                Some(frequency) if frequency > 0.0 && frequency != last_frequency => {
                    let log2 = frequency.log2();
                    self.step_log2[step_count] = log2;
                    step_count += 1;
                    last_frequency = frequency;
                    let retuning =
                        (log2 - (twelve_tet_log2 + key as f64 * SEMITONE_IN_LOG2)) * 12.0;
                    active |= retuning.abs() > RETUNED_EPSILON_SEMITONES;
                }
                // An unmapped key, or one a master without a keyboard map left out by repeating
                // its predecessor's frequency. Nothing about 12-TET does either.
                _ => active = true,
            }
            self.step_of_key[key] = step_count.saturating_sub(1) as u8;
        }
        self.step_count = step_count;
        self.active = active;
    }
}

impl std::fmt::Debug for TuningMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TuningMap")
            .field("active", &self.active)
            .field("step_count", &self.step_count)
            .finish()
    }
}

// -------------------------------------------------------------------------------------------------

#[inline]
fn key_index(key: u8) -> usize {
    (key as usize).min(KEY_COUNT - 1)
}

#[inline]
fn twelve_tet_frequency_for_key(key: u8) -> f64 {
    twelve_tet_frequency_for_fractional_key(key as f64)
}

#[inline]
fn twelve_tet_frequency_for_fractional_key(key: f64) -> f64 {
    440.0 * ((key - 69.0) * SEMITONE_IN_LOG2).exp2()
}

#[inline]
fn twelve_tet_key_for_frequency(frequency: f64) -> f64 {
    69.0 + 12.0 * (frequency / 440.0).log2()
}
