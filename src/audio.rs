use anyhow::{anyhow, Result};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
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
    sinks: HashMap<u64, Sink>,
    next_id: u64,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self {
            output_stream: None,
            output_handle: None,
            sinks: HashMap::new(),
            next_id: 1,
        }
    }
}

impl AudioEngine {
    pub fn play(
        &mut self,
        bytes: Vec<u8>,
        looped: bool,
        volume: f32,
        when: f64,
        offset: f64,
        duration: f64,
    ) -> Result<u64> {
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
        let sink = Sink::try_new(handle)
            .map_err(|error| anyhow!("failed to create native audio source: {error}"))?;
        sink.set_volume(volume.clamp(0.0, 4.0));
        let source_bytes = Cursor::new(bytes);
        if looped {
            append_source(
                &sink,
                Decoder::new_looped(source_bytes)
                    .map_err(|error| anyhow!("failed to decode looped audio: {error}"))?,
                when,
                offset,
                duration,
            );
        } else {
            append_source(
                &sink,
                Decoder::new(source_bytes)
                    .map_err(|error| anyhow!("failed to decode audio source: {error}"))?,
                when,
                offset,
                duration,
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
}

fn append_source<S>(sink: &Sink, source: S, when: f64, offset: f64, duration: f64)
where
    S: Source<Item = i16> + Send + 'static,
{
    let delay = seconds_to_duration(when);
    let skip = seconds_to_duration(offset);
    let take = if duration.is_finite() && duration > 0.0 {
        seconds_to_duration(duration)
    } else {
        Duration::MAX
    };
    sink.append(
        source
            .convert_samples::<f32>()
            .delay(delay)
            .skip_duration(skip)
            .take_duration(take),
    );
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
    use super::decode_audio;

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
}
