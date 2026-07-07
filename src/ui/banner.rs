//! Block-style ASCII banner spelling `landingpig` on the boot screen.

const GLYPH_WIDTH: usize = 6;
const GLYPH_HEIGHT: usize = 8;
const LETTER_GAP: &str = " ";

type Glyph = [&'static str; GLYPH_HEIGHT];

const L: Glyph = [
    "█     ",
    "█     ",
    "█     ",
    "█     ",
    "█     ",
    "█     ",
    "█     ",
    "██████",
];

const A: Glyph = [
    " ████ ",
    "█    █",
    "█    █",
    "██████",
    "█    █",
    "█    █",
    "█    █",
    "█    █",
];

const N: Glyph = [
    "█    █",
    "██   █",
    "█ █  █",
    "█  █ █",
    "█   ██",
    "█    █",
    "█    █",
    "█    █",
];

const D: Glyph = [
    "    █ ",
    "    █ ",
    " ████ ",
    "█    █",
    "█    █",
    "█    █",
    "█    █",
    " ████ ",
];

const I: Glyph = [
    " ████ ",
    "  ██  ",
    "  ██  ",
    "  ██  ",
    "  ██  ",
    "  ██  ",
    "  ██  ",
    " ████ ",
];

const G: Glyph = [
    " ████ ",
    "█     ",
    "█     ",
    "█ ███ ",
    "█    █",
    "█    █",
    " ████ ",
    "    █ ",
];

const P: Glyph = [
    "██████",
    "█    █",
    "█    █",
    "██████",
    "█     ",
    "█     ",
    "█     ",
    "█     ",
];

fn char_width(text: &str) -> usize {
    text.chars().count()
}

fn validate_glyph(glyph: Glyph) {
    for line in glyph {
        debug_assert_eq!(char_width(line), GLYPH_WIDTH);
    }
}

fn glyph_for(c: char) -> Glyph {
    let glyph = match c {
        'l' => L,
        'a' => A,
        'n' => N,
        'd' => D,
        'i' => I,
        'g' => G,
        'p' => P,
        _ => ["      "; GLYPH_HEIGHT],
    };
    validate_glyph(glyph);
    glyph
}

/// Build aligned rows for a word using fixed-width block glyphs.
pub fn build_word_lines(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut rows = vec![String::new(); GLYPH_HEIGHT];

    for (idx, ch) in chars.iter().enumerate() {
        let glyph = glyph_for(*ch);
        for (row, line) in glyph.iter().enumerate() {
            if idx > 0 {
                rows[row].push_str(LETTER_GAP);
            }
            rows[row].push_str(line);
        }
    }

    rows
}

/// Full banner: box frame, block art, subtitle — equal display width on every row.
pub fn build_banner(width: u16) -> Vec<String> {
    let art = build_word_lines("landingpig");
    let art_width = art.first().map(|l| char_width(l)).unwrap_or(0);
    let box_width = (art_width + 4).max(76).min(width as usize);

    let horizontal = "═".repeat(box_width - 2);
    let mut lines = vec![
        format!("╔{horizontal}╗"),
        format!("║{}║", " ".repeat(box_width - 2)),
    ];

    for row in &art {
        let row_width = char_width(row);
        let pad_left = (box_width - 2).saturating_sub(row_width) / 2;
        let pad_right = (box_width - 2).saturating_sub(row_width) - pad_left;
        lines.push(format!(
            "║{}{}{}║",
            " ".repeat(pad_left),
            row,
            " ".repeat(pad_right)
        ));
    }

    let subtitle = "landing page engine";
    let sub_pad_left = (box_width - 2).saturating_sub(subtitle.len()) / 2;
    let sub_pad_right = (box_width - 2).saturating_sub(subtitle.len()) - sub_pad_left;
    lines.push(format!("║{}║", " ".repeat(box_width - 2)));
    lines.push(format!(
        "║{}{}{}║",
        " ".repeat(sub_pad_left),
        subtitle,
        " ".repeat(sub_pad_right)
    ));
    lines.push(format!("║{}║", " ".repeat(box_width - 2)));
    lines.push(format!("╚{horizontal}╝"));

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_are_fixed_width() {
        for ch in "landingpig".chars() {
            let glyph = glyph_for(ch);
            for line in glyph {
                assert_eq!(char_width(line), GLYPH_WIDTH);
            }
        }
    }

    #[test]
    fn banner_rows_are_equal_width() {
        let lines = build_banner(120);
        let widths: Vec<usize> = lines.iter().map(|l| char_width(l)).collect();
        let first = widths[0];
        for (i, w) in widths.iter().enumerate() {
            assert_eq!(*w, first, "row {i} width {w} != {first}");
        }
    }

    #[test]
    fn word_rows_are_equal_width() {
        let rows = build_word_lines("landingpig");
        let first = char_width(&rows[0]);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(char_width(row), first, "art row {i}");
        }
    }
}
