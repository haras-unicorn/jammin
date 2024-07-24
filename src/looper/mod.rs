mod payload;
mod recorder;

use std::{cmp::Ordering, sync::Arc};

use tokio::{sync::Mutex, task::JoinHandle};
use web_audio_api::{
  context::{AudioContext, BaseAudioContext},
  media_recorder::{BlobEvent, MediaRecorder},
  node::{
    AudioBufferSourceNode, AudioNode, AudioScheduledSourceNode, GainNode,
    MediaStreamAudioDestinationNode,
  },
  AudioBuffer, AudioBufferOptions,
};

use self::{
  payload::PayloadFactory,
  recorder::{LoopRecorder, LoopRecorderStateMessage, ToggleRecording},
};

// TODO: tracing::debug, tracing::trace

struct LooperState {
  recorded: AudioBuffer,
  recorded_source: Option<AudioBufferSourceNode>,
  #[allow(unused)] // NOTE: have to store it somewhere
  recorder_handle: JoinHandle<()>,
  #[allow(unused)] // NOTE: have to store it somewhere
  destination: MediaStreamAudioDestinationNode,
  #[allow(unused)] // NOTE: have to store it somewhere
  recorder: MediaRecorder,
  output: GainNode,
}

#[derive(Clone)]
pub(crate) struct Looper {
  recorder_state_rx: flume::Receiver<LoopRecorderStateMessage>,
  toggle_recording_tx: flume::Sender<ToggleRecording>,
  state: Arc<Mutex<LooperState>>,
}

pub(crate) struct LooperWithGain {
  pub(crate) looper: Looper,
  pub(crate) input: GainNode,
  pub(crate) output: GainNode,
}

impl Looper {
  pub(crate) fn with_gain(context: &AudioContext) -> LooperWithGain {
    let input = context.create_gain();
    let output = context.create_gain();

    LooperWithGain {
      looper: Self::new(context, &input, &output),
      input,
      output,
    }
  }

  pub(crate) fn new(
    context: &AudioContext,
    input: &impl AudioNode,
    output: &impl AudioNode,
  ) -> Self {
    let sample_rate = context.sample_rate();
    let destination = context.create_media_stream_destination();
    input.connect(&destination);

    let recorder = MediaRecorder::new(destination.stream());
    let (recorder_tx, recorder_rx) = flume::unbounded();
    let started = chrono::Utc::now(); // NOTE: can't get the actual starting time from API...
    recorder.set_onerror(move |event| {
      tracing::error!("Recorder error {:?}", event);
    });
    let mut payload_factory = PayloadFactory::new();
    recorder.set_ondataavailable(move |event: BlobEvent| {
      tracing::trace!("Received buffer len {}", event.blob.len());
      let payload = payload_factory.load(sample_rate, started, event);
      match payload {
        Ok(payload) => {
          if let Err(error) = recorder_tx.try_send(payload) {
            tracing::error! {
              "Error sending recorder data through channel: {:?}",
              error
            };
          }
        }
        Err(err) => {
          tracing::error!("Failed creating payload {err}");
        }
      }
    });
    recorder.start();

    let (toggle_recording_tx, toggle_recording_rx) = flume::bounded(1);
    let (state_tx, state_rx) = flume::bounded(1);

    let handle = tokio::spawn(async move {
      let mut loop_recorder = LoopRecorder::new(
        recorder_rx,
        sample_rate,
        toggle_recording_rx,
        state_tx,
      );
      loop_recorder.run().await;
    });

    let recorded = AudioBuffer::new(AudioBufferOptions {
      number_of_channels: 1,
      length: (sample_rate * 30f32) as usize,
      sample_rate,
    });

    let gain = context.create_gain();
    gain.connect(output);

    Self {
      recorder_state_rx: state_rx,
      toggle_recording_tx,
      state: Arc::new(Mutex::new(LooperState {
        destination,
        recorded,
        recorded_source: None,
        recorder_handle: handle,
        recorder,
        output: gain,
      })),
    }
  }

  pub(crate) async fn toggle_recording(&self) -> anyhow::Result<()> {
    tracing::debug!("Toggled recording");
    self.toggle_recording_tx.send_async(ToggleRecording).await?;
    let recorder_state = self.recorder_state_rx.recv_async().await?;
    if let LoopRecorderStateMessage::Inactive(buffer) = recorder_state {
      tracing::debug!(
        "Received buffer with peak at {:?} lasting {} s",
        buffer
          .get_channel_data(0)
          .iter()
          .cloned()
          .max_by(|x, y| x
            .abs()
            .partial_cmp(&y.abs())
            .unwrap_or(Ordering::Equal))
          .unwrap_or(0f32),
        buffer.duration()
      );
      self.state.clone().lock_owned().await.recorded = buffer;
    };
    Ok(())
  }

  pub(crate) async fn oneshot(&self) -> anyhow::Result<()> {
    {
      let mut state = self.state.clone().lock_owned().await;
      let context = state.output.context();
      if let Some(recorded_source) = &state.recorded_source {
        tracing::debug!("Disconnected recording");
        recorded_source.disconnect();
      }
      let recorded = state.recorded.clone();
      let duration = state.recorded.duration();
      tracing::debug!(
        "Oneshot with peak at {:?} lasting {} s",
        recorded
          .get_channel_data(0)
          .iter()
          .cloned()
          .max_by(|x, y| x
            .abs()
            .partial_cmp(&y.abs())
            .unwrap_or(Ordering::Equal))
          .unwrap_or(0f32),
        duration
      );
      let mut buffer_source = context.create_buffer_source();
      buffer_source.set_buffer(recorded);
      buffer_source.start();
      buffer_source.stop_at(state.output.context().current_time() + duration);
      buffer_source.connect(&state.output);
      state.recorded_source = Some(buffer_source);
    }

    Ok(())
  }
}
