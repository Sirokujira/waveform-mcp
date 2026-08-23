//! Logic-analyzer capture from a Digilent device via the WaveForms SDK.
//!
//! The published binding crates cover the analog instruments only, so the
//! handful of `FDwfDigitalIn*` entry points this needs are declared here.
//! Everything is compiled behind the `ad3` feature because linking requires
//! the proprietary WaveForms SDK to be installed.

use crate::capture::LogicCapture;
use std::ffi::{CStr, c_char, c_double, c_int, c_uchar, c_void};

type Hdwf = c_int;

const HDWF_NONE: Hdwf = 0;

// Acquisition finished; see FDwfDigitalInStatus in dwf.h.
const STS_DONE: c_uchar = 2;

#[link(name = "dwf")]
unsafe extern "C" {
    fn FDwfGetLastErrorMsg(szError: *mut c_char) -> c_int;

    fn FDwfEnum(enumfilter: c_int, pcDevice: *mut c_int) -> c_int;
    fn FDwfEnumDeviceName(idxDevice: c_int, szDeviceName: *mut c_char) -> c_int;
    fn FDwfEnumSN(idxDevice: c_int, szSN: *mut c_char) -> c_int;

    fn FDwfDeviceOpen(idxDevice: c_int, phdwf: *mut Hdwf) -> c_int;
    fn FDwfDeviceClose(hdwf: Hdwf) -> c_int;

    fn FDwfDigitalInReset(hdwf: Hdwf) -> c_int;
    fn FDwfDigitalInInternalClockInfo(hdwf: Hdwf, phzFreq: *mut c_double) -> c_int;
    fn FDwfDigitalInDividerSet(hdwf: Hdwf, div: c_int) -> c_int;
    fn FDwfDigitalInBitsInfo(hdwf: Hdwf, pnBits: *mut c_int) -> c_int;
    fn FDwfDigitalInSampleFormatSet(hdwf: Hdwf, nBits: c_int) -> c_int;
    fn FDwfDigitalInBufferSizeInfo(hdwf: Hdwf, pnSizeMax: *mut c_int) -> c_int;
    fn FDwfDigitalInBufferSizeSet(hdwf: Hdwf, nSize: c_int) -> c_int;
    fn FDwfDigitalInConfigure(hdwf: Hdwf, fReconfigure: c_int, fStart: c_int) -> c_int;
    fn FDwfDigitalInStatus(hdwf: Hdwf, fReadData: c_int, psts: *mut c_uchar) -> c_int;
    fn FDwfDigitalInStatusData(hdwf: Hdwf, rgData: *mut c_void, countOfDataBytes: c_int) -> c_int;
}

/// The SDK reports failures through a thread-local message rather than through
/// return codes, so every wrapper funnels through here.
fn last_error() -> String {
    // dwf.h fixes the caller-supplied buffer at 512 bytes.
    let mut buf = [0 as c_char; 512];
    unsafe {
        if FDwfGetLastErrorMsg(buf.as_mut_ptr()) == 0 {
            return "unknown WaveForms SDK error".to_string();
        }
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
    }
}

/// SDK calls return 0 on failure and leave the reason in the error message.
fn check(ok: c_int, what: &str) -> Result<(), String> {
    if ok == 0 {
        Err(format!("{} failed: {}", what, last_error()))
    } else {
        Ok(())
    }
}

/// Read one of the SDK's fixed 32-byte identification strings.
fn read_name(fill: impl FnOnce(*mut c_char) -> c_int, what: &str) -> Result<String, String> {
    let mut buf = [0 as c_char; 32];
    check(fill(buf.as_mut_ptr()), what)?;
    Ok(unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .trim()
        .to_string())
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub index: i32,
    pub name: String,
    pub serial: String,
}

/// Enumerate every connected Digilent device.
pub fn list_devices() -> Result<Vec<DeviceInfo>, String> {
    let mut count: c_int = 0;
    check(unsafe { FDwfEnum(0, &mut count) }, "FDwfEnum")?;

    let mut devices = Vec::with_capacity(count.max(0) as usize);
    for index in 0..count {
        devices.push(DeviceInfo {
            index,
            name: read_name(
                |p| unsafe { FDwfEnumDeviceName(index, p) },
                "FDwfEnumDeviceName",
            )?,
            serial: read_name(|p| unsafe { FDwfEnumSN(index, p) }, "FDwfEnumSN")?,
        });
    }
    Ok(devices)
}

/// Closes the device even if capture fails partway through.
struct OpenDevice(Hdwf);

impl Drop for OpenDevice {
    fn drop(&mut self) {
        unsafe { FDwfDeviceClose(self.0) };
    }
}

pub struct CaptureRequest {
    pub device_index: i32,
    /// Requested sampling rate; the device clock is divided to the nearest
    /// achievable rate, which the result reports back.
    pub sample_rate_hz: f64,
    pub sample_count: usize,
    /// Channel names, least significant DIO first. Length sets how many
    /// channels are recorded.
    pub channel_names: Vec<String>,
}

