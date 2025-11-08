use std::fmt;

/// A simple representation of a STOMP frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub command: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Frame {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn set_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Command: {}", self.command)?;
        for (k, v) in &self.headers {
            writeln!(f, "{}: {}", k, v)?;
        }
        writeln!(f, "Body ({} bytes)", self.body.len())
    }
}
