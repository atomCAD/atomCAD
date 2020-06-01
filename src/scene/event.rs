use std::convert::TryFrom;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, WindowEvent},
};

#[derive(Debug)]
pub struct Resize {
    pub new_texture: wgpu::Texture,
    pub size: PhysicalSize<u32>,
}

#[derive(Debug, Default)]
pub struct Events {
    pub resize: Option<Resize>,
    pub events: Vec<Event>,
}

#[derive(Debug)]
pub enum Event {
    MouseInput {
        button: MouseButton,
        state: ElementState,
    },
    CursorMoved {
        new_pos: PhysicalPosition<u32>,
    },
    CursorLeft,
}

pub struct NotApplicable;

impl TryFrom<&'_ WindowEvent<'_>> for Event {
    type Error = NotApplicable;

    fn try_from(window_event: &WindowEvent) -> Result<Self, Self::Error> {
        Ok(match *window_event {
            WindowEvent::MouseInput { state, button, .. } => Event::MouseInput { state, button },
            WindowEvent::CursorMoved { position, .. } => Event::CursorMoved {
                new_pos: position.cast(),
            },
            WindowEvent::CursorLeft { .. } => Event::CursorLeft,
            WindowEvent::Resized(_) => {
                // This window event is special and should be handled by the
                // the hub.
                unreachable!(
                    "the WindowEvent::Resized event is special and should be handled by the hub"
                )
            }
            _ => return Err(NotApplicable),
        })
    }
}
