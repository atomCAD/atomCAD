// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use iced_wgpu::Renderer;
use iced_winit::{Align, Element, Length, Row};

use crate::{
    ui::fps,
    rendering::scene::Event as SceneEvent,
};

pub enum Message {}
pub struct Root {
    pub fps: fps::Fps,
}

impl Root {
    pub fn new() -> Self {
        Self {
            fps: fps::Fps::new(),
        }
    }

    pub fn update(&mut self, msg: Message, _scene_events: &mut Vec<SceneEvent>) {
        match msg {}
    }

    pub fn view(&self) -> Element<Message, Renderer> {
        let counter = self.fps.view();

        Row::new()
            .width(Length::Shrink)
            .height(Length::Shrink)
            .align_items(Align::Start)
            .push(counter)
            .into()
        // Row::new()
        //     .width(Length::Fill)
        //     .height(Length::Fill)
        //     .align_items(Align::End)
        //     .push(
        //         Text::new("This is renderering through Iced!")
        //             .color([0.0, 0.0, 1.0])
        //             .size(40),
        //     )
        //     .into()
    }
}
