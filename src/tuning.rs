//! Applying a tuning to a *modulated* pitch, by walking the scale.

use crate::Client;

// -------------------------------------------------------------------------------------------------

/// Number of MIDI keys.
const KEY_COUNT: i32 = 128;
/// Highest MIDI key.
const LAST_KEY: i32 = KEY_COUNT - 1;

// -------------------------------------------------------------------------------------------------

/// Apply note scaling with fractional modulation on top.
///
/// [`Client::retuning_in_semitones`] retunes a *note*, but synths add pitch modulation on top of
/// it: pitch bend, glide, an LFO on oscillator pitch. Added as plain semitones, that modulation
/// moves through 12-TET while the note it starts from sits in the scale, so a voice drifts out of
/// tune as soon as it leaves its key.
///
/// [`Tuning::frequency`] takes a key and one offset of each kind, because which unit is right
/// depends on where the modulation comes from:
///
/// * `key_offset` is a fractional offset in **MIDI keys**, one key being one semitone in 12-TET:
///   MPE pitch bend, whose range MPE defines in semitones, and anything else driven by a
///   controller's key geometry. A key width of it stays a key width, so a physical octave stays an
///   octave whether or not the keys in between are mapped. The pitch is taken from the mapped keys
///   only and interpolated across unmapped ones, so a slide over a gap ramps smoothly between the
///   scale pitches either side of it rather than resting on a pitch the scale does not contain.
///
/// * `step_modulation` is measured in **scale steps**: an LFO or envelope moving a voice by scale
///   degrees, an arpeggiator or sequencer transposing within the scale. A whole unit lands on the
///   next note of the scale, and a fraction interpolates between neighbouring ones.
///
/// The key offset applies first, and the steps are then walked from the bent position, so a scale
/// step is as wide as the scale is where the voice actually is. Pass `0.0` for the one you do not
/// need. For a scale without gaps the two units are the same thing; they part ways only where a
/// key is unmapped, or where a scale repeats a key's pitch instead of filtering it.
///
/// Modulation that should *not* follow the scale, e.g. a vibrato LFO in plain semitones, can be
/// applied last, on top of the result: add it to the fractional key that [`Tuning::pitch`] returns,
/// or multiply the frequency by `2^(semitones / 12)`.
///
/// Implement this for your own scale, or use [`MtsTuning`] for an MTS-ESP master and
/// [`ScaleTuning`] for a tuning that comes from elsewhere. Only the first two methods are
/// required; the rest describe how the scale repeats, and answer for plain 12-TET by default.
///
/// # Example
///
/// ```no_run
/// use mts_client_rs::{Client, MtsTuning, Tuning};
///
/// # let client = Client::new().unwrap();
/// // Pass `None` as channel for MPE, where the member channel identifies the voice.
/// let tuning = MtsTuning::new(&client, None);
///
/// // Per voice: an MPE bend in semitones, and an LFO in scale steps.
/// let (key, mpe_bend_in_semitones, lfo_in_steps) = (60, 2.0, 0.5);
/// if !tuning.is_key_filtered(key) {
///     let frequency = tuning.frequency(key, mpe_bend_in_semitones, lfo_in_steps);
/// }
/// ```
pub trait Tuning {
    /// True when the scale leaves `key` unmapped. Unmapped keys should not start playback.
    fn is_key_filtered(&self, key: u8) -> bool;

    /// The key's offset from 12-TET in semitones, ignoring any modulation.
    fn key_retuning_in_semitones(&self, key: u8) -> f64;

    /// How far apart the scale's repetitions are, in semitones. 12.0 by default (an octave).
    fn period_in_semitones(&self) -> f64 {
        12.0
    }

    /// How many MIDI keys one repetition of the scale spans, when that is known.
    ///
    /// This is used to extend modulation past the ends of the keyboard. Without it,
    /// pitch past either end extends in plain semitones.
    fn keys_per_period(&self) -> Option<u8> {
        None
    }

