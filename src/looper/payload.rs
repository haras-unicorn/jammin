use std::io::{Cursor, Read};

use web_audio_api::{
  media_recorder::BlobEvent, AudioBuffer, AudioBufferOptions,
};

// FIXME: stateful because the header contains information about channels

pub(super) struct Payload {
  pub(super) buffer: AudioBuffer,
  pub(super) start: chrono::DateTime<chrono::Utc>,
  pub(super) stop: chrono::DateTime<chrono::Utc>,
}

impl Payload {
  pub(super) fn new(
    sample_rate: f32,
    started: chrono::DateTime<chrono::Utc>,
    event: BlobEvent,
  ) -> anyhow::Result<Self> {
    let mut buffer = AudioBuffer::new(AudioBufferOptions {
      number_of_channels: 1,
      length: event.blob.len() / 4,
      sample_rate,
    });

    let mut cursor = Cursor::new(&event.blob);
    let mut pcm_data = Vec::new();

    let is_first_chunk = {
      let mut header = [0u8; 4];
      cursor.read_exact(&mut header)?;
      &header == b"RIFF"
    };

    if is_first_chunk {
      cursor.set_position(44 as u64);
    }
    cursor.read_to_end(&mut pcm_data)?;

    let encoded = pcm_data
      .chunks_exact(4)
      .map(|chunk| {
        #[allow(clippy::unwrap_used)] // NOTE: we set the chunk size statically
        f32::from_le_bytes(chunk[0..4].try_into().unwrap())
      })
      .collect::<Vec<f32>>();

    buffer.copy_to_channel(encoded.as_slice(), 0);

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

    Ok(Self {
      buffer,
      start,
      stop,
    })
  }
}
