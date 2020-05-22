use env_logger::fmt::{Color, Style, StyledValue};
use log::{Level, LevelFilter};

pub fn setup() {
    let mut builder = env_logger::Builder::new();

    builder
        .format(|f, record| {
            use std::io::Write;

            let target = record.target();

            let mut style = f.style();
            let level = colored_level(&mut style, record.level());

            let mut style = f.style();
            let target = style.set_bold(true).value(target);

            let time = f.timestamp_millis();

            writeln!(f, " {}[{}] {} > {}", level, time, target, record.args(),)
        })
        .filter(Some("atomcad"), LevelFilter::Info);

    builder.init();
}

fn colored_level<'a>(style: &'a mut Style, level: Level) -> StyledValue<'a, &'static str> {
    match level {
        Level::Trace => style.set_color(Color::Magenta).value("TRACE"),
        Level::Debug => style.set_color(Color::Blue).value("DEBUG"),
        Level::Info => style.set_color(Color::Green).value("INFO "),
        Level::Warn => style.set_color(Color::Yellow).value("WARN "),
        Level::Error => style.set_color(Color::Red).value("ERROR"),
    }
}
