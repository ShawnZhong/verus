use std::{any::Any, sync::Arc};

use serde::{Deserialize, Serialize};

pub type ArcDynMessage = Arc<dyn Any + Send + Sync>;
pub type ArcDynMessageLabel = Arc<dyn Any + Send + Sync>;

pub trait MessageInterface {
    fn empty(&self) -> ArcDynMessage;
    fn message_label_from_air_span(&self, air_span: &str, note: &str) -> ArcDynMessage;
    fn all_msgs(&self, message: &ArcDynMessage) -> Vec<String>;
    fn bare(&self, level: MessageLevel, notes: &str) -> ArcDynMessage;
    fn unexpected_z3_version(&self, expected: &str, found: &str) -> ArcDynMessage;
    fn get_note<'b>(&self, message: &'b ArcDynMessage) -> &'b str;
    /// The message's primary span as a string, if it has one. Used by the
    /// SMT-log emitter to prefix each labeled assertion with a
    /// `;; file:line:col` comment so the explorer's span-link plugin can
    /// surface it as a clickable jump-to-source target.
    fn get_span_as_string<'b>(&self, message: &'b ArcDynMessage) -> Option<&'b str>;
    fn from_labels(&self, labels: &Vec<ArcDynMessageLabel>) -> ArcDynMessage;
    fn append_labels(
        &self,
        message: &ArcDynMessage,
        labels: &Vec<ArcDynMessageLabel>,
    ) -> ArcDynMessage;
    fn get_message_label_note<'b>(&self, message_label: &'b ArcDynMessageLabel) -> &'b str;
    /// A message-label's span as a string. Counterpart to
    /// `get_span_as_string` for `LabeledAxiom`, which carries a list of
    /// `ArcDynMessageLabel` rather than a single message.
    fn get_message_label_span_as_string<'b>(&self, message_label: &'b ArcDynMessageLabel) -> &'b str;
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum MessageLevel {
    Error,
    Warning,
    Note,
}

pub trait Diagnostics {
    /// Display the corresponding message
    fn report(&self, msg: &ArcDynMessage);

    /// Immediately display the message, regardless of which module is currently printing
    fn report_now(&self, msg: &ArcDynMessage);

    /// Override the msg's reporting level
    fn report_as(&self, msg: &ArcDynMessage, msg_as: MessageLevel);

    /// Call report_as on each message
    /// (And, optionally, sort the messages by span before calling report_as on each.)
    fn report_as_multi(&self, msgs: Vec<(ArcDynMessage, MessageLevel)>) {
        for (msg, msg_as) in msgs {
            self.report_as(&msg, msg_as);
        }
    }

    /// Override the msg's reporting level and immediately display the message
    fn report_as_now(&self, msg: &ArcDynMessage, msg_as: MessageLevel);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AirSpan {
    pub as_string: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AirMessageLabel {
    pub span: AirSpan,
    pub note: String,
}

/// Very simple implementation of Diagnostics for use in AIR
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AirMessage {
    pub level: MessageLevel,
    pub note: String,
    pub span: Option<AirSpan>,
    pub labels: Vec<AirMessageLabel>,
}

pub struct AirMessageInterface {}

impl MessageInterface for AirMessageInterface {
    fn empty(&self) -> ArcDynMessage {
        Arc::new(AirMessage {
            level: MessageLevel::Error,
            labels: Vec::new(),
            note: "".to_owned(),
            span: None,
        })
    }

    fn all_msgs(&self, message: &ArcDynMessage) -> Vec<String> {
        let message = message.downcast_ref::<AirMessage>().unwrap();
        Some(message.note.clone())
            .into_iter()
            .chain(message.labels.iter().map(|l| l.note.clone()))
            .collect()
    }

    fn bare(&self, level: MessageLevel, msg: &str) -> ArcDynMessage {
        Arc::new(AirMessage { level, note: msg.to_owned(), labels: Vec::new(), span: None })
    }

    fn unexpected_z3_version(&self, expected: &str, found: &str) -> ArcDynMessage {
        Arc::new(AirMessage {
            level: MessageLevel::Error,
            note: format!("expected z3 version {expected}, found {found}"),
            labels: Vec::new(),
            span: None,
        })
    }

    fn get_message_label_note<'b>(&self, message_label: &'b ArcDynMessageLabel) -> &'b str {
        let message_label = message_label.downcast_ref::<AirMessageLabel>().unwrap();
        &message_label.note
    }

    fn get_message_label_span_as_string<'b>(&self, message_label: &'b ArcDynMessageLabel) -> &'b str {
        let message_label = message_label.downcast_ref::<AirMessageLabel>().unwrap();
        &message_label.span.as_string
    }

    fn append_labels(
        &self,
        message: &ArcDynMessage,
        labels: &Vec<ArcDynMessageLabel>,
    ) -> ArcDynMessage {
        let message = message.downcast_ref::<AirMessage>().unwrap();
        let mut m = message.clone();
        for l in labels {
            let l = l.downcast_ref::<AirMessageLabel>().unwrap().clone();
            m.labels.push(l.clone());
        }
        Arc::new(m)
    }

    fn get_note<'b>(&self, message: &'b ArcDynMessage) -> &'b str {
        let message = message.downcast_ref::<AirMessage>().unwrap();
        &message.note
    }

    fn get_span_as_string<'b>(&self, message: &'b ArcDynMessage) -> Option<&'b str> {
        let message = message.downcast_ref::<AirMessage>().unwrap();
        message.span.as_ref().map(|s| s.as_string.as_str())
    }

    fn message_label_from_air_span(&self, air_span: &str, note: &str) -> ArcDynMessageLabel {
        Arc::new(AirMessageLabel {
            span: AirSpan { as_string: air_span.to_owned() },
            note: note.to_owned(),
        })
    }

    fn from_labels(&self, labels: &Vec<ArcDynMessageLabel>) -> ArcDynMessage {
        if labels.len() == 0 {
            Arc::new(AirMessage {
                level: MessageLevel::Error,
                labels: Vec::new(),
                note: "".to_owned(),
                span: None,
            })
        } else {
            let AirMessageLabel { span, note } =
                labels[0].downcast_ref::<AirMessageLabel>().unwrap().clone();
            Arc::new(AirMessage {
                span: Some(span),
                level: MessageLevel::Error,
                note: note.clone(),
                labels: labels[1..]
                    .iter()
                    .map(|l| l.downcast_ref::<AirMessageLabel>().unwrap().clone())
                    .collect(),
            })
        }
    }
}

pub struct Reporter {}

impl Diagnostics for Reporter {
    fn report_as(&self, msg: &ArcDynMessage, level: MessageLevel) {
        let msg = msg.downcast_ref::<AirMessage>().unwrap();
        use MessageLevel::*;
        match level {
            Note => println!("Note: {}", msg.note),
            Warning => println!("Warning: {}", msg.note),
            Error => eprintln!("Error: {}", msg.note),
        }
    }

    fn report(&self, msg: &ArcDynMessage) {
        let air_msg = msg.downcast_ref::<AirMessage>().unwrap();
        self.report_as(msg, air_msg.level)
    }

    fn report_now(&self, msg: &ArcDynMessage) {
        let air_msg = msg.downcast_ref::<AirMessage>().unwrap();
        self.report_as(msg, air_msg.level)
    }

    fn report_as_now(&self, msg: &ArcDynMessage, msg_as: MessageLevel) {
        self.report_as(msg, msg_as)
    }
}
