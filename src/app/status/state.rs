use std::{
    borrow::Cow,
    ops::Deref,
    time::{Duration, Instant},
};

pub struct StatusState {
    message: String,
    timestamp: Option<(Instant, Duration)>,
}

impl StatusState {
    pub const fn new() -> Self {
        Self {
            message: String::new(),
            timestamp: None,
        }
    }

    pub fn msg<T>(&mut self, message: T)
    where
        T: Into<String>,
    {
        self.msg_with_duration(message.into(), Some(Duration::from_secs(2)))
    }

    pub fn msg_with_duration(&mut self, message: String, duration: Option<Duration>) {
        if message.is_empty() {
            self.message.clear();
            self.timestamp = None;
        } else {
            self.message = message;
            self.timestamp = duration.map(|dur| (Instant::now(), dur));
        }
    }

    pub fn get_message_update(&mut self) -> Option<Cow<'_, str>> {
        if let Some((time, dur)) = self.timestamp {
            if time.elapsed() > dur {
                self.timestamp = None;
                let message = std::mem::take(&mut self.message);
                return Some(Cow::Owned(message));
            }
        }
        (!self.message.is_empty()).then(|| Cow::Borrowed(self.message.deref()))
    }
}
