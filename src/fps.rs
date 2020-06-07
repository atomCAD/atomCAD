// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct Fps {
    last_second_frames: VecDeque<Instant>,
}

impl Fps {
    pub fn new() -> Fps {
        Self {
            last_second_frames: VecDeque::with_capacity(128),
        }
    }

    pub fn tick(&mut self) {
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
    }

    pub fn get(&self) -> usize {
        self.last_second_frames.len()
    }
}