/// Run one buffered acquisition and return it as a capture.
///
/// Blocks until the device reports the acquisition done. `poll` is called
/// between status reads so the caller can yield or time out.
pub fn capture_logic(
    request: &CaptureRequest,
    mut poll: impl FnMut() -> Result<(), String>,
) -> Result<LogicCapture, String> {
    if request.channel_names.is_empty() {
        return Err("at least one channel name is required".to_string());
    }
    if request.sample_count == 0 {
        return Err("sample_count must be greater than zero".to_string());
    }
    if request.sample_rate_hz.is_nan() || request.sample_rate_hz <= 0.0 {
        return Err("sample_rate_hz must be greater than zero".to_string());
    }

    let mut hdwf: Hdwf = HDWF_NONE;
    check(
        unsafe { FDwfDeviceOpen(request.device_index, &mut hdwf) },
        "FDwfDeviceOpen",
    )?;
    let device = OpenDevice(hdwf);
    check(
        unsafe { FDwfDigitalInReset(device.0) },
        "FDwfDigitalInReset",
    )?;

    // The device samples at its internal clock divided by an integer, so the
    // achieved rate is rarely exactly what was asked for.
    let mut clock_hz: c_double = 0.0;
    check(
        unsafe { FDwfDigitalInInternalClockInfo(device.0, &mut clock_hz) },
        "FDwfDigitalInInternalClockInfo",
    )?;
    let divider = (clock_hz / request.sample_rate_hz).round().max(1.0);
    let achieved_rate = clock_hz / divider;
    check(
        unsafe { FDwfDigitalInDividerSet(device.0, divider as c_int) },
        "FDwfDigitalInDividerSet",
    )?;

    // Sample width is 8, 16 or 32 bits; pick the narrowest that holds the
    // requested channels, and refuse channels the device does not have.
    let mut available_bits: c_int = 0;
    check(
        unsafe { FDwfDigitalInBitsInfo(device.0, &mut available_bits) },
        "FDwfDigitalInBitsInfo",
    )?;
    let wanted = request.channel_names.len();
    if wanted > available_bits as usize {
        return Err(format!(
            "device exposes {} digital channels but {} were requested",
            available_bits, wanted
        ));
    }
    let sample_bits: c_int = if wanted <= 8 {
        8
    } else if wanted <= 16 {
        16
    } else {
        32
    };
    check(
        unsafe { FDwfDigitalInSampleFormatSet(device.0, sample_bits) },
        "FDwfDigitalInSampleFormatSet",
    )?;

    let mut max_samples: c_int = 0;
    check(
        unsafe { FDwfDigitalInBufferSizeInfo(device.0, &mut max_samples) },
        "FDwfDigitalInBufferSizeInfo",
    )?;
    if request.sample_count > max_samples as usize {
        return Err(format!(
            "device buffer holds {} samples but {} were requested",
            max_samples, request.sample_count
        ));
    }
    check(
        unsafe { FDwfDigitalInBufferSizeSet(device.0, request.sample_count as c_int) },
        "FDwfDigitalInBufferSizeSet",
    )?;

    check(
        unsafe { FDwfDigitalInConfigure(device.0, 0, 1) },
        "FDwfDigitalInConfigure",
    )?;

    loop {
        let mut status: c_uchar = 0;
        check(
            unsafe { FDwfDigitalInStatus(device.0, 1, &mut status) },
            "FDwfDigitalInStatus",
        )?;
        if status == STS_DONE {
            break;
        }
        poll()?;
    }

    let samples = read_samples(device.0, sample_bits, request.sample_count)?;

    Ok(LogicCapture {
        channel_names: request.channel_names.clone(),
        sample_rate_hz: achieved_rate,
        samples,
    })
}

/// Copy the acquisition buffer out and widen it to one `u32` per sample.
fn read_samples(hdwf: Hdwf, sample_bits: c_int, sample_count: usize) -> Result<Vec<u32>, String> {
    let bytes_per_sample = (sample_bits / 8) as usize;
    let mut raw = vec![0u8; sample_count * bytes_per_sample];
    check(
        unsafe {
            FDwfDigitalInStatusData(hdwf, raw.as_mut_ptr() as *mut c_void, raw.len() as c_int)
        },
        "FDwfDigitalInStatusData",
    )?;

    Ok(raw
        .chunks_exact(bytes_per_sample)
        .map(|chunk| match chunk {
            [b0] => *b0 as u32,
            [b0, b1] => u16::from_le_bytes([*b0, *b1]) as u32,
            [b0, b1, b2, b3] => u32::from_le_bytes([*b0, *b1, *b2, *b3]),
            _ => unreachable!("sample width is 1, 2 or 4 bytes"),
        })
        .collect())
}
