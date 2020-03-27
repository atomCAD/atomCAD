#[cfg(build = "debug")]
pub use self::implementation::*;
#[cfg(not(build = "debug"))]
pub use self::stub::*;

#[cfg(build = "debug")]
mod implementation {
    use std::time::Duration;
    use std::fmt;

    #[derive(Default)]
    pub struct DebugMetrics {
        pub scene_draw: Option<Duration>,
        pub ui_draw: Option<Duration>,
        pub frame: Option<Duration>,
        pub queue: Option<Duration>,
    }

    impl DebugMetrics {
        pub fn output(&self) -> impl Iterator<Item = String> {
            let names: &[&'static str] = &["scene draw", "ui draw", "frame", "queue"];
            let durations = [self.scene_draw, self.ui_draw, self.frame, self.queue];
            
            (0..4).into_iter()
                .map(move |i| (names[i], durations[i].clone()))
                .filter_map(|(name, duration)| {
                    duration
                        .map(|duration| format!("    {}: {}", name, duration_to_display(duration)))
                })
        }
    }

    fn duration_to_display(duration: Duration) -> impl fmt::Display {
        struct Display {
            n: usize,
            suffix: &'static str,
        }
        impl fmt::Display for Display {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{}{}", self.n, self.suffix)
            }
        }

        match duration.as_nanos() {
            n @ 0 ..= 999 => Display { n: n as usize, suffix: "ns" },
            1000 ..= 999_999 => Display { n: duration.as_micros() as usize, suffix: "μs" },
            1_000_000 ..= 999_999_999 => Display { n: duration.as_millis() as usize, suffix: "ms" },
            _ => Display { n: duration.as_secs() as usize, suffix: "s" },
        }
    }
}

#[cfg(not(build = "debug"))]
mod stub {
    use std::time::Duration;

    #[derive(Default)]
    pub struct DebugMetrics {
        pub scene_draw: Option<Duration>,
        pub ui_draw: Option<Duration>,
        pub frame: Option<Duration>,
        pub queue: Option<Duration>,
    }

    impl DebugMetrics {
        pub fn output(&self) -> impl Iterator<Item = String> {
            std::iter::empty()
        }
    }
}