#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        eprintln!(
            "{} kairosd: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            format_args!($($arg)*)
        )
    };
}
