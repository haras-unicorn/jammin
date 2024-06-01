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

use app::{Jammin, JamminFlags};
use iced::{Application, Settings};
use web_audio_api::context::{
  AudioContext, AudioContextLatencyCategory, AudioContextOptions,
};

mod app;
mod args;
mod looper;

#[tokio::main]
#[tracing::instrument]
async fn main() -> anyhow::Result<()> {
  let args = args::parse();

  let context = AudioContext::new(AudioContextOptions {
    latency_hint: AudioContextLatencyCategory::Interactive,
    ..AudioContextOptions::default()
  });

  tracing::subscriber::set_global_default({
    let log_level = if args.trace {
      tracing::level_filters::LevelFilter::TRACE
    } else {
      #[cfg(debug_assertions)]
      {
        tracing::level_filters::LevelFilter::DEBUG
      }
      #[cfg(not(debug_assertions))]
      {
        tracing::level_filters::LevelFilter::WARN
      }
    };

    tracing_subscriber::FmtSubscriber::builder()
      .with_env_filter(
        tracing_subscriber::EnvFilter::builder()
          .with_default_directive(
            tracing::level_filters::LevelFilter::WARN.into(),
          )
          .from_env()?
          .add_directive(format!("jammin={log_level}").parse()?),
      )
      .finish()
  })?;

  Jammin::run(Settings::with_flags(JamminFlags { context }))?;

  Ok(())
}
