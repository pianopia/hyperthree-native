use anyhow::{anyhow, Result};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Source, SpatialSink};
use serde::Deserialize;
use std::f32::consts::PI;
use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
    sync::{Arc, Mutex},
    time::Duration,
};

const MAX_AUDIO_BYTES: usize = 64 * 1024 * 1024;

/// Decoded PCM metadata and channel-separated samples for the Web Audio
/// compatibility surface. Playback itself stays in the native mixer so large
/// buffers do not need to be copied back through JavaScript.
pub struct DecodedAudio {
    pub channels: Vec<Vec<f32>>,
    pub sample_rate: u32,
    pub length: usize,
}

pub struct AudioPlayback {
    pub looped: bool,
    pub volume: f32,
    pub when: f64,
    pub offset: f64,
    pub duration: f64,
    pub speed: f32,
    pub filters: Vec<AudioFilter>,
    pub analysers: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AudioFilter {
    #[serde(rename = "type")]
    pub kind: String,
    pub frequency: f32,
    pub q: f32,
    pub gain: f32,
    pub detune: f32,
}

const DEFAULT_ANALYSER_FFT_SIZE: usize = 2048;
const MAX_ANALYSER_FFT_SIZE: usize = 32768;

struct AnalyserState {
    fft_size: usize,
    smoothing_time_constant: f32,
    min_decibels: f32,
    max_decibels: f32,
    samples: VecDeque<f32>,
    frame_sum: f32,
    frame_channel: usize,
    channels: usize,
    previous_frequency: Vec<f32>,
}

impl Default for AnalyserState {
    fn default() -> Self {
        Self {
            fft_size: DEFAULT_ANALYSER_FFT_SIZE,
            smoothing_time_constant: 0.8,
            min_decibels: -100.0,
            max_decibels: -30.0,
            samples: VecDeque::with_capacity(MAX_ANALYSER_FFT_SIZE * 2),
            frame_sum: 0.0,
            frame_channel: 0,
            channels: 1,
            previous_frequency: vec![0.0; DEFAULT_ANALYSER_FFT_SIZE / 2],
        }
    }
}

impl AnalyserState {
    fn configure(
        &mut self,
        fft_size: usize,
        smoothing_time_constant: f32,
        min_decibels: f32,
        max_decibels: f32,
    ) {
        self.fft_size = sanitize_fft_size(fft_size);
        self.smoothing_time_constant = smoothing_time_constant.clamp(0.0, 1.0);
        self.min_decibels = min_decibels.min(max_decibels - 0.001);
        self.max_decibels = max_decibels.max(self.min_decibels + 0.001);
        self.previous_frequency.resize(self.fft_size / 2, 0.0);
    }

    fn push_sample(&mut self, sample: f32, channel: usize, channels: usize) {
        if channel == 0 {
            self.frame_sum = 0.0;
            self.channels = channels.max(1);
        }
        self.frame_sum += sample;
        if channel + 1 >= self.channels {
            self.samples
                .push_back(self.frame_sum / self.channels as f32);
            let max_samples = MAX_ANALYSER_FFT_SIZE * 2;
            while self.samples.len() > max_samples {
                self.samples.pop_front();
            }
            self.frame_channel = 0;
        } else {
            self.frame_channel = channel + 1;
        }
    }

    fn reset(&mut self) {
        self.samples.clear();
        self.frame_sum = 0.0;
        self.frame_channel = 0;
        self.previous_frequency.fill(0.0);
    }

    fn read_frequency(&mut self, output_len: usize) -> Vec<u8> {
        let magnitudes = self.fft_magnitudes();
        let mut output = vec![0; output_len];
        let smoothing = self.smoothing_time_constant;
        for (index, value) in magnitudes.into_iter().take(output_len).enumerate() {
            let smoothed = smoothing * self.previous_frequency[index] + (1.0 - smoothing) * value;
            self.previous_frequency[index] = smoothed;
            output[index] = decibel_to_byte(smoothed, self.min_decibels, self.max_decibels);
        }
        output
    }

    fn read_time_domain(&self, output_len: usize) -> Vec<u8> {
        let start = self.samples.len().saturating_sub(output_len);
        let values = self.samples.iter().skip(start);
        let mut output = vec![128; output_len];
        for (index, sample) in values.enumerate().take(output_len) {
            output[index] = (((sample.clamp(-1.0, 1.0) + 1.0) * 127.5).round()) as u8;
        }
        output
    }

