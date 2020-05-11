use std::sync::Arc;
use parking_lot::{Mutex, Condvar};

pub struct Sender<T> {
    most_recent: Arc<MostRecent<T>>,
}

pub struct Receiver<T> {
    most_recent: Arc<MostRecent<T>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SendError<T> {
    pub value: T,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RecvError;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TryRecvError {
    Disconnected,
    Empty,
}

struct MostRecent<T> {
    condvar: Condvar,
    mutex: Mutex<Option<T>>,
}

/// A single-item "channel" that only stores
/// the most recent value.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let most_recent = Arc::new(MostRecent {
        mutex: Mutex::new(None),
        condvar: Condvar::new(),
    });

    let sender = Sender { most_recent: Arc::clone(&most_recent) };
    let receiver = Receiver { most_recent };

    (sender, receiver)
}

impl<T> Sender<T> {
    /// Replace the current item in the channel.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        if Arc::strong_count(&self.most_recent) == 2 {
            *self.most_recent.mutex.lock() = Some(value);
            self.most_recent.condvar.notify_one();

            Ok(())
        } else {
            Err(SendError { value })
        }
    }
}

impl<T> Receiver<T> {
    pub fn recv(&self) -> Result<T, RecvError> {
        if Arc::strong_count(&self.most_recent) == 2 {
            let mut guard = self.most_recent.mutex.lock();
            if let Some(value) = guard.take() {
                Ok(value)
            } else {
                self.most_recent.condvar.wait(&mut guard);
                Ok(guard.take().expect("condvar spuriously woke up?"))
            }
        } else {
            Err(RecvError)
        }
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        if Arc::strong_count(&self.most_recent) == 2 {
            let mut guard = self.most_recent.mutex.lock();
            if let Some(value) = guard.take() {
                Ok(value)
            } else {
                Err(TryRecvError::Empty)
            }
        } else {
            Err(TryRecvError::Disconnected)
        }
    }
}