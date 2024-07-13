use std::sync::mpsc::Sender;

#[derive(Default)]
pub(crate) struct Message {
    pub(crate) success: bool,
    pub(crate) message: String,
}

pub(crate) struct MessageSender {
    tx: Sender<Message>,
}

impl MessageSender {
    pub(crate) fn new(tx: Sender<Message>) -> Self {
        Self {
            tx,
        }
    }

    pub(crate) fn send(&self, message: String) {
        let _ = self.tx.send(Message {
            success: true,
            message,
        });
    }

    pub(crate) fn success(&self, message: String) {
        let _ = self.tx.send(Message {
            success: true,
            message,
        });
    }

    pub(crate) fn failed(&self, message: String) {
        let _ = self.tx.send(Message {
            success: false,
            message,
        });
    }
}