use iced_wgpu::Renderer;
use iced_winit::{widget::text::Text, Align, Container, Element, Length, Space};

pub struct Fps {
    fps: Option<usize>,
}

impl Fps {
    pub fn new() -> Self {
        Self { fps: None }
    }

    pub fn set_fps(&mut self, fps: usize) {
        self.fps = Some(fps);
    }

    pub fn view<Message: 'static>(&self) -> Element<Message, Renderer> {
        let item: Element<_, _> = if let Some(fps) = self.fps {
            Text::new(fps.to_string())
                .color([0.0, 0.0, 0.0])
                .size(40)
                .into()
        } else {
            Space::new(Length::Shrink, Length::Shrink).into()
        };

        Container::new(item)
            .align_x(Align::Start)
            .align_y(Align::Start)
            .into()
    }
}
