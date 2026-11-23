//! The purr sound played while the cat is petted.
//!
//! The purr is synthesised live: white noise goes through two band-pass filters (a warm chest
//! rumble at 190 Hz and a breathy throat tone at 520 Hz), is modulated by a 24.5 Hz flutter
//! and a slow 0.68 Hz breathing swell, then softened by a low-pass filter.
//!
//! Usage: call [`set_purr_intensity`] every frame with 0 (silent) to 1 (full purr), and
//! [`set_sound_enabled`] when the sound setting changes. The audio device is only opened the
//! first time a purr is needed.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

/// Whether sound is switched on in the settings.
static ENABLED: AtomicBool = AtomicBool::new(true);
/// Whether the audio thread has been started (successfully or not).
static STARTED: AtomicBool = AtomicBool::new(false);
/// Target master gain, as `f32` bits.
static TARGET_GAIN: AtomicU32 = AtomicU32::new(0);
/// Time constant for reaching the target gain in seconds, as `f32` bits.
static GAIN_TIME_CONSTANT: AtomicU32 = AtomicU32::new(0);
/// The last intensity that was applied (to skip tiny changes).
static CURRENT_INTENSITY: Mutex<f32> = Mutex::new(0.0);

/// Switches the purr sound on or off. Switching off silences a running purr.
///
/// @param enabled True to allow sound.
pub fn set_sound_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        set_purr_intensity(0.0);
    }
}

/// Smoothly changes how loud the cat purrs.
///
/// @param intensity 0 = silent, 1 = full purr. Values outside 0..1 are clamped.
pub fn set_purr_intensity(intensity: f32) {
    let enabled = ENABLED.load(Ordering::Relaxed);
    let target = if enabled { intensity.clamp(0.0, 1.0) } else { 0.0 };
    let started = STARTED.load(Ordering::Relaxed);
    if target <= 0.01 && !started {
        return;
    }

    {
        let mut current = match CURRENT_INTENSITY.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if (target - *current).abs() < 0.005 {
            return;
        }
        *current = target;
    }

    if target > 0.01 && !started {
        start_audio_thread();
    }

    let time_constant: f32 = if target > 0.01 { 0.14 } else { 0.22 };
    GAIN_TIME_CONSTANT.store(time_constant.to_bits(), Ordering::Relaxed);
    TARGET_GAIN.store((target * 0.26).to_bits(), Ordering::Relaxed);
}

/// Starts the thread that owns the audio stream. The stream is kept alive for the rest of the
/// program. Failing to open a device is not an error: the pet simply stays silent.
fn start_audio_thread() {
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("purr-audio".to_string())
        .spawn(run_audio_thread);
    if spawned.is_err() {
        eprintln!("[audio] could not start the audio thread");
    }
}

/// Body of the audio thread: opens the default output device, plays the purr stream and then
/// waits forever so the stream is never dropped.
fn run_audio_thread() {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        eprintln!("[audio] no output device");
        return;
    };
    let supported = match device.default_output_config() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("[audio] no output config: {error}");
            return;
        }
    };
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();

    let stream_result = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(&device, &config),
        SampleFormat::I16 => build_stream::<i16>(&device, &config),
        SampleFormat::U16 => build_stream::<u16>(&device, &config),
        SampleFormat::I32 => build_stream::<i32>(&device, &config),
        SampleFormat::F64 => build_stream::<f64>(&device, &config),
        other => Err(format!("unsupported sample format {other:?}")),
    };
    let stream = match stream_result {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("[audio] could not open stream: {error}");
            return;
        }
    };
    if let Err(error) = stream.play() {
        eprintln!("[audio] could not play stream: {error}");
        return;
    }
    loop {
        std::thread::park();
    }
}

/// Builds the output stream for one sample type.
///
/// @param device The output device.
/// @param config Sample rate and channel count.
/// @return The stream, or an error message.
fn build_stream<T>(device: &cpal::Device, config: &StreamConfig) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut synth = PurrSynth::new(config.sample_rate.0 as f32);
    let stream = device
        .build_output_stream(
            config,
            move |output: &mut [T], _info: &cpal::OutputCallbackInfo| {
                for frame in output.chunks_mut(channels.max(1)) {
                    let value = T::from_sample(synth.next_sample());
                    for sample in frame.iter_mut() {
                        *sample = value;
                    }
                }
            },
            |error| eprintln!("[audio] stream error: {error}"),
            None,
        )
        .map_err(|error| error.to_string())?;
    Ok(stream)
}