    fn fft_magnitudes(&self) -> Vec<f32> {
        let size = self.fft_size;
        let mut values = vec![(0.0_f32, 0.0_f32); size];
        let start = self.samples.len().saturating_sub(size);
        for (index, sample) in self.samples.iter().skip(start).enumerate() {
            let window = 0.5 - 0.5 * (2.0 * PI * index as f32 / size as f32).cos();
            values[index].0 = sample * window;
        }
        let mut j = 0;
        for i in 1..size {
            let mut bit = size >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                values.swap(i, j);
            }
        }
        let mut length = 2;
        while length <= size {
            let angle = -2.0 * PI / length as f32;
            let (sin, cos) = angle.sin_cos();
            for offset in (0..size).step_by(length) {
                let mut wr = 1.0;
                let mut wi = 0.0;
                for index in 0..length / 2 {
                    let even = values[offset + index];
                    let odd = values[offset + index + length / 2];
                    let tr = wr * odd.0 - wi * odd.1;
                    let ti = wr * odd.1 + wi * odd.0;
                    values[offset + index] = (even.0 + tr, even.1 + ti);
                    values[offset + index + length / 2] = (even.0 - tr, even.1 - ti);
                    let next_wr = wr * cos - wi * sin;
                    wi = wr * sin + wi * cos;
                    wr = next_wr;
                }
            }
            length <<= 1;
        }
        values
            .into_iter()
            .take(size / 2)
            .map(|(real, imaginary)| {
                let magnitude = (real * real + imaginary * imaginary).sqrt() / size as f32;
                20.0 * magnitude.max(1.0e-12).log10()
            })
            .collect()
    }
}

fn sanitize_fft_size(value: usize) -> usize {
    value
        .clamp(32, MAX_ANALYSER_FFT_SIZE)
        .next_power_of_two()
        .min(MAX_ANALYSER_FFT_SIZE)
}

