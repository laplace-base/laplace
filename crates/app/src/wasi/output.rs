//! The stdout and stderr of a lapp backend, written to the host log line by line.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll};

use bytes::Bytes;
use log::Level;
use tokio::io::AsyncWrite;
use wasmtime_wasi::async_trait;
use wasmtime_wasi::cli::{IsTerminal, StdoutStream};
use wasmtime_wasi::p2::{OutputStream, Pollable, StreamResult};

/// The longest line that goes into the log; the rest of it is dropped, so that endless output
/// without a line break does not eat the host memory.
pub const MAX_LINE_LEN: usize = 4096;

const TRUNCATED_MARK: &str = " [truncated]";

/// Splits written bytes into lines and hands every one of them to `sink`.
pub struct LineWriter<S: FnMut(&str)> {
    line: Vec<u8>,
    truncated: bool,
    sink: S,
}

impl<S: FnMut(&str)> LineWriter<S> {
    pub fn new(sink: S) -> Self {
        Self {
            line: Vec::new(),
            truncated: false,
            sink,
        }
    }

    pub fn write(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let (part, rest, ends_line) = match bytes.iter().position(|&b| b == b'\n') {
                Some(end) => (&bytes[..end], &bytes[end + 1..], true),
                None => (bytes, &[][..], false),
            };

            let room = MAX_LINE_LEN.saturating_sub(self.line.len());
            self.line.extend_from_slice(&part[..part.len().min(room)]);
            self.truncated |= part.len() > room;

            if ends_line {
                self.emit();
            }
            bytes = rest;
        }
    }

    /// Hands over the line written so far without a line break.
    pub fn flush(&mut self) {
        if !self.line.is_empty() || self.truncated {
            self.emit();
        }
    }

    fn emit(&mut self) {
        let line = String::from_utf8_lossy(&self.line);
        let line = line.strip_suffix('\r').unwrap_or(&line);
        if self.truncated {
            (self.sink)(&format!("{line}{TRUNCATED_MARK}"));
        } else {
            (self.sink)(line);
        }
        self.line.clear();
        self.truncated = false;
    }
}

impl<S: FnMut(&str)> Drop for LineWriter<S> {
    fn drop(&mut self) {
        self.flush();
    }
}

type LogLineWriter = LineWriter<Box<dyn FnMut(&str) + Send>>;

/// An output of a lapp backend into the host log: target `lapp::<name>`, one record per line.
///
/// The clones, which WASI makes for every stream the guest opens, share the line.
#[derive(Clone)]
pub struct LogOutput(Arc<Mutex<LogLineWriter>>);

impl LogOutput {
    pub fn new(lapp_name: &str, level: Level) -> Self {
        let target = format!("lapp::{lapp_name}");
        let sink = Box::new(move |line: &str| log::log!(target: &target, level, "{line}"));

        Self(Arc::new(Mutex::new(LineWriter::new(sink))))
    }

    fn writer(&self) -> MutexGuard<'_, LogLineWriter> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes out the line the backend has left without a line break.
    pub fn flush(&self) {
        self.writer().flush();
    }
}

impl IsTerminal for LogOutput {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for LogOutput {
    fn p2_stream(&self) -> Box<dyn OutputStream> {
        Box::new(self.clone())
    }

    fn async_stream(&self) -> Box<dyn AsyncWrite + Send + Sync> {
        Box::new(self.clone())
    }
}

impl OutputStream for LogOutput {
    fn write(&mut self, bytes: Bytes) -> StreamResult<()> {
        self.writer().write(&bytes);
        Ok(())
    }

    // A flush of the guest does not end the line: the host writes it out after the call.
    fn flush(&mut self) -> StreamResult<()> {
        Ok(())
    }

    fn check_write(&mut self) -> StreamResult<usize> {
        Ok(64 * 1024)
    }
}

#[async_trait]
impl Pollable for LogOutput {
    async fn ready(&mut self) {}
}

impl AsyncWrite for LogOutput {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.writer().write(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn lines(write: impl FnOnce(&mut LineWriter<&mut dyn FnMut(&str)>)) -> Vec<String> {
        let lines = RefCell::new(Vec::new());
        let mut sink = |line: &str| lines.borrow_mut().push(line.to_owned());
        {
            let mut writer = LineWriter::new(&mut sink as &mut dyn FnMut(&str));
            write(&mut writer);
        }
        lines.into_inner()
    }

    #[test]
    fn one_write_of_several_lines() {
        assert_eq!(lines(|w| w.write(b"one\ntwo\r\n\nthree\n")), [
            "one", "two", "", "three"
        ]);
    }

    #[test]
    fn one_line_of_several_writes() {
        let written = lines(|w| {
            for part in ["on", "e\ntw", "o\n"] {
                w.write(part.as_bytes());
            }
        });
        assert_eq!(written, ["one", "two"]);
    }

    #[test]
    fn tail_without_a_line_break_on_flush_and_drop() {
        assert_eq!(
            lines(|w| {
                w.write(b"flushed");
                w.flush();
                w.flush();
                w.write(b"dropped");
            }),
            ["flushed", "dropped"]
        );
    }

    #[test]
    fn long_line_is_truncated() {
        let long = vec![b'x'; MAX_LINE_LEN * 3];
        let lines = lines(|w| {
            w.write(&long);
            w.write(b"x\nnext\n");
        });

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], format!("{}{TRUNCATED_MARK}", "x".repeat(MAX_LINE_LEN)));
        assert_eq!(lines[1], "next");
    }

    #[test]
    fn invalid_utf8_is_replaced() {
        assert_eq!(lines(|w| w.write(b"bad \xff byte\n")), ["bad \u{fffd} byte"]);
    }
}
