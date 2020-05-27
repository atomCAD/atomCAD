// Copyright (c) 2020 by Lachlan Sneff <lachlan@charted.space>
// Copyright (c) 2020 by Mark Friedenbach <mark@friedenbach.org>
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use parking_lot::{Condvar, Mutex};
use std::sync::Arc;
use thiserror::Error;

pub struct Sender<T> {
    most_recent: Arc<MostRecent<T>>,
}

pub struct Receiver<T> {
    most_recent: Arc<MostRecent<T>>,
}

#[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
#[error("the channel is disconnected, unable to send")]
pub struct SendError;

#[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
#[error("the channel is disconnected, unable to receive")]
pub struct RecvError;

// #[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
// pub enum TryRecvError {
//     #[error("the channel is disconnected, unable to try receiving")]
//     Disconnected,
//     #[error("the channel is empty, unable to try receiving")]
//     Empty,
// }

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

    let sender = Sender {
        most_recent: Arc::clone(&most_recent),
    };
    let receiver = Receiver { most_recent };

    (sender, receiver)
}

impl<T> Sender<T> {
    /// Replace the current item in the channel.
    pub fn send(&self, value: T) -> Result<(), SendError> {
        if Arc::strong_count(&self.most_recent) == 2 {
            *self.most_recent.mutex.lock() = Some(value);
            self.most_recent.condvar.notify_one();

            Ok(())
        } else {
            Err(SendError)
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

    // It's possible we'll use this function in the future, when
    // the main and scene thread are more independent and not in
    // lockstep, so I'm saving it.
    // pub fn try_recv(&self) -> Result<T, TryRecvError> {
    //     if Arc::strong_count(&self.most_recent) == 2 {
    //         let mut guard = self.most_recent.mutex.lock();
    //         if let Some(value) = guard.take() {
    //             Ok(value)
    //         } else {
    //             Err(TryRecvError::Empty)
    //         }
    //     } else {
    //         Err(TryRecvError::Disconnected)
    //     }
    // }
}

// End of File
