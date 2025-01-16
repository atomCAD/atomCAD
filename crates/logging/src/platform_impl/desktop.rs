// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

pub(crate) fn init_with_level(crates: &[&'static str], log_level: log::LevelFilter) {
    // Build the filter string that would have been set in RUST_LOG
    let filter_string = crates
        .iter()
        .map(|&pkg_name| {
            let level_str = match log_level {
                log::LevelFilter::Off => "off",
                log::LevelFilter::Error => "error",
                log::LevelFilter::Warn => "warn",
                log::LevelFilter::Info => "info",
                log::LevelFilter::Debug => "debug",
                log::LevelFilter::Trace => "trace",
            };
            format!("{}={}", pkg_name, level_str)
        })
        .fold(String::new(), |acc, x| {
            if acc.is_empty() {
                x
            } else {
                format!("{},{}", acc, x)
            }
        });

    // Use env_logger's builder API to avoid unsafe set_var call
    if std::env::var("RUST_LOG").is_err() {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&filter_string))
            .init();
    } else {
        env_logger::init();
    }
}

// End of File
