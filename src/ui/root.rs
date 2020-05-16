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
                    .color([0.0, 0.0, 0.0])
                    .size(40),
            )
            .into()
    }
}
