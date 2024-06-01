use std::cmp::Ordering;

use web_audio_api::{AudioBuffer, AudioBufferOptions};

use super::payload::Payload;

// TODO: tracing::debug, tracing::trace

pub(super) struct ToggleRecording;

pub(super) enum LoopRecorderStateMessage {
  Inactive(AudioBuffer),
  Recording,
}

enum LoopRecorderState {
  Inactive,
  Recording,
  MarkedInactive,
}

pub(super) struct LoopRecorder {
  inner_rx: flume::Receiver<Payload>,
  sample_rate: f32,
  toggle_rx: flume::Receiver<ToggleRecording>,
  state_tx: flume::Sender<LoopRecorderStateMessage>,
  buffer: AudioBuffer,
  buffer_position: usize,
  state: LoopRecorderState,
  started: chrono::DateTime<chrono::Utc>,
  stopped: chrono::DateTime<chrono::Utc>,
}

impl LoopRecorder {
  pub(super) fn new(
    inner_rx: flume::Receiver<Payload>,
    sample_rate: f32,
    toggle_rx: flume::Receiver<ToggleRecording>,
    state_tx: flume::Sender<LoopRecorderStateMessage>,
  ) -> Self {
    let recording_buffer = AudioBuffer::new(AudioBufferOptions {
      number_of_channels: 1,
      length: (sample_rate * 60f32).round() as usize,
      sample_rate,
    });

    Self {
      inner_rx,
      sample_rate,
      toggle_rx,
      state_tx,
      buffer: recording_buffer,
      buffer_position: 0,
      state: LoopRecorderState::Inactive,
      started: chrono::Utc::now(),
      stopped: chrono::Utc::now(),
    }
  }

  pub(super) async fn run(&mut self) {
    loop {
      tokio::select! {
        toggle_recv = self.toggle_rx.recv_async() => {
          tracing::info!("Toggle received");
          match toggle_recv {
            Ok(ToggleRecording) => match self.state {
              LoopRecorderState::Inactive => {
                tracing::debug!("Toggling from inactive to recording");
                self.started = chrono::Utc::now();
                self.state = LoopRecorderState::Recording;
                if self.state_tx.send(LoopRecorderStateMessage::Recording).is_err()
                {
                  tracing::error!("Failed sending recording state");
                  return;
                }
              }
              LoopRecorderState::Recording => {
                tracing::debug!("Toggling from recording to marked inactive");
                self.stopped = chrono::Utc::now();
                self.state = LoopRecorderState::MarkedInactive;
              }
              LoopRecorderState::MarkedInactive => {}
            },
            Err(flume::RecvError::Disconnected) => {
              tracing::error!("Toggle receiver disonnected");
              return;
            }
          }
        },
        inner_recv = self.inner_rx.recv_async() => {
          match inner_recv {
            Ok(payload) => match self.state {
              LoopRecorderState::Recording => {
                tracing::trace!("Recording payload of {} samples", payload.buffer.length());
                if payload.stop < self.started {
                  tracing::warn!("Payload not in recording range");
                  continue;
                }

                self.copy_to_buffer_from(payload, self.started)
              }
              LoopRecorderState::MarkedInactive => {
                tracing::trace!(
                  "Recording payload of {} samples while marked inactive",
                  payload.buffer.length()
                );
                if payload.start > self.stopped {
                  let samples = self.buffer_position;
                  let buffer = self.flush();
                  tracing::debug!(
                    "Flushing {samples} samples with peak {} and switching state to inactive",
                    buffer
                      .get_channel_data(0)
                      .iter()
                      .cloned()
                      .max_by(|x, y| x.abs().partial_cmp(&y.abs()).unwrap_or(Ordering::Equal))
                      .unwrap_or(0f32)
                  );
                  self.state = LoopRecorderState::Inactive;
                  if self.state_tx.send(LoopRecorderStateMessage::Inactive(buffer)).is_err()
                  {
                    tracing::error!("State receiver disonnected");
                    return;
                  }
                  continue;
                }

                self.copy_to_buffer_up_to(payload, self.stopped);
              }
              _ => {}
            },
            Err(flume::RecvError::Disconnected) => {
              tracing::error!("Recorder receiver disonnected");
              return;
            }
          }
        }
      }
    }
  }

  fn copy_to_buffer_up_to(
    &mut self,
    payload: Payload,
    time: chrono::DateTime<chrono::Utc>,
  ) {
    if let Some((buffer, _)) =
      Self::split_buffer(self.sample_rate, &payload, time)
    {
      self.copy_to_buffer(buffer);
    }
  }

  fn copy_to_buffer_from(
    &mut self,
    payload: Payload,
    time: chrono::DateTime<chrono::Utc>,
  ) {
    if let Some((_, buffer)) =
      Self::split_buffer(self.sample_rate, &payload, time)
    {
      self.copy_to_buffer(buffer);
    }
  }

  fn split_buffer(
    sample_rate: f32,
    payload: &Payload,
    time: chrono::DateTime<chrono::Utc>,
  ) -> Option<(&[f32], &[f32])> {
    let nanoseconds =
      match (time.signed_duration_since(payload.start)).num_nanoseconds() {
        Some(nanoseconds) => nanoseconds,
        None => {
          return None;
        }
      };
    let split = std::cmp::min(
      (sample_rate * nanoseconds as f32 / 1_000_000_000f32) as usize,
      payload.buffer.length().saturating_sub(1),
    );
    tracing::trace!("Splitting buffer at {}", split);
    Some(payload.buffer.get_channel_data(0).split_at(split))
  }

  fn copy_to_buffer(&mut self, buffer: &[f32]) {
    let initial_final_buffer_position =
      self.buffer_position.saturating_add(buffer.len());
    if initial_final_buffer_position < self.buffer.length() {
      tracing::trace!(
        "Copying to buffer from {} to {}",
        self.buffer_position,
        initial_final_buffer_position
      );
      self
        .buffer
        .copy_to_channel_with_offset(buffer, 0, self.buffer_position);
      self.buffer_position = initial_final_buffer_position;
    } else {
      let overflow =
        initial_final_buffer_position.saturating_sub(self.buffer.length());
      tracing::trace!(
        "Copying to buffer {} with overflow {}",
        buffer.len(),
        overflow
      );
      let trimmed = self
        .buffer
        .get_channel_data(0)
        .split_at(overflow)
        .1
        .to_vec();
      self.buffer.copy_to_channel(trimmed.as_slice(), 0);
      self
        .buffer
        .copy_to_channel_with_offset(buffer, 0, trimmed.len());
      self.buffer_position = self.buffer.length();
    }
  }

  fn flush(&mut self) -> AudioBuffer {
    let buffer = {
      let mut buffer = AudioBuffer::new(AudioBufferOptions {
        number_of_channels: 1,
        length: self.buffer_position,
        sample_rate: self.sample_rate,
      });
      buffer.copy_to_channel(
        self
          .buffer
          .get_channel_data(0)
          .split_at(self.buffer_position)
          .0,
        0,
      );
      buffer
    };
    self
      .buffer
      .get_channel_data_mut(0)
      .iter_mut()
      .map(|x| *x = 0f32)
      .for_each(drop);
    self.buffer_position = 0;

    buffer
  }
}
