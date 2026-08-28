use std::io;

use flexi_logger::{
    Age, Cleanup, Criterion, DeferredNow, Duplicate, FileSpec, FlexiLoggerError, Logger, LoggerHandle, Naming, style,
};
use laplace_settings::laplace::LoggerSettings;

pub fn init(settings: &LoggerSettings) -> Result<LoggerHandle, FlexiLoggerError> {
    let mut logger = Logger::try_with_env_or_str(&settings.spec)?;
    if let Some(path) = &settings.path {
        logger = logger
            .log_to_file(FileSpec::try_from(path)?.suppress_timestamp())
            .rotate(
                Criterion::Age(Age::Day),
                Naming::Timestamps,
                Cleanup::KeepLogFiles(settings.keep_log_for_days),
            )
            .append()
    }
    let handle = logger
        .duplicate_to_stdout(if settings.duplicate_to_stdout {
            Duplicate::All
        } else {
            Duplicate::None
        })
        .use_utc()
        .format(custom_colored_detailed_format)
        .start()?;

    Ok(handle)
}

fn custom_colored_detailed_format(
    writer: &mut dyn io::Write,
    now: &mut DeferredNow,
    record: &log::Record,
) -> Result<(), io::Error> {
    let level = record.level();
    let start_idx = record
        .file()
        .and_then(|path| {
            path.rfind("/src")
                .and_then(|src_idx| path[..src_idx].rfind('/'))
                .map(|start_idx| start_idx + 1)
        })
        .unwrap_or(0);
    let path = record.file().map(|path| &path[start_idx..]).unwrap_or("<unnamed>");

    write!(
        writer,
        "[{}] {} [{}] {}:{}: {}",
        style(level).paint(now.format("%Y-%m-%d %H:%M:%S%.6f").to_string()),
        style(level).paint(level.as_str()),
        record.module_path().unwrap_or("<unnamed>"),
        path,
        record.line().unwrap_or(0),
        style(level).paint(record.args().to_string()),
    )
}

/// Initializes logging on mobile, where stdout is discarded by the OS.
///
/// On Android the records go to logcat under the `Laplace` tag (`adb logcat -s Laplace`), filtered
/// by the same spec as on desktop. On iOS stdout is already redirected to the system log by
/// `tauri::log_stdout`, so the desktop logger is used as is.
#[cfg(mobile)]
pub fn init_mobile(settings: &LoggerSettings) {
    #[cfg(target_os = "android")]
    {
        let filter = android_logger::FilterBuilder::new().parse(&settings.spec).build();
        android_logger::init_once(
            android_logger::Config::default()
                .with_tag("Laplace")
                // `init_once` leaves the global max level of the `log` crate at `Off` unless it is
                // given one, and the `log` macros check it before the filter ever sees a record.
                .with_max_level(filter.filter())
                .with_filter(filter),
        );
    }

    #[cfg(not(target_os = "android"))]
    if let Err(err) = init(settings) {
        eprintln!("Logger should be initialized: {err}");
    }
}
