//! A test image's report line, saved and printed as it is added.

use crate::boot::Report;
use core::fmt::{Arguments, Write as _};

/// Adds `line` to the report, saved, and prints it (the emulator's scenarios wait for it).
pub fn note(report: &mut Report, line: Arguments<'_>) {
    let _ = report.write_fmt(line);
    let _ = writeln!(report);
    report.save();
    esp_println::println!("{line}");
}
