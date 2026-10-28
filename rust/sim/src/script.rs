//! Scenario scripts: one event per line, as `word` or `word:value`; `#` starts a comment.
//!
//! - `battery:N`: the battery at N percent;
//! - `time:HH:MM`: the time of day;
//! - `press:left`, `press:right`, `press:power`, `press:home`: a key pressed;
//! - `sleep`: Power held long enough to sleep.
//!
//! The words stand for events the services have already classified (a press, not the samples of
//! the pin); see `rust/firmware/DESIGN.md`.

use purestwhite_app::view::Time;
use purestwhite_app::{Event, Key};

/// The events of a scenario, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    events: Vec<Event>,
}

/// A line the parser did not understand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    /// From 1.
    pub line: usize,
    pub text: String,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: cannot read {:?}", self.line, self.text)
    }
}

impl std::error::Error for ScriptError {}

impl Script {
    /// The script in `text`.
    ///
    /// # Errors
    /// The first line that is not an event.
    pub fn parse(text: &str) -> Result<Self, ScriptError> {
        let mut events = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let event = event(line).ok_or_else(|| ScriptError {
                line: index + 1,
                text: line.to_string(),
            })?;
            events.push(event);
        }
        Ok(Script { events })
    }

    #[must_use]
    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

/// The event one line stands for.
fn event(line: &str) -> Option<Event> {
    let (word, value) = line.split_once(':').unwrap_or((line, ""));
    match word {
        "battery" => value.parse().ok().map(Event::Battery),
        "time" => time(value).map(Event::Time),
        "press" => key(value).map(Event::Pressed),
        "sleep" if value.is_empty() => Some(Event::SleepRequested),
        _ => None,
    }
}

fn time(value: &str) -> Option<Time> {
    let (hour, minute) = value.split_once(':')?;
    let time = Time {
        hour: hour.parse().ok()?,
        minute: minute.parse().ok()?,
    };
    (time.hour < 24 && time.minute < 60).then_some(time)
}

fn key(value: &str) -> Option<Key> {
    match value {
        "left" => Some(Key::Left),
        "right" => Some(Key::Right),
        "power" => Some(Key::Power),
        "home" => Some(Key::Home),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "a failure fails the test")]
mod tests {
    use super::*;

    #[test]
    fn it_reads_every_event_and_skips_comments() {
        let script =
            Script::parse("# a comment\nbattery:87\ntime:21:05  # evening\n\npress:left\nsleep\n")
                .unwrap();
        assert_eq!(
            script.events(),
            [
                Event::Battery(87),
                Event::Time(Time {
                    hour: 21,
                    minute: 5
                }),
                Event::Pressed(Key::Left),
                Event::SleepRequested,
            ]
        );
    }

    #[test]
    fn a_bad_line_is_reported_with_its_number() {
        for (text, line) in [
            ("battery:87\nbattery:lots", 2),
            ("time:25:00", 1),
            ("press:up", 1),
            ("sleep:now", 1),
            ("idle", 1),
            ("\n\nwhatever", 3),
        ] {
            assert_eq!(Script::parse(text).unwrap_err().line, line, "{text}");
        }
    }
}