    /// The tuned frequency of `key`, moved by both kinds of modulation, in Hz.
    ///
    /// See the [trait docs](Tuning) for what the two offsets mean and when they differ.
    fn frequency(&self, key: u8, key_offset: f64, step_modulation: f64) -> f64 {
        let pitch = self.pitch(key, key_offset, step_modulation);
        440.0 * ((pitch - 69.0) / 12.0).exp2()
    }

    /// The offset from 12-TET in semitones, of [`Tuning::frequency`]: what to add to
    /// `key + key_offset + step_modulation` to reach the final tuned pitch.
    ///
    /// See the [trait docs](Tuning) for what the two offsets mean and when they differ.
    fn retuning_in_semitones(&self, key: u8, key_offset: f64, step_modulation: f64) -> f64 {
        let pitch = self.pitch(key, key_offset, step_modulation);
        pitch - (key as f64 + key_offset + step_modulation)
    }

    /// The tuned pitch of `key` as a fractional MIDI key (69.0 = A4 in 12-TET).
    fn pitch(&self, key: u8, key_offset: f64, step_modulation: f64) -> f64 {
        debug_assert!(
            key < KEY_COUNT as u8,
            "MIDI key {key} is out of range 0..=127"
        );
        debug_assert!(
            key_offset.is_finite() && step_modulation.is_finite(),
            "modulation {key_offset}/{step_modulation} is not a finite number"
        );
        ScaleWalk::new(self).pitch(key as f64 + key_offset, step_modulation)
    }
}

// -------------------------------------------------------------------------------------------------

/// Tuning implementation of a MTS-ESP master *for one MIDI channel*.
///
/// Build one per audio block or per voice rather than storing it. Every query reads the master
/// directly, so an automated tuning is followed automatically.
///
/// Pass `Some(channel)` when the note's MIDI channel is known, so masters using multi-channel
/// tuning tables can answer precisely. MPE is the exception: there the member channel identifies
/// the voice rather than a tuning table, so MTS-ESP recommends `None` while in MPE mode.
pub struct MtsTuning<'a> {
    client: &'a Client,
    channel: Option<u8>,
}

impl<'a> MtsTuning<'a> {
    /// Reads `client` on `channel`, or channel-agnostically with `None`.
    pub fn new(client: &'a Client, channel: Option<u8>) -> Self {
        Self { client, channel }
    }
}

impl Tuning for MtsTuning<'_> {
    #[inline]
    fn is_key_filtered(&self, key: u8) -> bool {
        self.client.should_filter_note(key, self.channel)
    }

    #[inline]
    fn key_retuning_in_semitones(&self, key: u8) -> f64 {
        self.client.retuning_in_semitones(key, self.channel)
    }

    #[inline]
    fn period_in_semitones(&self) -> f64 {
        self.client.period_semitones()
    }

    #[inline]
    fn keys_per_period(&self) -> Option<u8> {
        self.client.map_size()
    }
}

impl std::fmt::Debug for MtsTuning<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MtsTuning")
            .field("channel", &self.channel)
            .finish()
    }
}

// -------------------------------------------------------------------------------------------------

/// Tuning implementation that does not come from an MTS-ESP master, e.g. a Scala file or a
/// completely custom one.
///
/// Holds one retuning per MIDI key, so queries need no allocation and are real-time safe.
#[derive(Clone)]
pub struct ScaleTuning {
    retuning: [f64; KEY_COUNT as usize],
    filtered: [bool; KEY_COUNT as usize],
    period_in_semitones: f64,
    keys_per_period: Option<u8>,
}

impl Default for ScaleTuning {
    fn default() -> Self {
        Self::new()
    }
}

impl ScaleTuning {
    /// Create a new plain 12-TET tuning, with every key mapped.
    pub fn new() -> Self {
        Self {
            retuning: [0.0; KEY_COUNT as usize],
            filtered: [false; KEY_COUNT as usize],
            period_in_semitones: 12.0,
            keys_per_period: None,
        }
    }

