//! Jammin - Audio thing

#![deny(
  unsafe_code,
  // reason = "Let's just not do it"
)]
#![deny(
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic,
  clippy::unreachable,
  clippy::arithmetic_side_effects
  // reason = "We have to handle errors properly"
)]
#![deny(
  clippy::dbg_macro,
  // reason = "Use tracing instead"
)]
#![deny(
  missing_docs,
  // reason = "Document everything"
)]

use iced::executor;
use iced::widget::{button, column, container, slider, text};
use iced::{Application, Command, Element, Settings, Theme};
use web_audio_api::context::{
  AudioContext, AudioContextLatencyCategory, AudioContextOptions,
  BaseAudioContext,
};
use web_audio_api::media_devices::{self, MediaStreamConstraints};
use web_audio_api::node::{
  AudioNode, GainNode, MediaStreamAudioSourceNode, StereoPannerNode,
};

#[tokio::main]
#[tracing::instrument]
async fn main() -> anyhow::Result<()> {
  let context = AudioContext::new(AudioContextOptions {
    latency_hint: AudioContextLatencyCategory::Interactive,
    ..AudioContextOptions::default()
  });

  let gain = context.create_gain();

  let panner = context.create_stereo_panner();
  let pan = panner.pan();
  pan.set_value(pan.min_value() + (pan.max_value() - pan.min_value()) / 2f32);

  let mic = context.create_media_stream_source(
    &media_devices::get_user_media_sync(MediaStreamConstraints::Audio),
  );

  mic.connect(&panner);
  panner.connect(&gain);
  gain.connect(&context.destination());

  Jammin::run(Settings::with_flags(JamminFlags {
    context,
    gain,
    panner,
    mic,
  }))?;

  Ok(())
}

struct Jammin {
  flags: JamminFlags,
  playing: bool,
}

struct JamminFlags {
  context: AudioContext,
  gain: GainNode,
  panner: StereoPannerNode,
  mic: MediaStreamAudioSourceNode,
}

#[derive(Debug, Clone)]
enum JamminMessage {
  PlayPause,
  VolumeChanged(u32),
  PanningChanged(u32),
}

impl Application for Jammin {
  type Executor = executor::Default;
  type Flags = JamminFlags;
  type Message = JamminMessage;
  type Theme = Theme;

  fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
    (
      Self {
        flags,
        playing: true,
      },
      Command::none(),
    )
  }

  fn title(&self) -> String {
    String::from("Jammin - Audio thing")
  }

  fn update(&mut self, message: Self::Message) -> iced::Command<JamminMessage> {
    match message {
      JamminMessage::PlayPause => {
        if self.playing {
          self.flags.gain.disconnect();
          self.playing = false;
        } else {
          self.flags.gain.connect(&self.flags.context.destination());
          self.playing = true;
        }
      }
      JamminMessage::VolumeChanged(gain) => {
        println!("{}", gain);
        self.flags.gain.gain().set_value(gain as f32 / 100f32);
      }
      JamminMessage::PanningChanged(panning) => {
        self.flags.panner.pan().set_value(panning as f32 / 100f32);
      }
    };

    Command::none()
  }

  fn view(&self) -> Element<Self::Message> {
    let play_pause =
      button(text("Play/Pause")).on_press(Self::Message::PlayPause);

    let volume = container(
      slider(
        0..=100,
        (self.flags.gain.gain().value() * 100f32).round() as u32,
        Self::Message::VolumeChanged,
      )
      .step(1u32),
    )
    .width(250);

    let panning = container(
      slider(
        0..=100,
        (self.flags.panner.pan().value() * 100f32).round() as u32,
        Self::Message::PanningChanged,
      )
      .step(1u32),
    )
    .width(250);

    column![play_pause, volume, panning].into()
  }
}
