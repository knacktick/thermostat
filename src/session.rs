use super::command_parser::{Command, Error as ParserError};

const MAX_LINE_LEN: usize = 64;

struct LineReader {
    buf: [u8; MAX_LINE_LEN],
    pos: usize,
}

impl LineReader {
    pub fn new() -> Self {
        LineReader {
            buf: [0; MAX_LINE_LEN],
            pos: 0,
        }
    }

    pub fn feed(&mut self, c: u8) -> Option<&[u8]> {
        if c == 13 || c == 10 {
            // Enter
            if self.pos > 0 {
                let len = self.pos;
                self.pos = 0;
                Some(&self.buf[..len])
            } else {
                None
            }
        } else if self.pos < self.buf.len() {
            // Add input
            self.buf[self.pos] = c;
            self.pos += 1;
            None
        } else {
            // Buffer is full, ignore
            None
        }
    }
}

pub enum SessionInput {
    Nothing,
    Command(Command),
    Error(ParserError),
}

impl From<Result<Command, ParserError>> for SessionInput {
    fn from(input: Result<Command, ParserError>) -> Self {
        input
            .map(SessionInput::Command)
            .unwrap_or_else(SessionInput::Error)
    }
}

pub struct Session {
    reader: LineReader,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Session {
            reader: LineReader::new(),
        }
    }

    pub fn reset(&mut self) {
        self.reader = LineReader::new();
    }

    pub fn feed(&mut self, buf: &[u8]) -> (usize, SessionInput) {
        let mut buf_bytes = 0;
        for (i, b) in buf.iter().enumerate() {
            buf_bytes = i + 1;
            let line = self.reader.feed(*b);
            if let Some(line) = line {
                let command = Command::parse(line);
                return (buf_bytes, command.into());
            }
        }
        (buf_bytes, SessionInput::Nothing)
    }
}
