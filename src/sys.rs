//! Raw declarations for the MTS-ESP client C API, one to one with `libMTSClient.h`.
//!
//! Prefer using the safe [`Client`](crate::Client) API instead.
//!
//! Every function accepts a null `client`, and then behaves as if no master were connected.

use std::os::raw::{c_char, c_int};

// -------------------------------------------------------------------------------------------------

/// Opaque client handle, allocated and freed by the C++ side.
#[repr(C)]
pub struct MTSClient {
    _private: [u8; 0],
}

unsafe extern "C" {
    pub fn MTS_RegisterClient() -> *mut MTSClient;
    pub fn MTS_DeregisterClient(client: *mut MTSClient);

    pub fn MTS_HasMaster(client: *mut MTSClient) -> bool;
    pub fn MTS_Client_ShouldUpdateLibrary(client: *mut MTSClient) -> bool;

    pub fn MTS_ShouldFilterNote(client: *mut MTSClient, note: c_char, channel: i8) -> bool;

    pub fn MTS_NoteToFrequency(client: *mut MTSClient, note: c_char, channel: i8) -> f64;
    pub fn MTS_RetuningInSemitones(client: *mut MTSClient, note: c_char, channel: i8) -> f64;
    pub fn MTS_RetuningAsRatio(client: *mut MTSClient, note: c_char, channel: i8) -> f64;

    pub fn MTS_FrequencyToNote(client: *mut MTSClient, frequency: f64, channel: i8) -> c_char;
    pub fn MTS_FrequencyToNoteAndChannel(
        client: *mut MTSClient,
        frequency: f64,
        channel: *mut i8,
    ) -> c_char;

    pub fn MTS_GetScaleName(client: *mut MTSClient) -> *const c_char;

    pub fn MTS_GetPeriodRatio(client: *mut MTSClient) -> f64;
    pub fn MTS_GetPeriodSemitones(client: *mut MTSClient) -> f64;

    pub fn MTS_GetMapSize(client: *mut MTSClient) -> i8;
    pub fn MTS_GetMapStartKey(client: *mut MTSClient) -> i8;
    pub fn MTS_GetRefKey(client: *mut MTSClient) -> i8;

    pub fn MTS_ParseMIDIDataU(client: *mut MTSClient, buffer: *const u8, length: c_int);
    pub fn MTS_ParseMIDIData(client: *mut MTSClient, buffer: *const i8, length: c_int);

    pub fn MTS_HasReceivedMTSSysEx(client: *mut MTSClient) -> bool;
}
