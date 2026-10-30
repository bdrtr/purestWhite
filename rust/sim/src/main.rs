//! `pw-sim [--font FONT.cpfont] [--out DIR] SCRIPT`: runs a scenario of the bare-metal firmware's
//! application on the PC and writes each frame the panel showed as `DIR/NNNN-<refresh>.png`
//! (`DIR` is `target/sim-out` by default; the shots of an earlier run there are removed first).
//! It prints a line per frame, with the simulator's time it was shown, and `sleep` when the
//! application asked to sleep.
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

use purestwhite_sim::{RunError, Script, clear_shots, run, shot_name, write_png};
use std::path::PathBuf;
use std::process::ExitCode;

/// The command line.
struct Args {
    font: Option<PathBuf>,
    out: PathBuf,
    script: PathBuf,
}

fn args() -> Result<Args, String> {
    let mut font = None;
    let mut out = PathBuf::from("target/sim-out");
    let mut script = None;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--font" => font = Some(it.next().ok_or("--font needs a file")?.into()),
            "--out" => out = it.next().ok_or("--out needs a directory")?.into(),
            _ if script.is_none() && !arg.starts_with("--") => script = Some(arg.into()),
            _ => return Err(format!("unexpected argument {arg:?}")),
        }
    }
    let script = script.ok_or("usage: pw-sim [--font FONT.cpfont] [--out DIR] SCRIPT")?;
    Ok(Args { font, out, script })
}

fn read(path: &PathBuf) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn main_inner() -> Result<(), String> {
    let args = args()?;
    let text = String::from_utf8(read(&args.script)?)
        .map_err(|_| format!("{}: not UTF-8", args.script.display()))?;
    let script = Script::parse(&text).map_err(|e| e.to_string())?;
    let font = args.font.as_ref().map(read).transpose()?;
    let run = run(&script, font.as_deref()).map_err(|e| match e {
        RunError::Font(e) => format!("font: {e:?}"),
        RunError::Stuck(None) => "the loops did not settle on the first screen".into(),
        RunError::Stuck(Some(i)) => {
            format!("the loops did not settle after the script's step {}", i + 1)
        }
    })?;
    let out = &args.out;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    clear_shots(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (index, shot) in run.shots.iter().enumerate() {
        let name = shot_name(index, shot.refresh);
        let path = out.join(&name);
        let file = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        write_png(shot, std::io::BufWriter::new(file)).map_err(|e| format!("{name}: {e}"))?;
        println!("{name} at {}.{:03} s", shot.at_ms / 1000, shot.at_ms % 1000);
    }
    if run.slept {
        println!("sleep");
    }
    Ok(())
}

fn main() -> ExitCode {
    match main_inner() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("pw-sim: {message}");
            ExitCode::FAILURE
        }
    }
}
