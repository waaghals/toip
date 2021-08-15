use log::Level;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};

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
pub fn init(level: Option<Level>) {
    TermLogger::new(
        level_filter(level),
        Config::default(),
        TerminalMode::Stderr,
        ColorChoice::Auto,
    );
}
