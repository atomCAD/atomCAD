use std::time::Instant;

pub struct Timer {
    action: String,
    start: Instant,
}

impl Timer {
    pub fn new(action: &str) -> Self {
        Self {
            action: action.to_string(),
            start: Instant::now(),
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        println!("{} took {:.3?}", self.action, elapsed);
    }
}
