/// A second-order filter (the same formulas as the Web Audio `BiquadFilterNode`).
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    /// Creates a band-pass filter with 0 dB gain at the centre frequency.
    ///
    /// @param sample_rate Samples per second.
    /// @param frequency Centre frequency in Hz.
    /// @param q Quality factor.
    /// @return The filter.
    fn band_pass(sample_rate: f32, frequency: f32, q: f32) -> Biquad {
        let omega = 2.0 * PI * frequency / sample_rate;
        let alpha = omega.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Biquad {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: -2.0 * omega.cos() / a0,
            a2: (1.0 - alpha) / a0,
            ..Biquad::default()
        }
    }

    /// Creates a low-pass filter with the Web Audio default resonance (Q = 1 dB).
    ///
    /// @param sample_rate Samples per second.
    /// @param frequency Cut-off frequency in Hz.
    /// @return The filter.
    fn low_pass(sample_rate: f32, frequency: f32) -> Biquad {
        let q = 10f32.powf(1.0 / 20.0);
        let omega = 2.0 * PI * frequency / sample_rate;
        let alpha = omega.sin() / (2.0 * q);
        let cos = omega.cos();
        let a0 = 1.0 + alpha;
        Biquad {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            ..Biquad::default()
        }
    }

    /// Filters one sample.
    ///
    /// @param input The input sample.
    /// @return The output sample.
    fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }
}

/// The purr generator running on the audio thread.
struct PurrSynth {
    sample_rate: f32,
    noise_state: u32,
    warm_filter: Biquad,
    breath_filter: Biquad,
    polish_filter: Biquad,
    flutter_phase: f32,
    breath_phase: f32,
    master_gain: f32,
}

impl PurrSynth {
    /// @param sample_rate Samples per second.
    /// @return A silent synth.
    fn new(sample_rate: f32) -> PurrSynth {
        PurrSynth {
            sample_rate,
            noise_state: 0x1234_5678,
            warm_filter: Biquad::band_pass(sample_rate, 190.0, 1.15),
            breath_filter: Biquad::band_pass(sample_rate, 520.0, 1.4),
            polish_filter: Biquad::low_pass(sample_rate, 1400.0),
            flutter_phase: 0.0,
            breath_phase: 0.0,
            master_gain: 0.0,
        }
    }

    /// Returns white noise in -1..1.
    ///
    /// @return One noise sample.
    fn noise(&mut self) -> f32 {
        let mut value = self.noise_state;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.noise_state = value;
        (value as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Produces the next output sample.
    ///
    /// @return One sample in roughly -1..1.
    fn next_sample(&mut self) -> f32 {
        let target = f32::from_bits(TARGET_GAIN.load(Ordering::Relaxed));
        let time_constant = f32::from_bits(GAIN_TIME_CONSTANT.load(Ordering::Relaxed)).max(0.001);
        let step = 1.0 - (-1.0 / (time_constant * self.sample_rate)).exp();
        self.master_gain += (target - self.master_gain) * step;
        if self.master_gain < 0.00001 && target <= 0.0 {
            self.master_gain = 0.0;
        }

        let noise = self.noise();
        let warm = self.warm_filter.process(noise) * 1.45;
        let breath = self.breath_filter.process(noise) * 0.34;

        let flutter = 0.65 + 0.35 * (self.flutter_phase * 2.0 * PI).sin();
        let breathing = 0.76 + 0.24 * (self.breath_phase * 2.0 * PI).sin();
        self.flutter_phase = (self.flutter_phase + 24.5 / self.sample_rate).fract();
        self.breath_phase = (self.breath_phase + 0.68 / self.sample_rate).fract();

        let shaped = self.polish_filter.process((warm + breath) * flutter * breathing);
        shaped * self.master_gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_pass_passes_centre_and_blocks_dc() {
        let mut filter = Biquad::band_pass(48000.0, 190.0, 1.15);
        let mut output = 0.0;
        for _ in 0..48000 {
            output = filter.process(1.0);
        }
        assert!(output.abs() < 0.001);
    }

    #[test]
    fn synth_output_stays_bounded() {
        let mut synth = PurrSynth::new(48000.0);
        synth.master_gain = 0.26;
        for _ in 0..48000 {
            let value = synth.next_sample();
            assert!(value.is_finite() && value.abs() < 2.0);
        }
    }
}
