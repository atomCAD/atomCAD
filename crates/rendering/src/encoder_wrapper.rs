// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use futures::{
    channel::oneshot::{channel, Receiver, Sender},
    future::{FutureExt as _, Shared},
};
use std::{
    future::Future,
    ops::{Deref, DerefMut},
};

pub struct Tripper {
    sender: Sender<()>,
}

pub struct EncoderWrapper {
    command_encoder: wgpu::CommandEncoder,
    sender: Sender<()>,
    shared: Shared<Receiver<()>>,
}

impl EncoderWrapper {
    pub fn new(device: &wgpu::Device) -> Self {
        let command_encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let (sender, receiver) = channel();

        Self {
            command_encoder,
            sender,
            shared: receiver.shared(),
        }
    }

    pub fn on_submit(&self) -> impl Future<Output = ()> {
        self.shared.clone().map(|res| res.unwrap())
    }

    pub fn inner(self) -> (wgpu::CommandEncoder, Tripper) {
        (
            self.command_encoder,
            Tripper {
                sender: self.sender,
            },
        )
    }
}

impl Deref for EncoderWrapper {
    type Target = wgpu::CommandEncoder;

    fn deref(&self) -> &Self::Target {
        &self.command_encoder
    }
}

impl DerefMut for EncoderWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.command_encoder
    }
}

impl Tripper {
    pub fn trip(self) {
        let _ = self.sender.send(());
    }
}
