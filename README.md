# mts-client-rs

Safe Rust bindings for [MTS-ESP](https://github.com/ODDSound/MTS-ESP), ODDSound's microtuning protocol for audio plugins.

This crate wraps the ODDSound client library (`libMTSClient`) with no unsafe code in the user-facing API. The raw C API is available as well under `mts_client_rs::sys`, in case you want to use it directly.

## Prerequisites

- Rust toolchain (stable, 1.82 or later)
- A C++ compiler (MSVC, clang++, or g++)

> Clone with `git clone --recurse-submodules <url>`. The MTS-ESP C client source is included as a git submodule under `vendor/MTS-ESP/`.

## How libMTS works

The client `dlopen`s the real `libMTS` at runtime, so there is no link-time dependency and nothing to ship with your plugin.

When no master is connected, or when `libMTS` is not installed at all, a plain 12-TET is assumed. So you don't need to check whether a master is connected.

`libMTS` itself usually is installed by the user alongside whichever MTS-ESP master they use. Installers are at [ODDSound/MTS-ESP/libMTS](https://github.com/ODDSound/MTS-ESP/tree/main/libMTS).

## Usage

### Main thread (initialize)

Create one `Client` per plugin instance, on the main or some other **non audio thread**. It is `Send + Sync`, so you can wrap it in an `Arc` to share it with the audio or other worker, UI threads:

```rust
use std::sync::Arc;
use mts_client_rs::Client;

let mts_client = Client::new().map(Arc::new).expect("Failed to register the MTS-ESP client");
```

### Audio Thread (processing)

On note-on, skip keys that the master leaves unmapped, then apply the tuning:

```rust
let note = 60;
if !mts_client.should_filter_note(note, None) {
    let semitones = mts_client.retuning_in_semitones(note, None);
    // add `semitones` to your voice's pitch
}
```

Prefer the semitone offset over `note_to_frequency`: it composes with pitch modulation such as pitch bend, note expressions and glide, whereas an absolute frequency would override them. Masters can automate their tuning, so re-query held notes periodically if you want to follow changes.

Pass `Some(channel)` instead of `None` when the note's MIDI channel is known, so masters using multi-channel tuning tables can answer precisely.

Real-time thread safety: `should_filter_note`, `retuning_in_semitones`, `retuning_as_ratio` and `note_to_frequency` are lock-free reads of the master's shared tuning table, and thus are safe to call in real-time threads. Everything else is for the UI or message thread.

This includes dropping the client: deregistering calls into `libMTS` and frees its tuning tables. Keep the `Arc` from above alive on the main thread for the plugin's lifetime, so the audio thread never holds the last reference and runs the drop itself.

### Reporting tuning to the user

```rust
if mts_client.has_master() {
    println!("MTS-ESP: {}", mts_client.scale_name());
}
```

There is also `period_ratio` / `period_semitones` for the scale's period, and `map_size` / `map_start_key` / `reference_key` for the keyboard map (each `None` when no master supplied one).

### MTS SysEx fallback

If you want to honour MTS SysEx tuning messages when no MTS-ESP master is present, feed incoming MIDI to the client:

```rust
mts_client.parse_midi_data(midi_bytes);
```

A connected master always takes precedence. This is the only mutating call in the API.

## Notes

- The master-side API (`libMTSMaster`) is not wrapped here. It link-depends on the shipped `libMTS` binary rather than loading it at runtime, so it is a different distribution problem.

- Only one copy of this crate can be linked into a build graph. It declares `links = "mtsclient"`, so Cargo rejects duplicates instead of letting them collide at link time.

- On Windows, a standalone binary may not find `libMTS`. The client locates `LIBMTS.dll` through `SHGetKnownFolderPath`, which it only resolves when `Shell32.dll` and `Ole32.dll` are already loaded. Plugin hosts usually will have both loaded, a plain console binary not. Preload the two DLLs from an early CRT initializer if you need a standalone app to see a master.

## License

`mts-client-rs` is licensed under the 0BSD license, consistent with the upstream MTS-ESP client library it vendors (Copyright (C) 2021 ODDSound Ltd.).
