//! Safe Rust bindings for [MTS-ESP](https://github.com/ODDSound/MTS-ESP), ODDSound's microtuning
//! protocol for audio plugins.
//!
//! This crate only wraps the vendored ODDSound `libMTSClient` library as [`Client`], avoiding
//! unsafe code in user-facing APIs. The raw C API is available at [`sys`].
//!
//! `libMTS`, the library that actually handles the tuning, is loaded dynamically, and thus is
//! not linked in here. So there is nothing to ship with your plugin or app. When it is not
//! installed or no master is connected, all client functions will respond as if a plain 12-TET is
//! loaded, so there's no need to check for a master when querying/applying tunings.
//!
//! `libMTS` itself usually is installed by the user alongside whichever MTS-ESP master they use.
//! Installers are at [ODDSound/MTS-ESP/libMTS](https://github.com/ODDSound/MTS-ESP/tree/main/libMTS).
//!
//! # Real-time safety
//!
//! [`Client::new`] allocates and `dlopen`s. Call it once, but *never from a real-time thread*.
//!
//! [`should_filter_note`](Client::should_filter_note), [`retuning_in_semitones`](Client::retuning_in_semitones),
//! [`retuning_as_ratio`](Client::retuning_as_ratio) and [`note_to_frequency`](Client::note_to_frequency)
//! are lock-free reads of the master's shared tuning table, and are safe to call in real-time threads.
//! [`period_ratio`](Client::period_ratio) and [`period_semitones`](Client::period_semitones) are
//! lock-free reads of the client's own buffers, and thus are safe too.
//!
//! [`Tuning`] is real-time safe throughout: its built from those reads only.
//!
//! Everything else is for the UI, or other non real-time threads. That includes dropping the
//! client: deregistering calls into `libMTS` and frees its tuning tables. Keep an `Arc<Client>`
//! alive on a non real-time thread for the plugin's lifetime, so a real-time thread never holds
//! the last reference and runs the drop itself.
//!
//! # Usage
//!
//! ## Main thread (initialize)
//!
//! Create one [`Client`] per plugin instance, on the main, or some other non audio thread. It is
//! [`Send`] and [`Sync`], so you can wrap it in `Arc` to share it with the audio, worker or UI
//! threads:
//!
//! ```no_run
//! use std::sync::Arc;
//! use mts_client_rs::Client;
//!
//! let mts_client = Client::new().map(Arc::new).expect("Failed to create MTS-ESP client");
//! ```
//!
//! ## Audio thread (processing)
//!
//! Apply tuning on note-on, skipping keys that the master marks as unmapped:
//!
//! ```no_run
//! use std::sync::Arc;
//! use mts_client_rs::Client;
//!
//! // `mts_client` is the `Arc<Client>` from the main thread
//! fn play_note(mts_client: &Arc<Client>, note: u8) {
//!     // An unmapped key should not start a voice.
//!     if !mts_client.should_filter_note(note, None) {
//!         let semitones = mts_client.retuning_in_semitones(note, None);
//!         // add `semitones` to your voice's pitch
//!     }
//! }
//! ```
//!
//! Prefer the semitone offset over [`note_to_frequency`](Client::note_to_frequency): it composes
//! with pitch modulation, whereas an absolute frequency would override them. To have modulation
//! follow the scale rather than 12-TET, use [`Tuning`] instead, as shown below.
//!
//! Masters can automate their tuning, so re-query held notes periodically if you want them to follow
//! changes.
//!
//! Pass `Some(channel)` instead of `None` when the note's MIDI channel is known, so masters using
//! multi-channel tuning tables can answer precisely. MPE is an exception here: there the member
//! channel identifies the voice rather than a tuning table, so MTS-ESP recommends passing `None`
//! as channel here.
//!
//! ## Modulating pitch within the scale
//!
//! A synth voice usually also has pitch modulation such as pitch bend, glide, or LFOs.
//! When added as plain semitones, that modulation would move through 12-TET, while the note it
//! starts from sits in the master's scale, so a voice drifts out of the tuning as soon as it leaves
//! its key.
//!
//! The [`Tuning`] trait fixes that by applying the tuning to the modulated pitch as well.
//! [`Tuning::frequency`] takes a key and one offset of each kind:
//! MIDI keys, i.e. semitones, for e.g. MPE pitch bend, and scale steps for modulation in scale
//! step units.
//!
//! See [`Tuning`] for more info on how the two offsets differ and when and why it matters.
//!
//! [`MtsTuning`] implements `Tuning` for a connected MTS master. It walks the master's scale
//! on every query, so automated tunings or scale changes are followed automatically:
//!
//! ```no_run
//! use mts_client_rs::{Client, MtsTuning, Tuning};
//!
//! # let mts_client = Client::new().unwrap();
//! // Cheap to instatiate: build one per audio block, or per voice, rather than storing it.
//! let tuning = MtsTuning::new(&mts_client, None);
//!
//! // Per voice: MPE bend in semitones and an LFO in scale steps.
//! # let (note, mpe_bend_in_semitones, lfo_in_steps) = (60, 2.0, 0.5);
//! if !tuning.is_key_filtered(note) {
//!     let frequency = tuning.frequency(note, mpe_bend_in_semitones, lfo_in_steps);
//! }
//! ```
//!
//! For a tuning that does not come from a master, e.g. a Scala file, use [`ScaleTuning`] or
//! implement [`Tuning`] yourself.
//!
//! ## Reporting tuning to the user
//!
//! ```no_run
//! # use mts_client_rs::Client;
//! # let mts_client = Client::new().unwrap();
//! if mts_client.has_master() {
//!     println!("MTS-ESP: {}", mts_client.scale_name());
//! }
//! ```
//!
//! There also is [`Client::period_ratio`] / [`Client::period_semitones`] for the scale's period,
//! and [`Client::map_size`] / [`Client::map_start_key`] / [`Client::reference_key`] for the
//! keyboard map.
//!
//! ## MTS SysEx fallback
//!
//! To honour MTS SysEx tuning messages when no MTS-ESP master is present, feed incoming MIDI to
//! the client. A connected master always takes precedence and non-MTS bytes are silently ignored:
//!
//! ```no_run
//! # use mts_client_rs::Client;
//! # let mut mts_client = Client::new().unwrap();
//! # let midi_bytes: &[u8] = &[];
//! mts_client.parse_midi_data(midi_bytes);
//! ```
//!
//! # Platform notes
//!
//! On Windows a standalone binary may not find `libMTS`. The client locates `LIBMTS.dll`
//! through `SHGetKnownFolderPath`, which it only resolves when `Shell32.dll` and `Ole32.dll` are
//! already loaded. Plugin hosts usually will have both libraries loaded, a plain console binary
//! not. To fix this, preload the two DLLs from an early CRT initializer in your standalone app.

