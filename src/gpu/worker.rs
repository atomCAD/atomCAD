use std::thread;
use tokio::{
    runtime::Runtime,
    task::LocalSet,
    sync::mpsc::{channel, Sender, Receiver},
    stream::StreamExt as _,
    time,
    task,
};
use anyhow::Result;

#[derive(Debug)]
enum Msg {
    Shutdown,
}

pub struct Worker {
    handle: Option<thread::JoinHandle<()>>,
    tx: Sender<Msg>,
}

impl Worker {
    pub fn spawn(f: impl FnOnce(&LocalSet) + Send + 'static) -> Result<Self> {
        let mut runtime = Runtime::new()?;

        let (tx, mut rx) = channel(1);

        let handle = thread::spawn(move || {
            info!("Started worker");

            let local_set = LocalSet::new();

            f(&local_set);

            local_set.block_on(&mut runtime, async {
                while let Some(msg) = rx.next().await {
                    match msg {
                        Msg::Shutdown => break,
                    }
                }
            });

            info!("worker thread shutting down");
        });

        Ok(Self {
            handle: Some(handle),
            tx,
        })
    }

    pub fn shutdown(&mut self) -> Result<()> {
        self.tx.try_send(Msg::Shutdown)?;
        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.shutdown();
        self.handle.take().unwrap().join().unwrap();
    }
}