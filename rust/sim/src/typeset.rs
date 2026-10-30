//! Pages of text for the battery measurement (rust/firmware/DESIGN.md, gate 1): a text set in one
//! style of a font, line by line within margins, a page's number at its foot, starting the text
//! over when it runs out. `pw-pages` writes them as the BMPs both firmwares show.

use purestwhite_render::{FRAME_BYTES, HEIGHT, Text, WIDTH, canvas};

/// The space around the text: at each side, above it, and below it (the page's number is there).
pub const SIDE: i32 = 20;
pub const TOP: i32 = 24;
pub const FOOT: i32 = 48;

/// Text to set when none is given: plain sentences, about as dense as a novel's.
pub const FILLER: &str = "The lamp by the window had been burning since the evening began, and \
nobody in the house had thought to put it out. Outside, the street was quiet; a cart went by now \
and then, and the rain came and went as it pleased.

She turned the page and read on. The story had taken its time to start, but now it would not let \
her go: every chapter ended a little before she was ready, and every new one began somewhere she \
had not expected.

Later, when the clock in the hall struck eleven, she marked her place with a slip of paper and \
listened. The house was still. Only the rain kept talking, softly, to the glass.";

/// `count` pages of `text` set in `font`: its paragraphs (split at blank lines) start on a new
/// line, and its words wrap within the margins. A word wider than a line goes on a line of its
/// own. Each page is a panel frame, `FRAME_BYTES` long.
pub fn typeset(text: &str, font: &mut Text<'_>, count: usize) -> Vec<Vec<u8>> {
    let lines = lines(text, font);
    let line_height = font.line_height().max(1);
    let per_page = usize::try_from((HEIGHT - TOP - FOOT) / line_height)
        .unwrap_or(0)
        .max(1);
    let mut lines = lines.iter().cycle();
    (1..=count)
        .map(|number| {
            let page: Vec<&String> = lines.by_ref().take(per_page).collect();
            draw_page(&page, number, font)
        })
        .collect()
}

/// `text` broken into lines that fit between the side margins.
fn lines(text: &str, font: &mut Text<'_>) -> Vec<String> {
    let width = WIDTH - 2 * SIDE;
    let mut lines = Vec::new();
    for paragraph in text.split("\n\n") {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let longer = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if font.width(&longer) <= width || line.is_empty() {
                line = longer;
            } else {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// A page with `lines` and its `number` at the foot.
fn draw_page(lines: &[&String], number: usize, font: &mut Text<'_>) -> Vec<u8> {
    let mut frame = vec![0xFF; FRAME_BYTES];
    let mut c = canvas(&mut frame);
    let line_height = font.line_height();
    for (line, y) in lines
        .iter()
        .zip((TOP..).step_by(usize::try_from(line_height).unwrap_or(1)))
    {
        font.draw(&mut c, SIDE, y, line);
    }
    let number_y = HEIGHT - FOOT + (FOOT - line_height) / 2;
    font.draw_centered(&mut c, number_y, &number.to_string());
    frame
}