use std::{ffi::CStr, os::raw::c_char, ptr::NonNull};

pub mod sys;

mod tuning;
pub use tuning::{MtsTuning, ScaleTuning, Tuning};

// -------------------------------------------------------------------------------------------------

/// A registered MTS-ESP client instance.
///
/// Registers with MTS-ESP on [`Client::new`] and deregisters on drop. Create one per plugin instance
/// and share it: it is [`Send`] and [`Sync`], so an `Arc<Client>` can be passed to other threads.
///
/// Note: Dropping the client is not real-time safe: it calls into `libMTS` and frees the client's
/// tuning tables. So keep an `Arc` on a non real-time thread for the plugin's lifetime, so that a
/// real-time thread never holds the last reference.
///
/// All queries take an optional MIDI channel. Pass `Some(channel)` (`0..=15`) when the note's
/// channel is known, so masters using multi-channel tuning tables can answer precisely; pass `None`
/// for a channel-agnostic query.
pub struct Client {
    raw: NonNull<sys::MTSClient>,
}

// SAFETY: every `&self` method here is a read.
//
// `retuning_*`, `note_to_frequency` and `should_filter_note` read the master's shared tuning table
// lock-free. The remaining queries read the client's own buffers.
//
// The only mutating entry point, `parse_midi_data`, takes `&mut self`, so it can never run
// concurrently with the reads.
unsafe impl Send for Client {}
unsafe impl Sync for Client {}

