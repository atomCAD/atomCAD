// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

pub(crate) fn init_with_level(crates: &[&'static str], log_level: log::LevelFilter) {
    let log_level = match log_level {
        log::LevelFilter::Off => "off",
        log::LevelFilter::Error => "error",
        log::LevelFilter::Warn => "warn",
        log::LevelFilter::Info => "info",
        log::LevelFilter::Debug => "debug",
        log::LevelFilter::Trace => "trace",
    };
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var(
            "RUST_LOG",
            crates
                .iter()
                .map(|&pkg_name| format!("{}={}", pkg_name, log_level))
                .fold(String::new(), |acc, x| {
                    if acc.is_empty() {
                        x
                    } else {
                        format!("{},{}", acc, x)
                    }
                }),
        );
    };
    env_logger::init();
}

// End of File