fn decibel_to_byte(value: f32, min_decibels: f32, max_decibels: f32) -> u8 {
    (((value - min_decibels) / (max_decibels - min_decibels)).clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn decode_audio(bytes: &[u8]) -> Result<DecodedAudio> {
    if bytes.len() > MAX_AUDIO_BYTES {
        return Err(anyhow!("audio payload exceeds {MAX_AUDIO_BYTES} bytes"));
    }
    let decoder = Decoder::new(Cursor::new(bytes.to_vec()))
        .map_err(|error| anyhow!("failed to decode audio: {error}"))?;
    let channel_count = decoder.channels() as usize;
    let sample_rate = decoder.sample_rate();
    if channel_count == 0 || sample_rate == 0 {
        return Err(anyhow!(
            "decoded audio has invalid channel or sample-rate metadata"
        ));
    }
    let samples: Vec<f32> = decoder.convert_samples().collect();
    let length = samples.len() / channel_count;
    let mut channels = vec![Vec::with_capacity(length); channel_count];
    for frame in samples.chunks_exact(channel_count) {
        for (channel, sample) in frame.iter().enumerate() {
            channels[channel].push(*sample);
        }
    }
    Ok(DecodedAudio {
        channels,
        sample_rate,
        length,
    })
}

/// Native output mixer shared by all Web Audio source nodes in one game
/// session. The output stream is lazy so headless tests can still decode audio
/// and exercise the API without requiring a physical audio device.
pub struct AudioEngine {
    output_stream: Option<OutputStream>,
    output_handle: Option<OutputStreamHandle>,
    sinks: HashMap<u64, SpatialSink>,
    analysers: HashMap<u64, Arc<Mutex<AnalyserState>>>,
    listener_position: [f32; 3],
    next_id: u64,
    next_analyser_id: u64,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self {
            output_stream: None,
            output_handle: None,
            sinks: HashMap::new(),
            analysers: HashMap::new(),
            listener_position: [0.0, 0.0, 0.0],
            next_id: 1,
            next_analyser_id: 1,
        }
    }
}

impl AudioEngine {
    pub fn play(&mut self, bytes: Vec<u8>, playback: AudioPlayback) -> Result<u64> {
        if bytes.len() > MAX_AUDIO_BYTES {
            return Err(anyhow!("audio payload exceeds {MAX_AUDIO_BYTES} bytes"));
        }
        if self.output_stream.is_none() {
            let (stream, handle) = OutputStream::try_default()
                .map_err(|error| anyhow!("failed to open the native audio output: {error}"))?;
            self.output_stream = Some(stream);
            self.output_handle = Some(handle);
        }
        let handle = self
            .output_handle
            .as_ref()
            .ok_or_else(|| anyhow!("native audio output is unavailable"))?;
        let sink = SpatialSink::try_new(
            handle,
            [0.0, 0.0, 0.0],
            [
                self.listener_position[0] - 0.5,
                self.listener_position[1],
                self.listener_position[2],
            ],
            [
                self.listener_position[0] + 0.5,
                self.listener_position[1],
                self.listener_position[2],
            ],
        )
        .map_err(|error| anyhow!("failed to create native audio source: {error}"))?;
        sink.set_volume(playback.volume.clamp(0.0, 4.0));
        sink.set_speed(sanitize_speed(playback.speed));
        let analyser_taps = playback
            .analysers
            .iter()
            .filter_map(|id| self.analysers.get(id).cloned())
            .collect::<Vec<_>>();
        let source_bytes = Cursor::new(bytes);
        if playback.looped {
            append_source(
                &sink,
                Decoder::new_looped(source_bytes)
                    .map_err(|error| anyhow!("failed to decode looped audio: {error}"))?,
                playback.when,
                playback.offset,
                playback.duration,
                &playback.filters,
                &analyser_taps,
            );
        } else {
            append_source(
                &sink,
                Decoder::new(source_bytes)
                    .map_err(|error| anyhow!("failed to decode audio source: {error}"))?,
                playback.when,
                playback.offset,
                playback.duration,
                &playback.filters,
                &analyser_taps,
            );
        }
        sink.play();
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.sinks.insert(id, sink);
        Ok(id)
    }

    pub fn stop(&mut self, id: u64) {
        if let Some(sink) = self.sinks.remove(&id) {
            sink.stop();
        }
    }

    pub fn pause(&mut self, id: u64) {
        if let Some(sink) = self.sinks.get(&id) {
            sink.pause();
        }
    }

    pub fn resume(&mut self, id: u64) {
        if let Some(sink) = self.sinks.get(&id) {
            sink.play();
        }
    }

    pub fn set_volume(&mut self, id: u64, volume: f32) {
        if let Some(sink) = self.sinks.get(&id) {
            sink.set_volume(volume.clamp(0.0, 4.0));
        }
    }

    pub fn set_speed(&mut self, id: u64, speed: f32) {
        if let Some(sink) = self.sinks.get(&id) {
            sink.set_speed(sanitize_speed(speed));
        }
    }

    pub fn set_emitter_position(&mut self, id: u64, position: [f32; 3]) {
        if let Some(sink) = self.sinks.get(&id) {
            sink.set_emitter_position(position);
        }
    }

    pub fn set_listener_position(&mut self, position: [f32; 3]) {
        self.listener_position = position;
        let left = [position[0] - 0.5, position[1], position[2]];
        let right = [position[0] + 0.5, position[1], position[2]];
        for sink in self.sinks.values() {
            sink.set_left_ear_position(left);
            sink.set_right_ear_position(right);
        }
    }

    pub fn create_analyser(&mut self) -> u64 {
        let id = self.next_analyser_id;
        self.next_analyser_id = self.next_analyser_id.wrapping_add(1).max(1);
        self.analysers
            .insert(id, Arc::new(Mutex::new(AnalyserState::default())));
        id
    }

    pub fn configure_analyser(
        &mut self,
        id: u64,
        fft_size: usize,
        smoothing_time_constant: f32,
        min_decibels: f32,
        max_decibels: f32,
    ) {
        if let Some(analyser) = self.analysers.get(&id) {
            if let Ok(mut analyser) = analyser.lock() {
                analyser.configure(
                    fft_size,
                    smoothing_time_constant,
                    min_decibels,
                    max_decibels,
                );
            }
        }
    }

    pub fn read_analyser_frequency(&self, id: u64, length: usize) -> Vec<u8> {
        self.analysers
            .get(&id)
            .and_then(|analyser| analyser.lock().ok())
            .map(|mut analyser| analyser.read_frequency(length))
            .unwrap_or_else(|| vec![0; length])
    }

    pub fn read_analyser_time_domain(&self, id: u64, length: usize) -> Vec<u8> {
        self.analysers
            .get(&id)
            .and_then(|analyser| analyser.lock().ok())
            .map(|analyser| analyser.read_time_domain(length))
            .unwrap_or_else(|| vec![128; length])
    }
}

fn append_source<S>(
    sink: &SpatialSink,
    source: S,
    when: f64,
    offset: f64,
    duration: f64,
    filters: &[AudioFilter],
    analysers: &[Arc<Mutex<AnalyserState>>],
) where
    S: Source<Item = i16> + Send + 'static,
{
    let delay = seconds_to_duration(when);
    let skip = seconds_to_duration(offset);
    let take = if duration.is_finite() && duration > 0.0 {
        seconds_to_duration(duration)
    } else {
        Duration::MAX
    };
    let source = FilterSource::new(source.convert_samples::<f32>(), filters);
    let source = source.delay(delay).skip_duration(skip).take_duration(take);
    sink.append(AnalyserTap::new(source, analysers));
}

struct FilterSource<S> {
    input: S,
    filters: Vec<Biquad>,
    channel: usize,
    channels: usize,
}

impl<S> FilterSource<S>
where
    S: Source<Item = f32>,
{
    fn new(input: S, filters: &[AudioFilter]) -> Self {
        let channels = input.channels().max(1) as usize;
        let sample_rate = input.sample_rate();
        let filters = filters
            .iter()
            .filter_map(|filter| Biquad::new(filter, sample_rate, channels))
            .collect();
        Self {
            input,
            filters,
            channel: 0,
            channels,
        }
    }

    fn reset(&mut self) {
        self.channel = 0;
        for filter in &mut self.filters {
            filter.reset();
        }
    }
}

impl<S> Iterator for FilterSource<S>
where
    S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let mut sample = self.input.next()?;
        for filter in &mut self.filters {
            sample = filter.process(self.channel, sample);
        }
        self.channel = (self.channel + 1) % self.channels;
        Some(sample)
    }
}

