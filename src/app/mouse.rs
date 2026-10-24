use super::actions::Action;
use crossterm::event::{Event, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

pub struct EventHandler {
    accept_mouse: bool,
    event: Option<Event>,
    action: Option<super::actions::Action>,
}

impl EventHandler {
    pub fn new() -> Self {
        Self {
            accept_mouse: true,
            event: None,
            action: None,
        }
    }

    pub fn set_accept_mouse(&mut self, accept: bool) {
        self.accept_mouse = accept;
    }

    #[inline]
    pub fn publish_event(&mut self, event: Event) {
        self.event = Some(event);
    }

    /// Whether the published event is a mouse event that can trigger an action.
    #[inline]
    pub fn has_pending_mouse(&self) -> bool {
        self.accept_mouse
            && matches!(&self.event, Some(Event::Mouse(mouse)) if mouse.kind != MouseEventKind::Moved)
    }

    pub fn on_mouse<F>(&mut self, area: Rect, cb: F)
    where
        F: FnOnce(&MouseEvent) -> Option<Action>,
    {
        if !self.accept_mouse {
            return;
        }

        if let Some(Event::Mouse(mouse)) = self.event.as_ref() {
            if area.intersects(Rect {
                x: mouse.column,
                y: mouse.row,
                width: 1,
                height: 1,
            }) {
                let action = cb(mouse);
                if action.is_some() {
                    self.action = action;
                }
            }
        }
    }

    #[inline]
    pub fn extract(&mut self) -> Option<Action> {
        self.event = None;
        self.action.take()
    }
}
