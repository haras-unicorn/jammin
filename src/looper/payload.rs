use std::io::BufReader;

use hound::WavReader;
use iter_read::IterRead;
use web_audio_api::{
  media_recorder::BlobEvent, AudioBuffer, AudioBufferOptions,
};

pub(super) struct Payload {
  pub(super) buffer: AudioBuffer,
  pub(super) start: chrono::DateTime<chrono::Utc>,
  pub(super) stop: chrono::DateTime<chrono::Utc>,
}

pub(super) struct PayloadFactory {
  header: Option<Vec<u8>>,
}

impl PayloadFactory {
  pub(super) fn new() -> Self {
    Self { header: None }
  }

  pub(super) fn load(
    &mut self,
    sample_rate: f32,
    started: chrono::DateTime<chrono::Utc>,
    event: BlobEvent,
  ) -> anyhow::Result<Payload> {
    let buffer = {
      if &event.blob[0..4] == b"RIFF" {
        self.header =
          Some(event.blob.iter().cloned().take(44).collect::<Vec<_>>());
        let reader = WavReader::new(BufReader::new(IterRead::new(
          event.blob.iter().cloned(),
        )))?;
        let channels = reader.spec().channels as usize;
        let mut buffer = AudioBuffer::new(AudioBufferOptions {
          number_of_channels: channels,
          length: reader.duration() as usize,
          sample_rate: reader.spec().sample_rate as f32,
        });
        for channel in 0..channels {
          let reader = WavReader::new(BufReader::new(IterRead::new(
            event.blob.iter().cloned(),
          )))?;
          buffer.copy_to_channel(
            reader
              .into_samples()
              .skip(channel as usize)
              .step_by(channels)
              .flatten()
              .collect::<Vec<_>>()
              .as_slice(),
            channel as usize,
          );
        }
        Ok(buffer)
      } else if let Some(header) = &self.header {
        let reader = WavReader::new(BufReader::new(IterRead::new(
          header.iter().cloned().chain(event.blob.iter().cloned()),
        )))?;
        let channels = reader.spec().channels as usize;
        let mut buffer = AudioBuffer::new(AudioBufferOptions {
          number_of_channels: reader.spec().channels as usize,
          length: reader.duration() as usize,
          sample_rate: reader.spec().sample_rate as f32,
        });
        for channel in 0..channels {
          let reader = WavReader::new(BufReader::new(IterRead::new(
            event.blob.iter().cloned(),
          )))?;
          buffer.copy_to_channel(
            reader
              .into_samples()
              .skip(channel as usize)
              .step_by(channels)
              .flatten()
              .collect::<Vec<_>>()
              .as_slice(),
            channel as usize,
          );
        }
        Ok(buffer)
      } else {
        Err(anyhow::anyhow!("No header"))
      }
    }?;

    let start = started
      .checked_add_signed(chrono::TimeDelta::nanoseconds(
        (event.timecode * 1_000_000_000f64).round() as i64,
      ))
      .ok_or_else(|| anyhow::anyhow!("Failed getting start of payload"))?;
    let buffer_length = buffer.length();

    let stop = start
      .checked_add_signed(chrono::TimeDelta::nanoseconds(
        (((buffer_length as f32) / sample_rate) * 1_000_000_000f32).round()
          as i64,
      ))
      .ok_or_else(|| anyhow::anyhow!("Failed getting stop of payload"))?;

    tracing::trace!(
      "Created payload with length {}, start {}, stop {}",
      buffer.length(),
      start,
      stop
    );

    Ok(Payload {
      buffer,
      start,
      stop,
    })
  }
}
