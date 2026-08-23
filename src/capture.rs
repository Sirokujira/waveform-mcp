//! Turning a logic-analyzer capture into a VCD file.
//!
//! Acquisition hardware hands back a flat buffer of samples taken at a fixed
//! rate, one bit per channel. The rest of this crate reads waveforms through
//! wellen, so a capture becomes useful by being written out as VCD and read
//! back like any other file.

use std::fmt::Write as _;

/// Time units a VCD `$timescale` can be written in, coarsest first, each in
/// femtoseconds. VCD only allows a magnitude of 1, 10 or 100, so the unit is
/// always written with a magnitude of 1 and the period is carried as a tick
/// count instead.
const UNITS: [(&str, u128); 6] = [
    ("s", 1_000_000_000_000_000),
    ("ms", 1_000_000_000_000),
    ("us", 1_000_000_000),
    ("ns", 1_000_000),
    ("ps", 1_000),
    ("fs", 1),
];

/// One sample per element, bit `i` carrying channel `i`.
#[derive(Debug, Clone)]
pub struct LogicCapture {
    /// Channel names, least significant bit first.
    pub channel_names: Vec<String>,
    pub sample_rate_hz: f64,
    pub samples: Vec<u32>,
}

/// A `$timescale` unit and the number of those units between two samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Timescale {
    unit: &'static str,
    ticks_per_sample: u128,
}

/// Pick the coarsest unit that still expresses the sample period as a whole
/// number of ticks, so a 100 MHz capture reads in `ns` rather than in `fs`.
fn choose_timescale(sample_rate_hz: f64) -> Timescale {
    // Femtoseconds keep every rate the hardware offers integral.
    let period_fs = if sample_rate_hz > 0.0 {
        (1.0e15 / sample_rate_hz).round() as u128
    } else {
        1_000_000 // fall back to 1 GHz rather than divide by zero
    };

    for (unit, fs_per_unit) in UNITS {
        if period_fs % fs_per_unit == 0 {
            return Timescale {
                unit,
                ticks_per_sample: period_fs / fs_per_unit,
            };
        }
    }
    // The last unit is a single femtosecond, so the loop always returns.
    unreachable!("fs divides every whole femtosecond period")
}

/// VCD identifiers are printable ASCII. `"` is skipped so the output stays
/// easy to quote in shells and test fixtures.
fn identifier(index: usize) -> String {
    const FIRST: u8 = b'!';
    const LAST: u8 = b'~';
    const SPAN: usize = (LAST - FIRST) as usize; // one short: '"' is excluded

    let mut out = String::new();
    let mut n = index;
    loop {
        let mut c = FIRST + (n % SPAN) as u8;
        if c >= b'"' {
            c += 1;
        }
        out.push(c as char);
        n /= SPAN;
        if n == 0 {
            break;
        }
        n -= 1;
    }
    out
}

impl LogicCapture {
    /// Serialize as VCD, emitting a timestamp only where some channel changed.
    pub fn to_vcd(&self, scope: &str) -> String {
        let ts = choose_timescale(self.sample_rate_hz);
        let ids: Vec<String> = (0..self.channel_names.len()).map(identifier).collect();

        let mut out = String::new();
        writeln!(out, "$version waveform-mcp logic capture $end").unwrap();
        writeln!(out, "$timescale 1{} $end", ts.unit).unwrap();
        writeln!(out, "$scope module {} $end", scope).unwrap();
        for (name, id) in self.channel_names.iter().zip(&ids) {
            writeln!(out, "$var wire 1 {} {} $end", id, name).unwrap();
        }
        writeln!(out, "$upscope $end").unwrap();
        writeln!(out, "$enddefinitions $end").unwrap();

        let mask = self.channel_mask();
        let mut prev: Option<u32> = None;
        for (i, raw) in self.samples.iter().enumerate() {
            let sample = raw & mask;
            let changed = match prev {
                None => mask,
                Some(p) => sample ^ p,
            };
            if changed == 0 {
                continue;
            }
            writeln!(out, "#{}", i as u128 * ts.ticks_per_sample).unwrap();
            for (bit, id) in ids.iter().enumerate() {
                if changed & (1 << bit) != 0 {
                    writeln!(out, "{}{}", (sample >> bit) & 1, id).unwrap();
                }
            }
            prev = Some(sample);
        }
        out
    }

    /// Bits that belong to a named channel; samples carry junk above them.
    fn channel_mask(&self) -> u32 {
        match self.channel_names.len() {
            0 => 0,
            n if n >= 32 => u32::MAX,
            n => (1u32 << n) - 1,
        }
    }
}
