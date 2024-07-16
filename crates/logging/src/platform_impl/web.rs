// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

pub(crate) fn init_with_level(crates: &[&'static str], log_level: log::LevelFilter) {
    let _ = crates;
    let log_level = match log_level {
        log::LevelFilter::Off => {
            // console_log does not have an "off" level, so just don't configure it.
            return;
        }
        log::LevelFilter::Error => log::Level::Error,
        log::LevelFilter::Warn => log::Level::Warn,
        log::LevelFilter::Info => log::Level::Info,
        log::LevelFilter::Debug => log::Level::Debug,
        log::LevelFilter::Trace => log::Level::Trace,
    };
    console_log::init_with_level(log_level)
        .expect("console_log::init_with_level failed to initialize");
}

// End of File
