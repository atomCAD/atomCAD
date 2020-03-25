use crate::scene::Scene;

use iced_wgpu::Renderer;
use iced_winit::{
    Element, Row, Length, Align,
    widget::{
        text::Text,
    },
};

type Message = ();

pub struct Ui;

impl Ui {
    pub fn new() -> Self {
        Self
    }

    pub fn update(&mut self, _msg: Message, _scene: &mut Scene) {

    }

    pub fn view(&self, _scene: &Scene) -> Element<Message, Renderer> {
        Row::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .align_items(Align::End)
            .push(
                Text::new("This is renderering through Iced!")
                    .color([0.0, 0.0, 1.0])
                    .size(40)
            ).into()
    }
}