impl<S> Source for FilterSource<S>
where
    S: Source<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.input.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.input.try_seek(pos)?;
        self.reset();
        Ok(())
    }
}

struct AnalyserTap<S> {
    input: S,
    analysers: Vec<Arc<Mutex<AnalyserState>>>,
    channel: usize,
    channels: usize,
}

impl<S> AnalyserTap<S>
where
    S: Source<Item = f32>,
{
    fn new(input: S, analysers: &[Arc<Mutex<AnalyserState>>]) -> Self {
        Self {
            channels: input.channels().max(1) as usize,
            input,
            analysers: analysers.to_vec(),
            channel: 0,
        }
    }

    fn reset(&mut self) {
        self.channel = 0;
        for analyser in &self.analysers {
            if let Ok(mut analyser) = analyser.lock() {
                analyser.reset();
            }
        }
    }
}

impl<S> Iterator for AnalyserTap<S>
where
    S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.input.next()?;
        for analyser in &self.analysers {
            if let Ok(mut analyser) = analyser.lock() {
                analyser.push_sample(sample, self.channel, self.channels);
            }
        }
        self.channel = (self.channel + 1) % self.channels;
        Some(sample)
    }
}

impl<S> Source for AnalyserTap<S>
where
    S: Source<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.input.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.input.try_seek(pos)?;
        self.reset();
        Ok(())
    }
}

struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: Vec<f32>,
    x2: Vec<f32>,
    y1: Vec<f32>,
    y2: Vec<f32>,
}

impl Biquad {
    fn new(filter: &AudioFilter, sample_rate: u32, channels: usize) -> Option<Self> {
        let sample_rate = sample_rate as f32;
        if sample_rate <= 0.0 {
            return None;
        }
        let frequency = (filter.frequency * 2.0_f32.powf(filter.detune / 1200.0))
            .clamp(1.0, (sample_rate * 0.49).max(1.0));
        let q = filter.q.max(0.0001);
        let gain = filter.gain.clamp(-120.0, 120.0);
        let omega = 2.0 * PI * frequency / sample_rate;
        let sin = omega.sin();
        let cos = omega.cos();
        let alpha = sin / (2.0 * q);
        let a = 10.0_f32.powf(gain / 40.0);
        let beta = 2.0 * a.sqrt() * alpha;
        let (b0, b1, b2, a0, a1, a2) = match filter.kind.as_str() {
            "lowpass" => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            "highpass" => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            "bandpass" => (
                sin / 2.0,
                0.0,
                -sin / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            "notch" => (1.0, -2.0 * cos, 1.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            "allpass" => (
                1.0 - alpha,
                -2.0 * cos,
                1.0 + alpha,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            "peaking" => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            "lowshelf" => (
                a * ((a + 1.0) - (a - 1.0) * cos + beta),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - beta),
                (a + 1.0) + (a - 1.0) * cos + beta,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - beta,
            ),
            "highshelf" => (
                a * ((a + 1.0) + (a - 1.0) * cos + beta),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - beta),
                (a + 1.0) - (a - 1.0) * cos + beta,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - beta,
            ),
            _ => return None,
        };
        if !a0.is_finite() || a0.abs() < f32::EPSILON {
            return None;
        }
        Some(Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: vec![0.0; channels],
            x2: vec![0.0; channels],
            y1: vec![0.0; channels],
            y2: vec![0.0; channels],
        })
    }

