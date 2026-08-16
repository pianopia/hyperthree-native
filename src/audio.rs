use anyhow::{anyhow, Result};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Source, SpatialSink};
use serde::Deserialize;
use std::f32::consts::PI;
use std::{collections::HashMap, io::Cursor, time::Duration};

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
    listener_position: [f32; 3],
    next_id: u64,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self {
            output_stream: None,
            output_handle: None,
            sinks: HashMap::new(),
            listener_position: [0.0, 0.0, 0.0],
            next_id: 1,
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
}

fn append_source<S>(
    sink: &SpatialSink,
    source: S,
    when: f64,
    offset: f64,
    duration: f64,
    filters: &[AudioFilter],
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
    sink.append(source.delay(delay).skip_duration(skip).take_duration(take));
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
}
