use crate::ImageContent;
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct QueuedInput {
    pub text: String,
    pub images: Vec<ImageContent>,
}

#[derive(Default)]
pub struct InputQueue {
    messages: Mutex<VecDeque<QueuedInput>>,
}

impl InputQueue {
    pub fn push(&self, text: String, images: Vec<ImageContent>) {
        self.messages
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_back(QueuedInput { text, images });
    }

    pub fn drain(&self) -> Vec<QueuedInput> {
        self.messages
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain(..)
            .collect()
    }

    pub fn discard_front(&self, text: &str) {
        let mut messages = self
            .messages
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if messages.front().is_some_and(|queued| queued.text == text) {
            messages.pop_front();
        }
    }
}
