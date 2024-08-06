use std::{
  io::{BufReader, Cursor, Read, Seek},
  sync::{Arc, Mutex},
};

use symphonia::{
  core::{
    codecs::CODEC_TYPE_NULL,
    io::{MediaSource, MediaSourceStream, ReadOnlySource},
    probe::Hint,
  },
  default::get_probe,
};
use web_audio_api::{
  media_recorder::BlobEvent, AudioBuffer, AudioBufferOptions,
};

// FIXME: the pony optimization thing

pub(super) struct Payload {
  pub(super) buffer: AudioBuffer,
  pub(super) start: chrono::DateTime<chrono::Utc>,
  pub(super) stop: chrono::DateTime<chrono::Utc>,
}

trait ReadSeek: Read + Seek + Send {}

impl<T> ReadSeek for Cursor<T> where T: AsRef<[u8]> + Send {}

#[derive(Clone)]
struct Pony {
  data: Arc<Mutex<Box<dyn ReadSeek>>>,
}

impl Pony {
  fn new(data: impl ReadSeek) -> Self {
    Self {
      data: Arc::new(Mutex::new(Box::new(data))),
    }
  }
}

impl Read for Pony {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut lock = self.data.lock().map_err(|err| {
      std::io::Error::new(std::io::ErrorKind::Other, anyhow::anyhow!("Poison"))
    })?;
    (*lock).read(buf)
  }
}

impl Seek for Pony {
  fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
    let mut lock = self.data.lock().map_err(|err| {
      std::io::Error::new(std::io::ErrorKind::Other, anyhow::anyhow!("Poison"))
    })?;
    (*lock).seek(pos)
  }
}

impl MediaSource for Pony {
  fn is_seekable(&self) -> bool {
    true
  }

  fn byte_len(&self) -> Option<u64> {
    None
  }
}

pub(super) struct PayloadFactory {
  header: Option<Pony>,
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
    let source = MediaSourceStream::new(
      Box::new(Cursor::new(event.blob.clone())),
      Default::default(),
    );
    let mut hint = Hint::new();
    hint.mime_type("audio/wav");
    let probed = if let Ok(probed) = get_probe().format(
      &hint,
      source,
      &Default::default(),
      &Default::default(),
    ) {
      self.header = Some(Pony::new(Cursor::new(event.blob)));
      probed
    } else if let Some(header) = &self.header {
      let pony =
        Pony::new(BufReader::new(header.chain(Cursor::new(event.blob))));
      self.header = Some(pony.clone());
      get_probe().format(
        &hint,
        MediaSourceStream::new(
          Box::new(ReadOnlySource::new(pony.clone())),
          Default::default(),
        ),
        &Default::default(),
        &Default::default(),
      )?
    } else {
      return Err(anyhow::anyhow!("No header"));
    };

    let default_track = probed
      .format
      .tracks()
      .iter()
      .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
      .ok_or_else(|| anyhow::anyhow!("No default track"))?;
    let mut buffer = AudioBuffer::new(AudioBufferOptions {
      number_of_channels: probed.format.tracks().len(),
      length: default_track
        .codec_params
        .n_frames
        .ok_or_else(|| anyhow::anyhow!("No frame count"))?
        as usize,
      sample_rate: default_track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow::anyhow!("No sample rate"))?
        as f32,
    });

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