    fn process(&mut self, channel: usize, sample: f32) -> f32 {
        let output = self.b0 * sample + self.b1 * self.x1[channel] + self.b2 * self.x2[channel]
            - self.a1 * self.y1[channel]
            - self.a2 * self.y2[channel];
        self.x2[channel] = self.x1[channel];
        self.x1[channel] = sample;
        self.y2[channel] = self.y1[channel];
        self.y1[channel] = output;
        output
    }

    fn reset(&mut self) {
        self.x1.fill(0.0);
        self.x2.fill(0.0);
        self.y1.fill(0.0);
        self.y2.fill(0.0);
    }
}

fn sanitize_speed(speed: f32) -> f32 {
    if speed.is_finite() {
        speed.clamp(0.01, 100.0)
    } else {
        1.0
    }
}

fn seconds_to_duration(seconds: f64) -> Duration {
    if !seconds.is_finite() || seconds <= 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(seconds.min(Duration::MAX.as_secs_f64()))
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_audio, AudioFilter, FilterSource};
    use rodio::buffer::SamplesBuffer;

    fn test_wav() -> Vec<u8> {
        let sample_rate = 8u32;
        let samples = [0i16, 8192, -8192, 0];
        let data_len = samples.len() * 2;
        let mut bytes = Vec::with_capacity(44 + data_len);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36u32 + data_len as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data_len as u32).to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn decodes_wav_to_channel_separated_float_samples() {
        let decoded = decode_audio(&test_wav()).unwrap();
        assert_eq!(decoded.sample_rate, 8);
        assert_eq!(decoded.length, 4);
        assert_eq!(decoded.channels.len(), 1);
        assert!((decoded.channels[0][1] - 0.25).abs() < 0.01);
        assert!((decoded.channels[0][2] + 0.25).abs() < 0.01);
    }

    #[test]
    fn applies_native_biquad_filter_chain_to_multichannel_samples() {
        let filter = AudioFilter {
            kind: "lowpass".to_string(),
            frequency: 400.0,
            q: 0.707,
            gain: 0.0,
            detune: 0.0,
        };
        let input = SamplesBuffer::new(2, 48_000, vec![1.0; 128]);
        let output: Vec<f32> = FilterSource::new(input, &[filter]).collect();
        assert_eq!(output.len(), 128);
        assert!(output.iter().all(|sample| sample.is_finite()));
        assert!(output.iter().any(|sample| *sample < 0.99));
    }

    #[test]
    fn supports_web_audio_biquad_filter_types() {
        for kind in [
            "lowpass",
            "highpass",
            "bandpass",
            "notch",
            "allpass",
            "peaking",
            "lowshelf",
            "highshelf",
        ] {
            let filter = AudioFilter {
                kind: kind.to_string(),
                frequency: 1_000.0,
                q: 1.0,
                gain: 6.0,
                detune: 0.0,
            };
            assert!(super::Biquad::new(&filter, 48_000, 2).is_some(), "{kind}");
        }
    }

    #[test]
    fn analyser_fft_and_time_domain_data_follow_native_samples() {
        let mut analyser = super::AnalyserState::default();
        analyser.configure(32, 0.0, -100.0, 0.0);
        for _ in 0..32 {
            analyser.push_sample(1.0, 0, 1);
        }
        let frequency = analyser.read_frequency(16);
        let time_domain = analyser.read_time_domain(32);
        assert_eq!(frequency.len(), 16);
        assert!(frequency.iter().any(|value| *value > 0));
        assert!(time_domain.iter().all(|value| *value == 255));
    }
}
