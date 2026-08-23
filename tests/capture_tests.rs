//! Capture-to-VCD tests
//!
//! Each test writes a capture out and reads it back through wellen, so the
//! assertions cover what a waveform consumer actually sees.

use std::io::Write;
use tempfile::NamedTempFile;
use waveform_mcp::LogicCapture;
use waveform_mcp::{list_signals, read_signal_values};

fn names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("dio{}", i)).collect()
}

/// Write VCD text to a temp file and hand back the parsed waveform.
fn round_trip(capture: &LogicCapture, scope: &str) -> wellen::simple::Waveform {
    let vcd = capture.to_vcd(scope);
    let mut file = NamedTempFile::new().expect("temp file");
    write!(file, "{}", vcd).expect("write vcd");
    file.flush().expect("flush");
    wellen::simple::read(file.path()).expect("wellen reads the generated vcd")
}

#[test]
fn capture_round_trips_through_wellen() {
    let capture = LogicCapture {
        channel_names: names(4),
        sample_rate_hz: 100_000_000.0,
        // dio0 toggles every sample, dio1 every two, and so on.
        samples: (0..16u32).collect(),
    };

    let mut waveform = round_trip(&capture, "dio");
    let hierarchy = waveform.hierarchy();
    let signals = list_signals(hierarchy, None, None, true, None);
    assert_eq!(
        signals,
        vec!["dio.dio0", "dio.dio1", "dio.dio2", "dio.dio3"],
        "every named channel becomes a signal under the requested scope"
    );

    // Sample n holds the value n, so dio0 is n's low bit.
    let signal_ref =
        waveform_mcp::find_signal_by_path(waveform.hierarchy(), "dio.dio0").expect("dio0 resolves");
    waveform.load_signals(&[signal_ref]);
    let values = read_signal_values(&waveform, signal_ref, &[0, 1, 2, 3]).expect("dio0 reads");
    let bits: Vec<&str> = values
        .iter()
        .map(|line| line.rsplit(": ").next().expect("value follows the colon"))
        .collect();
    assert_eq!(bits, vec!["1'b0", "1'b1", "1'b0", "1'b1"]);

    // Sample spacing shows up as real time, not just as an index.
    assert!(
        values[1].contains("10ns"),
        "second sample sits at 10ns: {}",
        values[1]
    );
}

#[test]
fn timescale_uses_the_coarsest_whole_unit() {
    let at = |rate: f64| {
        let capture = LogicCapture {
            channel_names: names(1),
            sample_rate_hz: rate,
            samples: vec![0, 1],
        };
        let vcd = capture.to_vcd("dio");
        vcd.lines()
            .find(|l| l.starts_with("$timescale"))
            .expect("timescale line")
            .to_string()
    };

    // The magnitude stays at 1 because VCD only permits 1, 10 or 100; the
    // sample period is carried by the tick count instead.
    assert_eq!(at(100_000_000.0), "$timescale 1ns $end");
    assert_eq!(at(1_000.0), "$timescale 1ms $end");
    assert_eq!(at(1.0), "$timescale 1s $end");
    // 3 MHz lands on no whole ns or ps, so femtoseconds carry it.
    assert_eq!(at(3_000_000.0), "$timescale 1fs $end");
}

#[test]
fn unchanged_samples_emit_no_timestamp() {
    let capture = LogicCapture {
        channel_names: names(2),
        sample_rate_hz: 1_000_000.0,
        // Steady, then one change, then steady again.
        samples: vec![0, 0, 0, 3, 3, 3],
    };
    let vcd = capture.to_vcd("dio");
    let stamps: Vec<&str> = vcd.lines().filter(|l| l.starts_with('#')).collect();
    assert_eq!(
        stamps,
        vec!["#0", "#3"],
        "only the initial state and the one real change are recorded"
    );
}

#[test]
fn bits_above_the_named_channels_are_ignored() {
    let capture = LogicCapture {
        channel_names: names(2),
        sample_rate_hz: 1_000_000.0,
        // Only the low two bits are wired to a name; the rest is noise.
        samples: vec![0b0000, 0b1100, 0b1101],
    };
    let vcd = capture.to_vcd("dio");
    let stamps: Vec<&str> = vcd.lines().filter(|l| l.starts_with('#')).collect();
    assert_eq!(
        stamps,
        vec!["#0", "#2"],
        "a change confined to unnamed bits is not a change"
    );
}

#[test]
fn empty_capture_still_parses() {
    let capture = LogicCapture {
        channel_names: names(2),
        sample_rate_hz: 1_000_000.0,
        samples: vec![],
    };
    let waveform = round_trip(&capture, "dio");
    let signals = list_signals(waveform.hierarchy(), None, None, true, None);
    assert_eq!(signals, vec!["dio.dio0", "dio.dio1"]);
}
