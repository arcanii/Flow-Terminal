//! Server-sent events as MCP's Streamable HTTP sends them: each event's
//! data, and the stream's last event id and the delay the server asked
//! for before it's picked up again (`retry:`), which the assistant's own
//! reader (`provider::read_sse`) has no use for. An event with an id and no
//! data, which a server sends first so its stream can be resumed, still
//! moves the last id on.

use std::io::BufRead;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub struct Events<R> {
    lines: R,
    /// The id of the last event that had one.
    pub last_id: Option<String>,
    /// How long the server asked to be given before the stream is picked
    /// up again.
    pub retry: Option<Duration>,
}

impl<R: BufRead> Events<R> {
    pub fn new(lines: R) -> Events<R> {
        Events { lines, last_id: None, retry: None }
    }

    /// The next event's data, none once the stream ends (or `cancel` is
    /// set between its lines), or why it couldn't be read.
    pub fn next_data(&mut self, cancel: &AtomicBool) -> Option<Result<String, String>> {
        let mut data: Option<String> = None;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let mut line = String::new();
            match self.lines.read_line(&mut line) {
                Ok(0) => return data.map(Ok),
                Ok(_) => {}
                Err(e) => return Some(Err(e.to_string())),
            }
            let line = line.strip_suffix('\n').unwrap_or(&line);
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                // An event ends; one with no data only moved the id on.
                match data.take() {
                    Some(d) => return Some(Ok(d)),
                    None => continue,
                }
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
                None => (line, ""),
            };
            match field {
                "data" => match data.as_mut() {
                    Some(d) => {
                        d.push('\n');
                        d.push_str(value);
                    }
                    None => data = Some(value.to_string()),
                },
                "id" if !value.contains('\0') => self.last_id = Some(value.to_string()),
                "retry" => {
                    if let Ok(ms) = value.parse::<u64>() {
                        self.retry = Some(Duration::from_millis(ms));
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_keep_their_ids_and_the_delay_asked_for() {
        let stream = "id: s1-0\ndata:\n\n: a comment\nevent: message\nid: s1-1\nretry: 2500\ndata: {\"a\":\ndata: 1}\n\ndata: tail";
        let mut events = Events::new(stream.as_bytes());
        let cancel = AtomicBool::new(false);
        assert_eq!(events.next_data(&cancel), Some(Ok(String::new())), "a priming event's empty data is an event");
        assert_eq!(events.last_id.as_deref(), Some("s1-0"));
        assert_eq!(events.next_data(&cancel), Some(Ok("{\"a\":\n1}".into())));
        assert_eq!(events.last_id.as_deref(), Some("s1-1"));
        assert_eq!(events.retry, Some(Duration::from_millis(2500)));
        assert_eq!(events.next_data(&cancel), Some(Ok("tail".into())), "the last event, cut off, still counts");
        assert_eq!(events.next_data(&cancel), None);
        let mut stopped = Events::new("data: x\n\n".as_bytes());
        assert_eq!(stopped.next_data(&AtomicBool::new(true)), None);
    }
}
