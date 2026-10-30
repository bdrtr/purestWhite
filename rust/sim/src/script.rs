//! Scenario scripts: one step per line, as `word` or `word:value`; `#` starts a comment.
//!
//! - `battery:N`: the battery at N percent;
//! - `time:HH:MM`: the time of day;
//! - `press:left`, `press:right`, `press:power`, `press:home`: a key pressed;
//! - `sleep`: Power held long enough to sleep;
//! - `wait:S`: S seconds pass (as eink-qemu's `wait`; to the millisecond, `wait:0.25`), and the
//!   application's timers due meanwhile fire, each at its time.
//!
//! The event words stand for events the services have already classified (a press, not the
//! samples of the pin); see `rust/firmware/DESIGN.md`.

use purestwhite_app::view::Time;
use purestwhite_app::{Event, Key};

/// A step of a scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// An event for the application.
    Event(Event),
    /// Time passes, in milliseconds.
    Wait(u64),
}

/// The steps of a scenario, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    steps: Vec<Step>,
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
        let mut steps = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let step = step(line).ok_or_else(|| ScriptError {
                line: index + 1,
                text: line.to_string(),
            })?;
            steps.push(step);
        }
        Ok(Script { steps })
    }

    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}

/// The step one line stands for.
fn step(line: &str) -> Option<Step> {
    let (word, value) = line.split_once(':').unwrap_or((line, ""));
    let event = match word {
        "wait" => return seconds(value).map(Step::Wait),
        "battery" => value.parse().ok().map(Event::Battery),
        "time" => time(value).map(Event::Time),
        "press" => key(value).map(Event::Pressed),
        "sleep" if value.is_empty() => Some(Event::SleepRequested),
        _ => None,
    };
    event.map(Step::Event)
}

/// Seconds, with up to three decimals, in milliseconds.
fn seconds(value: &str) -> Option<u64> {
    let (whole, part) = value.split_once('.').unwrap_or((value, ""));
    let digits = whole
        .bytes()
        .chain(part.bytes())
        .all(|b| b.is_ascii_digit());
    if whole.is_empty() || part.len() > 3 || !digits {
        return None;
    }
    let ms: u64 = format!("{part:0<3}").parse().ok()?;
    whole
        .parse::<u64>()
        .ok()?
        .checked_mul(1000)?
        .checked_add(ms)
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
        let time = Time {
            hour: 21,
            minute: 5,
        };
        assert_eq!(
            script.steps(),
            [
                Event::Battery(87),
                Event::Time(time),
                Event::Pressed(Key::Left),
                Event::SleepRequested,
            ]
            .map(Step::Event)
        );
    }

    #[test]
    fn a_wait_is_in_seconds_to_the_millisecond() {
        let script = Script::parse("wait:600\nwait:0.25\nwait:1.5\nwait:2.125").unwrap();
        assert_eq!(script.steps(), [600_000, 250, 1_500, 2_125].map(Step::Wait));
    }

    #[test]
    fn a_bad_line_is_reported_with_its_number() {
        for (text, line) in [
            ("battery:87\nbattery:lots", 2),
            ("time:25:00", 1),
            ("press:up", 1),
            ("sleep:now", 1),
            ("wait", 1),
            ("wait:1.2345", 1),
            ("wait:-1", 1),
            ("wait:+1", 1),
            ("wait:.5", 1),
            ("idle", 1),
            ("\n\nwhatever", 3),
        ] {
            assert_eq!(Script::parse(text).unwrap_err().line, line, "{text}");
        }
    }
}
