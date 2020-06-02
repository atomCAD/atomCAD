// Copyright (c) 2020 by Lachlan Sneff <lachlan@charted.space>
// Copyright (c) 2020 by Mark Friedenbach <mark@friedenbach.org>
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use iced_wgpu::Renderer;
use iced_winit::{widget::text::Text, Align, Element, Length, Row};

use crate::scene::Event as SceneEvent;

type Message = ();

pub struct Root;

impl Root {
    pub fn new() -> Self {
        Self
    }

    pub fn update(&mut self, _msg: Message, _scene_events: &mut Vec<SceneEvent>) {}

    pub fn view(&self) -> Element<Message, Renderer> {
        Row::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .align_items(Align::End)
            .push(
                Text::new("This is renderering through Iced!")
                    .color([0.0, 0.0, 1.0])
                    .size(40),
            )
            .into()
    }
}
