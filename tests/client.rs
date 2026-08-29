//! Integration tests against the real MTS-ESP C client.
//!
//! Note that everything here must also hold on a machine with no `libMTS` installed,
//! and no master running (which also is a CI's situation) and thus the only state we
//! can assert.

use std::sync::Arc;

use mts_client_rs::Client;

// -------------------------------------------------------------------------------------------------

/// All MIDI notes, plus the edges the C side masks.
const NOTES: [u8; 6] = [0, 1, 60, 69, 126, 127];

fn client() -> Client {
    Client::new().expect("Failed to register an MTS-ESP client")
}

// -------------------------------------------------------------------------------------------------

#[test]
fn registers_and_deregisters() {
    let client = client();
    let _ = client.has_master();
    let _ = client.should_update_library();
    drop(client);
}

#[test]
fn falls_back_to_12_tet_retuning() {
    let client = client();
    if client.has_master() {
        return;
    }
    for note in NOTES {
        assert_eq!(client.retuning_in_semitones(note, None), 0.0);
        assert_eq!(client.retuning_as_ratio(note, None), 1.0);
        assert!(!client.should_filter_note(note, None));
    }
}

#[test]
fn falls_back_to_12_tet_frequencies() {
    let client = client();
    if client.has_master() {
        return;
    }
    assert!((client.note_to_frequency(69, None) - 440.0).abs() < 1e-9);
    assert!((client.note_to_frequency(60, None) - 261.625_565_300_598_6).abs() < 1e-9);

    assert_eq!(client.frequency_to_note(440.0, None), 69);
    for note in 0..=127u8 {
        let frequency = client.note_to_frequency(note, None);
        assert_eq!(client.frequency_to_note(frequency, None), note);
    }
}

#[test]
fn frequency_to_note_and_channel() {
    let client = client();
    let (note, channel) = client.frequency_to_note_and_channel(440.0);
    assert!(channel < 16);
    if !client.has_master() {
        assert_eq!(note, 69);
    }
}

#[test]
fn channel_specific_queries_match_channel_agnostic() {
    let client = client();
    if client.has_master() {
        return;
    }
    for note in NOTES {
        for channel in [None, Some(0), Some(9), Some(15)] {
            assert_eq!(client.retuning_in_semitones(note, channel), 0.0);
            assert_eq!(
                client.note_to_frequency(note, channel),
                client.note_to_frequency(note, None)
            );
            assert!(!client.should_filter_note(note, channel));
        }
    }
}

#[test]
fn default_period_and_unmapped_keyboard() {
    let client = client();
    if client.has_master() {
        return;
    }
    assert_eq!(client.period_semitones(), 12.0);
    assert_eq!(client.period_ratio(), 2.0);
    assert_eq!(client.map_size(), None);
    assert_eq!(client.map_start_key(), None);
    assert_eq!(client.reference_key(), None);
}

#[test]
fn default_scale_name() {
    let client = client();
    if client.has_master() {
        return;
    }
    assert_eq!(client.scale_name(), "12-TET");
}

#[test]
fn parse_midi_data_ignores_junk() {
    let mut client = client();
    if client.has_master() {
        return;
    }
    client.parse_midi_data(&[]);
    client.parse_midi_data(&[0x90, 0x3c, 0x7f]); // note-on
    client.parse_midi_data(&[0xf0, 0x7e, 0x00, 0x08]); // truncated MTS SysEx

    assert!(!client.has_received_mts_sysex());
    for note in NOTES {
        assert_eq!(client.retuning_in_semitones(note, None), 0.0);
    }
}

#[test]
fn client_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Client>();

    // A plugin shares one client between its audio thread and its editor.
    let client = Arc::new(client());
    let queried = {
        let client = Arc::clone(&client);
        std::thread::spawn(move || client.retuning_in_semitones(69, None))
            .join()
            .expect("worker thread panicked")
    };
    assert_eq!(queried, client.retuning_in_semitones(69, None));
}

#[test]
fn multiple_clients_coexist() {
    let first = client();
    let second = client();
    for note in NOTES {
        assert_eq!(
            first.retuning_in_semitones(note, None),
            second.retuning_in_semitones(note, None)
        );
    }
    drop(first);
    // The survivor must still work after its sibling deregistered.
    let _ = second.retuning_in_semitones(69, None);
}
