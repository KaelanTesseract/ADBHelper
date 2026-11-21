//! Live screen stream for time-critical minigames.
//!
//! A full `screencap` takes about half a second, far too slow for the fishing
//! pull. Like ADB AutoPlayer, ADBHelper reads `screenrecord`'s H.264 stream and
//! decodes it continuously. Decoding uses the H.264 decoder built into Windows
//! (Media Foundation), so no codec library is shipped with the app.

use image::RgbaImage;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// One decoded frame in NV12 layout: a full-size Y plane, then interleaved UV
/// at half resolution, both with the same row stride.
pub(in crate::afk_journey) struct Nv12 {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub data: Vec<u8>,
}

impl Nv12 {
    /// RGBA using BT.601 limited range, the default of ffmpeg that ADB
    /// AutoPlayer's colour thresholds were tuned with.
    pub fn to_rgba(&self, width: u32, height: u32) -> RgbaImage {
        let (width, height) = (width.min(self.width), height.min(self.height));
        let stride = self.stride as usize;
        let uv_start = stride * self.height as usize;
        let mut image = RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let luma = f32::from(self.data[y as usize * stride + x as usize]);
            let uv = uv_start + (y as usize / 2) * stride + (x as usize / 2) * 2;
            let (u, v) = (
                f32::from(self.data[uv]) - 128.0,
                f32::from(self.data[uv + 1]) - 128.0,
            );
            let c = 1.164 * (luma - 16.0);
            let clamp = |value: f32| value.round().clamp(0.0, 255.0) as u8;
            pixel.0 = [
                clamp(c + 1.596 * v),
                clamp(c - 0.392 * u - 0.813 * v),
                clamp(c + 2.017 * u),
                255,
            ];
        }
        image
    }
}

/// Splits an Annex B byte stream into NAL units, each with its start code.
#[derive(Default)]
pub(in crate::afk_journey) struct NalSplitter {
    pending: Vec<u8>,
}

impl NalSplitter {
    pub fn push(&mut self, bytes: &[u8], mut unit: impl FnMut(&[u8])) {
        self.pending.extend_from_slice(bytes);
        let starts = start_codes(&self.pending);
        // Every unit but the last is complete; the last may still grow.
        for pair in starts.windows(2) {
            unit(&self.pending[pair[0]..pair[1]]);
        }
        if let Some(&last) = starts.last() {
            self.pending.drain(..last);
        } else if self.pending.len() > 3 {
            // No start code yet: keep only bytes that may begin one.
            let keep = self.pending.len() - 3;
            self.pending.drain(..keep);
        }
    }
}

