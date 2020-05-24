// Copyright (c) 2020 by Lachlan Sneff <lachlan@charted.space>
// Copyright (c) 2020 by Mark Friedenbach <mark@friedenbach.org>

use log::{Log, Metadata, Record, LevelFilter};
use parking_lot::Mutex;
use anyhow::Result;
use std::{
    io::{self, Write},
    fs::{OpenOptions, File},
};

pub struct Logger {
    file: Mutex<File>,
    stderr: io::Stderr,
}

impl Logger {
    pub fn init() -> Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open("atomcad.log")?;
        
        log::set_boxed_logger(Box::new(Logger {
            file: Mutex::new(file),
            stderr: io::stderr(),
        }))?;

        log::set_max_level(LevelFilter::Info);

        Ok(())
    }
}

impl Log for Logger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let mut logfile = self.file.lock();
        let mut stderr = self.stderr.lock();

        match (record.file(), record.line()) {
            (Some(file), Some(line)) => {
                writeln!(logfile,
                    "{}|{}({}:{}): {}",
                    record.level(),
                    record.target(),
                    file,
                    line,
                    record.args()
                ).unwrap();

                if record.target().starts_with("atomcad") {
                    writeln!(stderr,
                        "{}|{}({}:{}): {}",
                        record.level(),
                        record.target(),
                        file,
                        line,
                        record.args()
                    ).unwrap();
                }
            }
            (Some(file), None) => {
                writeln!(logfile,
                    "{}|{}({}): {}",
                    record.level(),
                    record.target(),
                    file,
                    record.args()
                ).unwrap();

                if record.target().starts_with("atomcad") {
                    writeln!(stderr,
                        "{}|{}({}): {}",
                        record.level(),
                        record.target(),
                        file,
                        record.args()
                    ).unwrap();
                }
            }
            _ => {
                writeln!(logfile,
                    "{}|{}: {}",
                    record.level(),
                    record.target(),
                    record.args()
                ).unwrap();

                if record.target().starts_with("atomcad") {
                    writeln!(stderr,
                        "{}|{}: {}",
                        record.level(),
                        record.target(),
                        record.args()
                    ).unwrap();
                }
            }
        }

        stderr.flush().unwrap();
        logfile.flush().unwrap();
    }

    fn flush(&self) {

    }
}

// End of File
