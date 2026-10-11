//! Native microphone capture. Only completed recordings become ACP attachments.
use crate::config::{ChatAudio, ChatFile};
use base64::{Engine, engine::general_purpose::STANDARD};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    path::Path,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

const MAX_SECONDS: u32 = 120;
const MAX_AUDIO_BYTES: u64 = 32 * 1024 * 1024;

pub struct Recording {
    pub agent_id: u64,
    pub started: Instant,
    pub stopping: bool,
    pub displayed_seconds: u64,
    stop: mpsc::Sender<()>,
    result: mpsc::Receiver<Result<ChatFile, String>>,
}

impl Recording {
    pub fn start(agent_id: u64) -> Self {
        let (stop, stopped) = mpsc::channel();
        let (finished, result) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = finished.send(capture(stopped));
        });
        Self {
            agent_id,
            started: Instant::now(),
            stopping: false,
            displayed_seconds: 0,
            stop,
            result,
        }
    }

    pub fn stop(&mut self) {
        self.stopping = true;
        let _ = self.stop.send(());
    }

    pub fn poll(&self) -> Option<Result<ChatFile, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Voice recorder stopped unexpectedly.".into()))
            }
        }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

#[derive(Default)]
struct Capture {
    samples: Vec<i16>,
    error: Option<String>,
}

fn input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    state: Arc<Mutex<Capture>>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let errors = state.clone();
    let channels = usize::from(config.channels);
    let limit = config.sample_rate as usize * MAX_SECONDS as usize;
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let mut state = state.lock().unwrap();
                let remaining = limit.saturating_sub(state.samples.len());
                for frame in data.chunks_exact(channels).take(remaining) {
                    let mono = frame
                        .iter()
                        .map(|sample| sample.to_sample::<f32>())
                        .sum::<f32>()
                        / channels as f32;
                    state
                        .samples
                        .push((mono.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16);
                }
            },
            move |error| {
                errors.lock().unwrap().error = Some(format!("Microphone recording failed: {error}"))
            },
            None,
        )
        .map_err(|error| format!("Could not open microphone: {error}"))
}

fn capture(stopped: mpsc::Receiver<()>) -> Result<ChatFile, String> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or("No microphone is available.")?;
    let supported = device
        .default_input_config()
        .map_err(|error| format!("Could not configure microphone: {error}"))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    if config.channels == 0 || config.sample_rate == 0 || config.sample_rate > 384_000 {
        return Err("Unsupported microphone configuration.".into());
    }
    let state = Arc::new(Mutex::new(Capture::default()));
    let stream = match format {
        cpal::SampleFormat::I8 => input_stream::<i8>(&device, &config, state.clone()),
        cpal::SampleFormat::I16 => input_stream::<i16>(&device, &config, state.clone()),
        cpal::SampleFormat::I32 => input_stream::<i32>(&device, &config, state.clone()),
        cpal::SampleFormat::I64 => input_stream::<i64>(&device, &config, state.clone()),
        cpal::SampleFormat::U8 => input_stream::<u8>(&device, &config, state.clone()),
        cpal::SampleFormat::U16 => input_stream::<u16>(&device, &config, state.clone()),
        cpal::SampleFormat::U32 => input_stream::<u32>(&device, &config, state.clone()),
        cpal::SampleFormat::U64 => input_stream::<u64>(&device, &config, state.clone()),
        cpal::SampleFormat::F32 => input_stream::<f32>(&device, &config, state.clone()),
        cpal::SampleFormat::F64 => input_stream::<f64>(&device, &config, state.clone()),
        _ => return Err(format!("Unsupported microphone sample format: {format}")),
    }?;
    stream
        .play()
        .map_err(|error| format!("Could not start microphone: {error}"))?;
    let started = Instant::now();
    loop {
        if stopped.recv_timeout(Duration::from_millis(100)) != Err(mpsc::RecvTimeoutError::Timeout)
            || started.elapsed() >= Duration::from_secs(u64::from(MAX_SECONDS))
            || state.lock().unwrap().error.is_some()
        {
            break;
        }
    }
    drop(stream);
    let mut state = state.lock().unwrap();
    if let Some(error) = state.error.take() {
        return Err(error);
    }
    if state.samples.is_empty() {
        return Err("The microphone did not capture any audio.".into());
    }
    // Resample to 16 kHz mono to keep PCM voice prompts small at any device rate.
    let samples = resample(&state.samples, config.sample_rate);
    if samples.is_empty() {
        return Err("The voice recording was too short. Try recording again.".into());
    }
    Ok(attachment(
        "Voice recording.wav".into(),
        "audio/wav",
        &wav(&samples, 16_000),
    ))
}

fn resample(samples: &[i16], sample_rate: u32) -> Vec<i16> {
    let count = samples.len() as u64 * 16_000 / u64::from(sample_rate);
    (0..count)
        .map(|index| {
            let position = index as f64 * f64::from(sample_rate) / 16_000.0;
            let left = position as usize;
            let right = (left + 1).min(samples.len() - 1);
            let fraction = position.fract();
            (f64::from(samples[left]) * (1.0 - fraction) + f64::from(samples[right]) * fraction)
                .round() as i16
        })
        .collect()
}

fn wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let size = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(size as usize + 44);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(size + 36).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&size.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn attachment(name: String, mime_type: &str, bytes: &[u8]) -> ChatFile {
    ChatFile {
        name,
        uri: "".into(),
        text: None,
        audio: Some(ChatAudio {
            mime_type: mime_type.into(),
            data: STANDARD.encode(bytes),
        }),
    }
}

pub fn read_audio_file(path: &Path) -> Option<Result<ChatFile, String>> {
    let mime_type = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "webm" => "audio/webm",
        _ => return None,
    };
    Some((|| {
        let metadata =
            std::fs::metadata(path).map_err(|error| format!("Could not attach audio: {error}"))?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_AUDIO_BYTES {
            return Err("Audio attachments must be non-empty files up to 32 MiB.".into());
        }
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .and_then(|file| file.take(MAX_AUDIO_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|error| format!("Could not attach audio: {error}"))?;
        if bytes.is_empty() || bytes.len() as u64 > MAX_AUDIO_BYTES {
            return Err("Audio attachments must be non-empty files up to 32 MiB.".into());
        }
        Ok(attachment(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            mime_type,
            &bytes,
        ))
    })())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_has_a_valid_pcm_wav_header_and_samples() {
        let bytes = wav(&[-32768, 0, 32767], 16_000);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 42);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            16_000
        );
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(&bytes[44..], &[0, 128, 0, 0, 255, 127]);
        assert_eq!(resample(&[0, 3, 6, 9, 12, 15], 48_000), [0, 9]);
    }

    #[test]
    fn audio_files_embed_bytes_and_reject_empty_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("voice.WAV");
        std::fs::write(&path, wav(&[1, 2], 16_000)).unwrap();
        let file = read_audio_file(&path).unwrap().unwrap();
        let audio = file.audio.unwrap();
        assert_eq!(audio.mime_type, "audio/wav");
        assert_eq!(
            STANDARD.decode(audio.data).unwrap(),
            std::fs::read(&path).unwrap()
        );
        std::fs::write(&path, []).unwrap();
        assert!(read_audio_file(&path).unwrap().is_err());
        assert!(read_audio_file(Path::new("notes.txt")).is_none());
    }
}
