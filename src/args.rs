#[derive(Debug, Clone, clap::Parser)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Values {
  /// Set log level to trace
  #[arg(short, long)]
  pub(crate) trace: bool,
}

pub(crate) fn parse() -> Values {
  clap::Parser::parse()
}
