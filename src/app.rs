use iced::{
  executor,
  widget::{button, column, container, slider, text},
  Application, Command, Element, Theme,
};
use web_audio_api::{
  context::{AudioContext, BaseAudioContext},
  media_devices::{self, MediaStreamConstraints},
  media_streams::MediaStream,
  node::{AudioNode, GainNode, MediaStreamAudioSourceNode, StereoPannerNode},
};

use crate::looper::Looper;

pub(super) struct Jammin {
  #[allow(unused)] // NOTE: we need to hold it somewhere
  context: AudioContext,
  #[allow(unused)] // NOTE: we need to hold it somewhere
  mic_stream: MediaStream,
  #[allow(unused)] // NOTE: we need to hold it somewhere
  mic: MediaStreamAudioSourceNode,
  panner: StereoPannerNode,
  input: GainNode,
  looper: Looper,
  output: GainNode,
  status: String,
}

pub(super) struct JamminFlags {
  pub(super) context: AudioContext,
}

#[derive(Debug, Clone)]
pub(super) enum JamminMessage {
  ToggleRecording,
  RecordingToggled,
  Oneshot,
  PlayingToggled,
  InputChanged(u32),
  OutputChanged(u32),
  PanningChanged(u32),
}

impl Application for Jammin {
  type Executor = executor::Default;
  type Flags = JamminFlags;
  type Message = JamminMessage;
  type Theme = Theme;

  fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
    let context = flags.context;
    let mic_stream =
      media_devices::get_user_media_sync(MediaStreamConstraints::Audio);
    let mic = context.create_media_stream_source(&mic_stream);
    let panner = context.create_stereo_panner();
    let looper_with_gain = super::looper::Looper::with_gain(&context);

    mic.connect(&panner);
    panner.connect(&looper_with_gain.input);
    panner.connect(&looper_with_gain.output);
    looper_with_gain.output.connect(&context.destination());

    (
      Self {
        context,
        mic_stream,
        mic,
        panner,
        input: looper_with_gain.input,
        output: looper_with_gain.output,
        looper: looper_with_gain.looper,
        status: "".into(),
      },
      Command::none(),
    )
  }

  fn title(&self) -> String {
    String::from("Jammin - Audio thing")
  }

  fn update(&mut self, message: Self::Message) -> iced::Command<JamminMessage> {
    match message {
      JamminMessage::PanningChanged(panning) => {
        self.panner.pan().set_value(panning as f32 / 100f32);
        Command::none()
      }
      JamminMessage::InputChanged(gain) => {
        self.input.gain().set_value(gain as f32 / 100f32);
        Command::none()
      }
      JamminMessage::OutputChanged(gain) => {
        self.output.gain().set_value(gain as f32 / 100f32);
        Command::none()
      }
      JamminMessage::ToggleRecording => {
        let looper = self.looper.clone();
        Command::perform(
          async move { looper.toggle_recording().await },
          |result| match result {
            Ok(()) => Self::Message::RecordingToggled,
            Err(err) => {
              tracing::warn!("Error toggling recording: {}", err);
              Self::Message::RecordingToggled
            }
          },
        )
      }
      JamminMessage::RecordingToggled => {
        self.status = "Recording toggled".into();
        Command::none()
      }
      JamminMessage::Oneshot => {
        let looper = self.looper.clone();
        Command::perform(async move { looper.oneshot().await }, |result| {
          match result {
            Ok(()) => Self::Message::PlayingToggled,
            Err(err) => {
              tracing::warn!("Error toggling playing: {}", err);
              Self::Message::PlayingToggled
            }
          }
        })
      }
      JamminMessage::PlayingToggled => {
        self.status = "Playing toggled".into();
        Command::none()
      }
    }
  }

  fn view(&self) -> Element<Self::Message> {
    let panning = container(
      slider(
        0..=100,
        (self.panner.pan().value() * 100f32).round() as u32,
        Self::Message::PanningChanged,
      )
      .step(1u32),
    )
    .width(250);

    let input = container(
      slider(
        0..=100,
        (self.input.gain().value() * 100f32).round() as u32,
        Self::Message::InputChanged,
      )
      .step(1u32),
    )
    .width(250);
    let output = container(
      slider(
        0..=100,
        (self.output.gain().value() * 100f32).round() as u32,
        Self::Message::OutputChanged,
      )
      .step(1u32),
    )
    .width(250);

    let toggle_recording =
      button(text("Recording")).on_press(Self::Message::ToggleRecording);

    let oneshot = button(text("Oneshot")).on_press(Self::Message::Oneshot);

    let status = text(self.status.clone());

    column![panning, input, output, toggle_recording, oneshot, status].into()
  }
}
