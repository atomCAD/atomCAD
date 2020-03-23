use std::collections::VecDeque;
use std::time::{Duration, Instant};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub struct Fps {
    last_second_frames: VecDeque<Instant>,
    known_fps: Arc<AtomicUsize>,
}

#[derive(Clone)]
pub struct FpsGet {
    known_fps: Arc<AtomicUsize>,
}

impl Fps {
    pub fn create() -> (Fps, FpsGet) {
        let known_fps = Arc::new(AtomicUsize::new(0));
        (
            Self {
                last_second_frames: VecDeque::with_capacity(128),
                known_fps: known_fps.clone(),
            },
            FpsGet {
                known_fps,
            }
        )
    }

    pub fn tick(&mut self) -> usize {
        let now = Instant::now();
        let a_second_ago = now - Duration::from_secs(1);

        while self
            .last_second_frames
            .front()
            .map_or(false, |t| *t < a_second_ago)
        {
            self.last_second_frames.pop_front();
        }

        self.last_second_frames.push_back(now);
        let fps = self.last_second_frames.len();

        self.known_fps.store(fps, Ordering::SeqCst);

        fps
    }
}

impl FpsGet {
    pub fn get(&self) -> usize {
        self.known_fps.load(Ordering::SeqCst)
    }
}