    /// Builds a scale from a frequency per key in Hz, or `None` when the key is unmapped.
    pub fn from_frequencies(mut key_frequency: impl FnMut(u8) -> Option<f64>) -> Self {
        let mut scale = Self::new();
        for key in 0..KEY_COUNT {
            match key_frequency(key as u8) {
                Some(frequency) if frequency > 0.0 => {
                    scale.retuning[key as usize] =
                        69.0 + 12.0 * (frequency / 440.0).log2() - key as f64;
                }
                _ => scale.filtered[key as usize] = true,
            }
        }
        scale
    }

    /// Sets how the scale repeats, which is what carries modulation past the ends of the keyboard.
    ///
    /// Without it, pitch past either end extends in plain semitones.
    pub fn with_period(mut self, semitones: f64, keys_per_period: u8) -> Self {
        self.period_in_semitones = semitones;
        self.keys_per_period = Some(keys_per_period);
        self
    }
}

impl Tuning for ScaleTuning {
    #[inline]
    fn is_key_filtered(&self, key: u8) -> bool {
        debug_assert!(
            key < KEY_COUNT as u8,
            "MIDI key {key} is out of range 0..=127"
        );
        self.filtered[(key as usize).min(LAST_KEY as usize)]
    }

    #[inline]
    fn key_retuning_in_semitones(&self, key: u8) -> f64 {
        debug_assert!(
            key < KEY_COUNT as u8,
            "MIDI key {key} is out of range 0..=127"
        );
        self.retuning[(key as usize).min(LAST_KEY as usize)]
    }

    #[inline]
    fn period_in_semitones(&self) -> f64 {
        self.period_in_semitones
    }

    #[inline]
    fn keys_per_period(&self) -> Option<u8> {
        self.keys_per_period
    }
}

impl std::fmt::Debug for ScaleTuning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mapped = self.filtered.iter().filter(|filtered| !**filtered).count();
        f.debug_struct("ScaleTuning")
            .field("mapped_keys", &mapped)
            .field("period_in_semitones", &self.period_in_semitones)
            .field("keys_per_period", &self.keys_per_period)
            .finish()
    }
}

// -------------------------------------------------------------------------------------------------

/// Walks the notes of a [`Tuning`]'s scale to apply modulation within the scale.
///
/// Notes are `(key, pitch)` tuples: the key a note starts on, and its retuned pitch as
/// fractional 12-TET key.
struct ScaleWalk<'a, T: Tuning + ?Sized> {
    tuning: &'a T,
    /// How many keys and how many semitones one repetition of the scale spans, when known.
    period: Option<(i32, f64)>,
    /// How many keys to search for the next note before giving up the search.
    search_limit: i32,
}

impl<'a, T: Tuning + ?Sized> ScaleWalk<'a, T> {
    fn new(tuning: &'a T) -> Self {
        let period = tuning
            .keys_per_period()
            .filter(|keys| *keys > 0)
            .map(|keys| (keys as i32, tuning.period_in_semitones()));
        let search_limit = 2 * period.map_or(KEY_COUNT, |(keys, _)| keys);
        Self {
            tuning,
            period,
            search_limit,
        }
    }

