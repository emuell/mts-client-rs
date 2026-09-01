# mts-client-rs

Safe Rust bindings for [MTS-ESP](https://github.com/ODDSound/MTS-ESP), ODDSound's microtuning protocol for audio plugins.

This crate wraps the vendored ODDSound **client** library (`libMTSClient`) as a safe `Client`, avoiding unsafe code in the public APIs. The raw *unsafe* C API is available under `mts_client_rs::sys`.

`libMTS`, the library that actually carries the tuning, is loaded at runtime rather than linked in, so there is nothing to ship with your plugin or app. When it is not installed, or no master is connected, every query answers as if a plain 12-TET is loaded. Users install it alongside whichever MTS-ESP master they use, from [ODDSound/MTS-ESP/libMTS](https://github.com/ODDSound/MTS-ESP/tree/main/libMTS).

`TuningMap` (an addition of this crate, not part of `libMTSClient`) indexes the tuning by fractional *scale steps* rather than by MIDI *keys*, so pitch modulation such as glide or bend can move through the master's scale as well.

## Installation

```sh
cargo add mts-client-rs
```

or add it manually to your `Cargo.toml`:

```toml
[dependencies]
mts-client-rs = "0.1"
```

The vendored client sources are part of the published crate, so all you need to build is a C++ compiler (MSVC, clang++ or g++) and Rust 1.82 or later. 

## Examples and Usage

Examples covering initialization, note re-tuning, modulating pitch within the scale, reporting tuning to the user, and the MTS SysEx fallback can be found in the crate documentation, along with the real-time safety rules and platform notes:

- [docs.rs/mts-client-rs](https://docs.rs/mts-client-rs): the rendered API documentation
- [`src/lib.rs`](https://github.com/emuell/mts-client-rs/blob/master/src/lib.rs): the module docs it is generated from

## Building from source

Clone with `git clone --recurse-submodules <url>`: the client source is a git submodule under `vendor/MTS-ESP/`.

## Limitations

- The master-side API (`libMTSMaster`) is not wrapped. It's another use-case. It links the `libMTS` binary rather than loading it at runtime, so this should be handled in a different crate.

- Only one copy of this crate can be linked into a build graph: It declares `links = "mtsclient"`, so Cargo rejects duplicates instead of letting them collide at link time.

- On Windows, a standalone binary may not find `libMTS` at all, and then silently reports 12-TET. Plugin hosts are unaffected. See the crate documentation for the cause and the workaround.

## License

`mts-client-rs` is licensed under the 0BSD license, consistent with the upstream MTS-ESP client library it vendors (Copyright (C) 2021 ODDSound Ltd.).
