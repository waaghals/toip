use anyhow::{Context, Result};
use log::Level;
use simplelog::{ColorChoice, Config, ConfigBuilder, LevelFilter, TermLogger, TerminalMode};

fn level_filter(level: Option<Level>) -> LevelFilter {
    match level {
        Some(Level::Error) => LevelFilter::Error,
        Some(Level::Warn) => LevelFilter::Warn,
        Some(Level::Info) => LevelFilter::Info,
        Some(Level::Debug) => LevelFilter::Debug,
        Some(Level::Trace) => LevelFilter::Trace,
        None => LevelFilter::Off,
    }
}
pub fn init(level: Option<Level>) -> Result<()> {
    let config = ConfigBuilder::new()
        .set_max_level(LevelFilter::Error)
        .set_time_level(LevelFilter::Error)
        .set_thread_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Off)
        .set_location_level(LevelFilter::Off)
        .build();

    TermLogger::init(
        level_filter(level),
        config,
        TerminalMode::Stderr,
        ColorChoice::Auto,
    )
    .with_context(|| format!("could not initialize logger"))?;

    Ok(())
}