/// Offsets of `00 00 01` and `00 00 00 01` start codes (at the first zero).
fn start_codes(bytes: &[u8]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        if bytes[index] == 0 && bytes[index + 1] == 0 && bytes[index + 2] == 1 {
            let begin = if index > 0 && bytes[index - 1] == 0 {
                index - 1
            } else {
                index
            };
            if starts.last() != Some(&begin) {
                starts.push(begin);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    starts
}

struct Latest {
    frame: Option<(u64, Arc<Nv12>)>,
}

/// Background stream; the newest decoded frame replaces the previous one.
pub(in crate::afk_journey) struct ScreenStream {
    latest: Arc<(Mutex<Latest>, Condvar)>,
    running: Arc<AtomicBool>,
    size: (u32, u32),
}

impl ScreenStream {
    /// Starts streaming and waits up to `first_frame` for a picture.
    pub fn start(serial: &str, size: (u32, u32), first_frame: Duration) -> Option<Self> {
        let latest = Arc::new((Mutex::new(Latest { frame: None }), Condvar::new()));
        let running = Arc::new(AtomicBool::new(true));
        let stream = Self {
            latest: latest.clone(),
            running: running.clone(),
            size,
        };
        let serial = serial.to_owned();
        std::thread::Builder::new()
            .name("adbhelper-stream".into())
            .spawn(move || platform::run(&serial, size, &running, &latest))
            .ok()?;
        stream.next_after(0, first_frame).map(|_| stream)
    }

    /// The first frame newer than `seen`, waiting up to `timeout`; returns the
    /// frame number with an RGBA picture at the device size.
    pub fn next_after(&self, seen: u64, timeout: Duration) -> Option<(u64, RgbaImage)> {
        let deadline = Instant::now() + timeout;
        let (lock, ready) = &*self.latest;
        let mut latest = lock.lock().ok()?;
        loop {
            if let Some((number, frame)) = latest.frame.as_ref().filter(|(n, _)| *n > seen) {
                let (number, frame) = (*number, frame.clone());
                drop(latest);
                return Some((number, frame.to_rgba(self.size.0, self.size.1)));
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            latest = ready.wait_timeout(latest, left).ok()?.0;
        }
    }
}

impl Drop for ScreenStream {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

fn publish(latest: &(Mutex<Latest>, Condvar), frame: Nv12) {
    let (lock, ready) = latest;
    if let Ok(mut latest) = lock.lock() {
        let number = latest.frame.as_ref().map_or(0, |(n, _)| *n) + 1;
        latest.frame = Some((number, Arc::new(frame)));
        ready.notify_all();
    }
}

#[cfg(windows)]
mod platform {
    use super::{publish, Latest, NalSplitter, Nv12};
    use std::io::Read;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Condvar, Mutex};
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };

    /// The Windows H.264 decoder transform, fed one NAL unit at a time.
    struct Decoder {
        transform: IMFTransform,
        width: u32,
        height: u32,
        stride: u32,
        time: i64,
    }

    impl Decoder {
        fn new(size: (u32, u32)) -> windows::core::Result<Self> {
            unsafe {
                let transform: IMFTransform =
                    CoCreateInstance(&CLSID_MSH264DecoderMFT, None, CLSCTX_INPROC_SERVER)?;
                if let Ok(attributes) = transform.GetAttributes() {
                    let _ = attributes.SetUINT32(&MF_LOW_LATENCY, 1);
                }
                let input = MFCreateMediaType()?;
                input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
                input.SetUINT64(
                    &MF_MT_FRAME_SIZE,
                    (u64::from(size.0) << 32) | u64::from(size.1),
                )?;
                transform.SetInputType(0, &input, 0)?;
                let mut decoder = Self {
                    transform,
                    width: size.0,
                    height: size.1,
                    stride: size.0,
                    time: 0,
                };
                decoder.choose_nv12()?;
                decoder
                    .transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
                decoder
                    .transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
                Ok(decoder)
            }
        }

        /// Selects NV12 output and reads the decoded frame size and stride.
        fn choose_nv12(&mut self) -> windows::core::Result<()> {
            unsafe {
                for index in 0.. {
                    let candidate = self.transform.GetOutputAvailableType(0, index)?;
                    if candidate.GetGUID(&MF_MT_SUBTYPE)? == MFVideoFormat_NV12 {
                        self.transform.SetOutputType(0, &candidate, 0)?;
                        let size = candidate.GetUINT64(&MF_MT_FRAME_SIZE)?;
                        self.width = (size >> 32) as u32;
                        self.height = size as u32;
                        self.stride = candidate
                            .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                            .map_or(self.width, |stride| stride.max(self.width));
                        return Ok(());
                    }
                }
                Err(MF_E_INVALIDMEDIATYPE.into())
            }
        }

        fn feed(&mut self, unit: &[u8], mut frame: impl FnMut(Nv12)) -> windows::core::Result<()> {
            unsafe {
                let buffer = MFCreateMemoryBuffer(unit.len() as u32)?;
                let mut target = std::ptr::null_mut();
                buffer.Lock(&mut target, None, None)?;
                std::ptr::copy_nonoverlapping(unit.as_ptr(), target, unit.len());
                buffer.Unlock()?;
                buffer.SetCurrentLength(unit.len() as u32)?;
                let sample = MFCreateSample()?;
                sample.AddBuffer(&buffer)?;
                sample.SetSampleTime(self.time)?;
                self.time += 333_333;
                loop {
                    match self.transform.ProcessInput(0, &sample, 0) {
                        Ok(()) => break,
                        Err(error) if error.code() == MF_E_NOTACCEPTING => {
                            self.drain(&mut frame)?
                        }
                        Err(error) => return Err(error),
                    }
                }
                self.drain(&mut frame)
            }
        }

        fn drain(&mut self, frame: &mut impl FnMut(Nv12)) -> windows::core::Result<()> {
            unsafe {
                loop {
                    let info = self.transform.GetOutputStreamInfo(0)?;
                    let provides = info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
                    let own = if provides {
                        None
                    } else {
                        let sample = MFCreateSample()?;
                        sample.AddBuffer(&MFCreateMemoryBuffer(info.cbSize.max(1))?)?;
                        Some(sample)
                    };
                    let mut output = [MFT_OUTPUT_DATA_BUFFER {
                        dwStreamID: 0,
                        pSample: std::mem::ManuallyDrop::new(own),
                        dwStatus: 0,
                        pEvents: std::mem::ManuallyDrop::new(None),
                    }];
                    let mut status = 0;
                    let result = self.transform.ProcessOutput(0, &mut output, &mut status);
                    let sample = std::mem::ManuallyDrop::take(&mut output[0].pSample);
                    drop(std::mem::ManuallyDrop::take(&mut output[0].pEvents));
                    match result {
                        Ok(()) => {
                            if let Some(sample) = sample {
                                frame(self.copy(&sample)?);
                            }
                        }
                        Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => {
                            return Ok(())
                        }
                        Err(error) if error.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                            self.choose_nv12()?
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
        }

        fn copy(&self, sample: &IMFSample) -> windows::core::Result<Nv12> {
            unsafe {
                let buffer = sample.ConvertToContiguousBuffer()?;
                let mut source = std::ptr::null_mut();
                let mut length = 0;
                buffer.Lock(&mut source, None, Some(&mut length))?;
                let data = std::slice::from_raw_parts(source, length as usize).to_vec();
                buffer.Unlock()?;
                // Some decoders pad the height to a multiple of 16 rows.
                let rows = (data.len() as u64 * 2 / 3 / u64::from(self.stride)) as u32;
                Ok(Nv12 {
                    width: self.width,
                    height: rows.max(self.height).min(self.height.next_multiple_of(16)),
                    stride: self.stride,
                    data,
                })
            }
        }
    }

    pub(super) fn run(
        serial: &str,
        size: (u32, u32),
        running: &AtomicBool,
        latest: &(Mutex<Latest>, Condvar),
    ) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        }
        // screenrecord stops after three minutes; start it again while needed.
        while running.load(Ordering::Relaxed) {
            let Ok(mut decoder) = Decoder::new(size) else {
                return;
            };
            let Ok(mut child) = crate::adb::screen_stream(serial, size) else {
                return;
            };
            let Some(mut output) = child.stdout.take() else {
                return;
            };
            let mut splitter = NalSplitter::default();
            let mut buffer = vec![0_u8; 64 * 1024];
            while running.load(Ordering::Relaxed) {
                let Ok(count) = output.read(&mut buffer) else {
                    break;
                };
                if count == 0 {
                    break;
                }
                let mut failed = false;
                splitter.push(&buffer[..count], |unit| {
                    if !failed && decoder.feed(unit, |frame| publish(latest, frame)).is_err() {
                        failed = true;
                    }
                });
                if failed {
                    break;
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Latest;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Condvar, Mutex};

    /// No decoder outside Windows; callers fall back to screenshots.
    pub(super) fn run(_: &str, _: (u32, u32), _: &AtomicBool, _: &(Mutex<Latest>, Condvar)) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nal_units_are_split_at_both_start_code_lengths_across_chunks() {
        let stream = [
            0, 0, 0, 1, 0x67, 1, 2, // SPS
            0, 0, 1, 0x68, 3, // PPS
            0, 0, 0, 1, 0x65, 4, 5, 6, // IDR slice
            0, 0, 1, 0x41, // next slice, still open
        ];
        let mut units = Vec::new();
        let mut splitter = NalSplitter::default();
        for chunk in stream.chunks(3) {
            splitter.push(chunk, |unit| units.push(unit.to_vec()));
        }
        assert_eq!(
            units,
            vec![
                vec![0, 0, 0, 1, 0x67, 1, 2],
                vec![0, 0, 1, 0x68, 3],
                vec![0, 0, 0, 1, 0x65, 4, 5, 6],
            ]
        );
    }

    #[test]
    #[ignore = "streams the connected emulator for five seconds (no input)"]
    fn connected_emulator_stream_rate() {
        let serial = std::env::var("ADBHELPER_TEST_SERIAL").unwrap();
        let size = crate::adb::display_size(&serial).unwrap();
        let started = Instant::now();
        let stream =
            ScreenStream::start(&serial, size, Duration::from_secs(5)).expect("first frame");
        println!("first frame after {:?}", started.elapsed());
        let (mut seen, mut frames, begin) = (0, 0, Instant::now());
        let mut last = None;
        while begin.elapsed() < Duration::from_secs(5) {
            if let Some((number, image)) = stream.next_after(seen, Duration::from_millis(500)) {
                seen = number;
                frames += 1;
                last = Some(image);
            }
        }
        println!("{frames} frames in 5 s, newest number {seen}");
        if let (Some(image), Ok(path)) = (last, std::env::var("ADBHELPER_STREAM_FRAME")) {
            image.save(path).unwrap();
        }
    }

    #[test]
    fn nv12_converts_with_bt601_limited_range() {
        // 2 x 2 pixels: Y plane (stride 2), then one UV pair.
        let frame = Nv12 {
            width: 2,
            height: 2,
            stride: 2,
            data: vec![16, 235, 81, 145, 128, 128],
        };
        let image = frame.to_rgba(2, 2);
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(1, 0).0, [255, 255, 255, 255]);
        let grey = image.get_pixel(0, 1).0;
        assert!(grey[0] == grey[1] && grey[1] == grey[2], "{grey:?}");
    }
}