    /// The tuned pitch of a fractional `bent_key`, moved by `step_modulation` scale steps.
    fn pitch(&self, bent_key: f64, step_modulation: f64) -> f64 {
        // Skip interpolation when the key is not modulated.
        if step_modulation == 0.0 && bent_key.fract() == 0.0 {
            if let Some(pitch) = self.pitch_of(bent_key as i32) {
                return pitch;
            }
        }
        // The note the bent position sits on, and the next one above it.
        let mut below = self.notes_at_or_below(bent_key.floor() as i32);
        let Some(mut lower) = below.next() else {
            // Nothing mapped: no scale to follow -> plain 12-TET.
            return bent_key + step_modulation;
        };
        let mut above = self.notes_above(lower);
        let Some(mut upper) = above.next() else {
            // A single note has no interval of its own: extend it in plain semitones.
            let (lower_key, lower_pitch) = lower;
            return lower_pitch + (bent_key + step_modulation - lower_key as f64);
        };

        // Scale steps from `lower`: the bent position ramps evenly across the keys up to `upper`,
        // so a slide over a gap sweeps smoothly, and the step modulation adds on top of that.
        let ((lower_key, _), (upper_key, _)) = (lower, upper);
        let mut steps =
            (bent_key - lower_key as f64) / (upper_key - lower_key) as f64 + step_modulation;

        // Walk whole steps until `steps` lies between `lower` and `upper`.
        while steps >= 1.0 {
            let Some(next) = above.next() else { break };
            (lower, upper) = (upper, next);
            steps -= 1.0;
        }
        while steps < 0.0 {
            let Some(previous) = below.next() else { break };
            (lower, upper) = (previous, lower);
            steps += 1.0;
        }

        let ((_, lower_pitch), (_, upper_pitch)) = (lower, upper);
        lower_pitch + (upper_pitch - lower_pitch) * steps
    }

    /// A key's pitch as a fractional 12-TET key or `None` when it's unmapped.
    ///
    /// Past the ends of the keyboard a key repeats the key whole periods in, or without a period,
    /// continues the outermost key in plain semitones.
    fn pitch_of(&self, key: i32) -> Option<f64> {
        let (resolved, shift) = match self.period {
            Some((keys_per_period, period_in_semitones)) if !(0..=LAST_KEY).contains(&key) => {
                // Whole periods to shift by, so that the key lands back on the keyboard.
                let periods = if key < 0 {
                    key.div_euclid(keys_per_period)
                } else {
                    (key - LAST_KEY + keys_per_period - 1) / keys_per_period
                };
                let resolved = (key - periods * keys_per_period).clamp(0, LAST_KEY);
                (resolved, periods as f64 * period_in_semitones)
            }
            _ => {
                let held = key.clamp(0, LAST_KEY);
                (held, (key - held) as f64)
            }
        };
        let resolved = resolved as u8;
        (!self.tuning.is_key_filtered(resolved))
            .then(|| resolved as f64 + self.tuning.key_retuning_in_semitones(resolved) + shift)
    }

    /// Two keys count as the same note when their pitches are this close, in semitones.
    fn same_pitch(one: f64, other: f64) -> bool {
        const SAME_PITCH_EPSILON: f64 = 1e-9;
        (one - other).abs() <= SAME_PITCH_EPSILON
    }

    /// Iterator of retuned notes starting at or below `key`, in descending order.
    fn notes_at_or_below(&self, key: i32) -> impl Iterator<Item = (i32, f64)> + '_ {
        let mut key = key + 1;
        let mut pending: Option<(i32, f64)> = None;
        std::iter::from_fn(move || {
            for _ in 0..self.search_limit {
                key -= 1;
                let Some(pitch) = self.pitch_of(key) else {
                    continue;
                };
                match &mut pending {
                    Some((note_key, note_pitch)) if Self::same_pitch(*note_pitch, pitch) => {
                        *note_key = key
                    }
                    _ => {
                        if let Some(note) = pending.replace((key, pitch)) {
                            return Some(note);
                        }
                    }
                }
            }
            pending.take()
        })
    }

    /// Iterator of retuned notes above `note`, in ascending order.
    fn notes_above(&self, note: (i32, f64)) -> impl Iterator<Item = (i32, f64)> + '_ {
        let (mut key, mut pitch) = note;
        std::iter::from_fn(move || {
            for _ in 0..self.search_limit {
                key += 1;
                match self.pitch_of(key) {
                    Some(next) if !Self::same_pitch(next, pitch) => {
                        pitch = next;
                        return Some((key, pitch));
                    }
                    _ => {}
                }
            }
            None
        })
    }
}
