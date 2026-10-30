//! `pw-pages --font FONT.cpfont [--text TEXT.txt] [--count N] [DIR]`: the battery measurement's
//! pages (rust/firmware/DESIGN.md, gate 1), `N` of them (120 by default, an hour of the page loop),
//! set from `TEXT.txt` (UTF-8, paragraphs split at blank lines) or from built-in sentences, written
//! as `DIR/001.bmp` on (`DIR` is `target/purestwhite-pages` by default). Copied onto the card as
//! `/purestwhite-pages`, both firmwares show them.
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

use purestwhite_app::LOOP_PAGES;
use purestwhite_render::Text;
use purestwhite_sim::bmp::write_bmp;
use purestwhite_sim::typeset::{FILLER, typeset};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: pw-pages --font FONT.cpfont [--text TEXT.txt] [--count N] [DIR]";

/// The command line.
struct Args {
    font: PathBuf,
    text: Option<PathBuf>,
    count: usize,
    out: PathBuf,
}

fn args() -> Result<Args, String> {
    let (mut font, mut text, mut out) = (None, None, None);
    let mut count = usize::from(LOOP_PAGES);
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--font" => font = Some(it.next().ok_or("--font needs a file")?.into()),
            "--text" => text = Some(it.next().ok_or("--text needs a file")?.into()),
            "--count" => {
                let n = it.next().ok_or("--count needs a number")?;
                count = n
                    .parse()
                    .ok()
                    .filter(|&n| (1..=999).contains(&n))
                    .ok_or("--count: 1 to 999")?;
            }
            _ if out.is_none() && !arg.starts_with("--") => out = Some(arg.into()),
            _ => return Err(format!("unexpected argument {arg:?}\n{USAGE}")),
        }
    }
    Ok(Args {
        font: font.ok_or(USAGE)?,
        text,
        count,
        out: out.unwrap_or_else(|| PathBuf::from("target/purestwhite-pages")),
    })
}

fn main_inner() -> Result<(), String> {
    let args = args()?;
    let read = |path: &PathBuf| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    let font_bytes = read(&args.font)?;
    let text = match &args.text {
        Some(path) => {
            String::from_utf8(read(path)?).map_err(|_| format!("{}: not UTF-8", path.display()))?
        }
        None => FILLER.to_string(),
    };
    let mut table = vec![0u8; 8192];
    let mut font = Text::open(&font_bytes, 0, &mut table).map_err(|e| format!("font: {e:?}"))?;
    std::fs::create_dir_all(&args.out).map_err(|e| format!("{}: {e}", args.out.display()))?;
    for (n, page) in (1..).zip(typeset(&text, &mut font, args.count)) {
        let path = args.out.join(format!("{n:03}.bmp"));
        let file = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        write_bmp(&page, std::io::BufWriter::new(file))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    println!("{} pages in {}", args.count, args.out.display());
    Ok(())
}

fn main() -> ExitCode {
    match main_inner() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("pw-pages: {message}");
            ExitCode::FAILURE
        }
    }
}