impl Client {
    /// Register with MTS-ESP. Allocates and `dlopen`s `libMTS`, so never call this from an
    /// audio thread.
    ///
    /// Succeeds whether or not `libMTS` is installed, and whether or not a master is running.
    /// The client simply behaves as if a 12-TET tuning is loaded until one appears.
    ///
    /// The only possible error is an allocation failure. Nothing about the installation or the
    /// state of `libMTS` can make this fail.
    pub fn new() -> Result<Self, &'static str> {
        NonNull::new(unsafe { sys::MTS_RegisterClient() })
            .map(|raw| Self { raw })
            .ok_or("Failed to allocate the MTS-ESP client")
    }

    /// True when an MTS-ESP master is currently connected.
    ///
    /// Only useful to tell the user whether microtuning is active: the audio path needs no such
    /// check, because without a master all functions behave as if a 12-TET tuning is loaded.
    pub fn has_master(&self) -> bool {
        unsafe { sys::MTS_HasMaster(self.as_ptr()) }
    }

    /// True when the installed `libMTS` is older than the API this client was built against, and
    /// so cannot serve all of it. You may want to hint this in your UI.
    pub fn should_update_library(&self) -> bool {
        unsafe { sys::MTS_ShouldUpdateLibrary(self.as_ptr()) }
    }

    /// True when the master's keyboard map leaves `note` unmapped, in which case the note should
    /// not produce any sound at all. **Real-time safe**.
    #[inline]
    pub fn should_filter_note(&self, note: u8, channel: Option<u8>) -> bool {
        unsafe { sys::MTS_ShouldFilterNote(self.as_ptr(), midi_note(note), midi_channel(channel)) }
    }

    /// The note's tuned frequency in Hz. **Real-time safe**.
    ///
    /// An absolute frequency overrides pitch modulation instead of composing with it, so prefer
    /// [`Client::retuning_in_semitones`] for a voice that only retunes its note, and
    /// [`Tuning`] for one whose pitch modulation should follow the scale.
    #[inline]
    pub fn note_to_frequency(&self, note: u8, channel: Option<u8>) -> f64 {
        unsafe { sys::MTS_NoteToFrequency(self.as_ptr(), midi_note(note), midi_channel(channel)) }
    }

    /// The note's offset from 12-TET, in semitones. Zero without a master. **Real-time safe**.
    ///
    /// See also [`Tuning`] to apply tuning with pitch modulation.
    #[inline]
    pub fn retuning_in_semitones(&self, note: u8, channel: Option<u8>) -> f64 {
        unsafe {
            sys::MTS_RetuningInSemitones(self.as_ptr(), midi_note(note), midi_channel(channel))
        }
    }

    /// The note's offset from 12-TET as a frequency ratio. **Real-time safe**.
    #[inline]
    pub fn retuning_as_ratio(&self, note: u8, channel: Option<u8>) -> f64 {
        unsafe { sys::MTS_RetuningAsRatio(self.as_ptr(), midi_note(note), midi_channel(channel)) }
    }

    /// The note whose tuned pitch is closest to `frequency`. **Real-time safe**.
    ///
    /// Pass `Some(channel)` if the resulting note-on goes out on a known channel, so the matching
    /// multi-channel tuning table is consulted; `None` ignores multi-channel tables.
    pub fn frequency_to_note(&self, frequency: f64, channel: Option<u8>) -> u8 {
        let note =
            unsafe { sys::MTS_FrequencyToNote(self.as_ptr(), frequency, midi_channel(channel)) };
        note as u8
    }

    /// The note closest to `frequency`, together with the MIDI channel to send it on. **Real-time safe**.
    ///
    /// Use this instead of [`Client::frequency_to_note`] when you are free to pick the channel:
    /// multi-channel tuning tables are always consulted.
    pub fn frequency_to_note_and_channel(&self, frequency: f64) -> (u8, u8) {
        let mut channel: i8 = 0;
        let note =
            unsafe { sys::MTS_FrequencyToNoteAndChannel(self.as_ptr(), frequency, &mut channel) };
        (note as u8 & 127, channel as u8 & 15)
    }

    /// A copy of the master's current scale name, or "12-TET" without a master.
    /// Can be empty as well, when the master has no name to report.
    pub fn scale_name(&self) -> String {
        // Note: The name must be copied here rather than borrowed: the pointer belongs to `libMTS`,
        // and a connected master may rewrite that buffer at any time from its own process.
        let name = unsafe { sys::MTS_GetScaleName(self.as_ptr()) };
        if name.is_null() {
            return String::new();
        }
        // The master supplies arbitrary bytes here, so do not assume they are UTF-8.
        String::from_utf8_lossy(unsafe { CStr::from_ptr(name) }.to_bytes()).into_owned()
    }

    /// The scale's period as a frequency ratio. 2.0 (an octave) unless a master says otherwise.
    pub fn period_ratio(&self) -> f64 {
        unsafe { sys::MTS_GetPeriodRatio(self.as_ptr()) }
    }

    /// The scale's period in semitones. 12.0 (an octave) unless a master says otherwise.
    pub fn period_semitones(&self) -> f64 {
        unsafe { sys::MTS_GetPeriodSemitones(self.as_ptr()) }
    }

    /// Number of keys in the keyboard map, or `None` if no master supplied one.
    pub fn map_size(&self) -> Option<u8> {
        optional_key(unsafe { sys::MTS_GetMapSize(self.as_ptr()) })
    }

    /// The key the keyboard map starts at, or `None` if no master supplied one.
    pub fn map_start_key(&self) -> Option<u8> {
        optional_key(unsafe { sys::MTS_GetMapStartKey(self.as_ptr()) })
    }

    /// The keyboard map's reference key, or `None` if no master supplied one.
    pub fn reference_key(&self) -> Option<u8> {
        optional_key(unsafe { sys::MTS_GetRefKey(self.as_ptr()) })
    }

    /// Feed incoming MIDI to the client so it can pick up MTS SysEx tuning messages.
    ///
    /// You only needed to pass MTS SysEx as fallback when no MTS-ESP master is connected. A
    /// connected master always takes precedence. All MTS SysEx formats are accepted, and non-MTS
    /// bytes are ignored. Takes `&mut self` because it updates the client's local tuning tables.
    pub fn parse_midi_data(&mut self, bytes: &[u8]) {
        let length = bytes.len().min(i32::MAX as usize) as i32;
        unsafe { sys::MTS_ParseMIDIDataU(self.as_ptr(), bytes.as_ptr(), length) }
    }

    /// True once [`Client::parse_midi_data`] has seen a valid MTS SysEx message, meaning the client
    /// falls back to that local tuning while no master is connected.
    pub fn has_received_mts_sysex(&self) -> bool {
        unsafe { sys::MTS_HasReceivedMTSSysEx(self.as_ptr()) }
    }

    fn as_ptr(&self) -> *mut sys::MTSClient {
        self.raw.as_ptr()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        unsafe { sys::MTS_DeregisterClient(self.as_ptr()) }
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("has_master", &self.has_master())
            .field("scale_name", &self.scale_name())
            .finish()
    }
}

// -------------------------------------------------------------------------------------------------

/// `None` is the C API's "no particular channel" (-1). An out-of-range channel is clamped,
/// but asserted. The C API masks the channel to ensure it's in the valid range.
#[inline]
fn midi_channel(channel: Option<u8>) -> i8 {
    match channel {
        Some(channel) => {
            debug_assert!(
                channel < 16,
                "MIDI channel {channel} is out of range 0..=15"
            );
            channel.min(15) as i8
        }
        None => -1,
    }
}

/// The C side masks notes to 0..=127, so clamp instead for consistency with [`Tuning`].
#[inline]
fn midi_note(note: u8) -> c_char {
    debug_assert!(note < 128, "MIDI note {note} is out of range 0..=127");
    note.min(127) as c_char
}

/// Keyboard map queries report "not supplied by a master" as -1.
#[inline]
fn optional_key(value: i8) -> Option<u8> {
    (value >= 0).then_some(value as u8)
}
