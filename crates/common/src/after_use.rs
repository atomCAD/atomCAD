use smallvec::SmallVec;
use std::{
    future::Future,
    task::{Context, Waker},
    sync::Arc,
};
use futures::task::AtomicWaker;

pub struct AfterUse<T> {
    wakers: Arc<AtomicWaker>,
    inner: T,
}

struct AfterUseFuture {

}

impl<T> AfterUse<T> {
    pub fn new(inner: T) -> Self {
        Self {
            wakers: SmallVec::new(),
            inner,
        }
    }

    pub fn after_use(&self) -> impl Future<Output = ()> {
        
    }
}
