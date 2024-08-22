use std::io::{Cursor, Read};

use symphonia::{
  core::{
    audio::{AudioBufferRef, Signal},
    codecs::DecoderOptions,
    formats::{SeekMode, SeekTo},
    io::MediaSourceStream,
    probe::Hint,
    units::Time,
  },
  default::get_probe,
};
use web_audio_api::{
  media_recorder::BlobEvent, AudioBuffer, AudioBufferOptions,
};

// FIXME: make a source stream for symphonia and send symphonia packets as audio buffers

pub(super) struct Payload {
  pub(super) buffer: AudioBuffer,
  pub(super) start: chrono::DateTime<chrono::Utc>,
  pub(super) stop: chrono::DateTime<chrono::Utc>,
}

pub(super) struct PayloadFactory {
  started: chrono::DateTime<chrono::Utc>,
  header: Option<Vec<u8>>,
}

impl PayloadFactory {
  pub(super) fn new(started: chrono::DateTime<chrono::Utc>) -> Self {
    Self {
      started,
      header: None,
    }
  }

  pub(super) fn load(&mut self, event: BlobEvent) -> anyhow::Result<Payload> {
    let source = MediaSourceStream::new(
      Box::new(Cursor::new(event.blob.clone())),
      Default::default(),
    );
    let mut hint = Hint::new();
    hint.mime_type("audio/wav");
    let mut probed = if let Ok(probed) = get_probe().format(
      &hint,
      source,
      &Default::default(),
      &Default::default(),
    ) {
      self.header = Some(event.blob);
      probed
    } else if let Some(header) = &self.header {
      let mut new_header = Vec::new();
      header
        .chain(Cursor::new(event.blob))
        .read_to_end(&mut new_header)?;
      self.header = Some(new_header.clone());
      get_probe().format(
        &hint,
        MediaSourceStream::new(
          Box::new(Cursor::new(new_header.clone())),
          Default::default(),
        ),
        &Default::default(),
        &Default::default(),
      )?
    } else {
      return Err(anyhow::anyhow!("No header"));
    };

    let track = probed
      .format
      .default_track()
      .ok_or_else(|| anyhow::anyhow!("No default track"))?;
    let channels = track
      .codec_params
      .channels
      .ok_or_else(|| anyhow::anyhow!("No channels"))?;
    let length = track
      .codec_params
      .n_frames
      .ok_or_else(|| anyhow::anyhow!("No frame count"))?
      as usize;
    let sample_rate = track
      .codec_params
      .sample_rate
      .ok_or_else(|| anyhow::anyhow!("No sample rate"))?
      as f32;
    let mut buffer = AudioBuffer::new(AudioBufferOptions {
      number_of_channels: channels.count(),
      length,
      sample_rate,
    });

    probed.format.seek(
      SeekMode::Coarse,
      SeekTo::Time {
        time: Time::new(event.timecode.floor() as u64, event.timecode.fract()),
        track_id: None,
      },
    )?;

    loop {
      let packet = match probed.format.next_packet() {
        Ok(packet) => packet,
        Err(symphonia::core::errors::Error::ResetRequired) => {
          break;
        }
        Err(err) => {
          // A unrecoverable error occured, halt decoding.
          panic!("{}", err);
        }
      };

      let track = probed
        .format
        .tracks()
        .iter()
        .find(|track| track.id == packet.track_id())
        .ok_or_else(|| anyhow::anyhow!("Track not found"))?;

      let dec_opts: DecoderOptions = Default::default();

      let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .expect("unsupported codec");

      match decoder.decode(&packet) {
        Ok(decoded) => match decoded {
          AudioBufferRef::F32(decoded_f32) => {
            for (i, _) in decoded_f32.spec().channels.iter().enumerate() {
              buffer.copy_to_channel_with_offset(decoded_f32.chan(i), i, 0);
            }
          }
          _ => {
            unimplemented!();
          }
        },
        Err(symphonia::core::errors::Error::IoError(_)) => {
          continue;
        }
        Err(symphonia::core::errors::Error::DecodeError(_)) => {
          continue;
        }
        Err(err) => {
          panic!("{}", err);
        }
      };
    }

    let start = self
      .started
      .checked_add_signed(chrono::TimeDelta::nanoseconds(
        (event.timecode * 1_000_000_000f64).round() as i64,
      ))
      .ok_or_else(|| anyhow::anyhow!("Failed getting start of payload"))?;

    let stop = start
      .checked_add_signed(chrono::TimeDelta::nanoseconds(
        (((length as f32) / sample_rate) * 1_000_000_000f32).round() as i64,
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
