//! The profile table, drawn the way rich 13.9 drew it for awspick 2.x.
//!
//! rich was used for one `Table` with the HEAVY_HEAD box, a title, four
//! columns and a section line between groups. This module reproduces that
//! output byte for byte, colours included, for tables that fit the terminal.
//! rich also folded tables wider than the terminal; that is not reproduced.
//!
//! Cell text is printed literally. rich parsed it as console markup, which
//! is how a profile named `a[/b]` crashed the Python release.

use std::env;
use std::io::IsTerminal;

use unicode_width::UnicodeWidthStr;

use crate::out;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSystem {
    Standard,
    EightBit,
    TrueColor,
}

/// How rich would have styled output on stderr.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Not a terminal, or a dumb one: no escape codes at all.
    Plain,
    /// A terminal with `NO_COLOR` set: bold and italic, no colours.
    NoColor,
    Color(ColorSystem),
}

impl Mode {
    pub fn detect() -> Mode {
        let force = env::var_os("FORCE_COLOR").is_some();
        let is_terminal = force || std::io::stderr().is_terminal();
        let term = env::var("TERM").unwrap_or_default().to_lowercase();
        let dumb = is_terminal && (term == "dumb" || term == "unknown");
        if !is_terminal || dumb {
            return Mode::Plain;
        }
        if env::var_os("NO_COLOR").is_some() {
            return Mode::NoColor;
        }
        let colorterm = env::var("COLORTERM").unwrap_or_default();
        let colorterm = crate::py::strip(&colorterm).to_lowercase();
        if colorterm == "truecolor" || colorterm == "24bit" {
            return Mode::Color(ColorSystem::TrueColor);
        }
        let term = crate::py::strip(&term).to_owned();
        let colors = term.rsplit_once('-').map(|(_, c)| c).unwrap_or(&term);
        Mode::Color(match colors {
            "kitty" | "256color" => ColorSystem::EightBit,
            _ => ColorSystem::Standard,
        })
    }
}

/// A style as rich resolved it: attributes and an optional foreground.
#[derive(Clone, Copy)]
struct Style {
    bold: bool,
    italic: bool,
    color: Option<Color>,
}

#[derive(Clone, Copy)]
enum Color {
    Red,
    Green,
    Blue,
    Cyan,
    White,
    Orange3,
}

const fn st(bold: bool, italic: bool, color: Option<Color>) -> Style {
    Style {
        bold,
        italic,
        color,
    }
}

const BORDER: Style = st(true, false, Some(Color::Blue));
const TITLE: Style = st(false, true, None);
const HEADER: Style = st(true, false, None);
const NUMBER: Style = st(false, false, Some(Color::Cyan));
const TEXT: Style = st(false, false, Some(Color::White));
const MARKER: Style = st(true, false, Some(Color::Green));
const EMPTY_MSG: Style = st(true, false, Some(Color::Red));

fn group_style(group: &str) -> Style {
    match group {
        "prod" => st(true, false, Some(Color::Red)),
        "preprod" => st(true, false, Some(Color::Green)),
        "stg" => st(true, false, Some(Color::Orange3)),
        "dev" => st(true, false, Some(Color::Blue)),
        "others" => st(true, false, Some(Color::White)),
        _ => TEXT,
    }
}

fn sgr(style: Style, mode: Mode) -> Option<String> {
    let (color_ok, system) = match mode {
        Mode::Plain => return None,
        Mode::NoColor => (false, ColorSystem::Standard),
        Mode::Color(s) => (true, s),
    };
    let mut codes: Vec<String> = Vec::new();
    if style.bold {
        codes.push("1".into());
    }
    if style.italic {
        codes.push("3".into());
    }
    if color_ok && let Some(c) = style.color {
        codes.push(
            match c {
                Color::Red => "31",
                Color::Green => "32",
                Color::Blue => "34",
                Color::Cyan => "36",
                Color::White => "37",
                Color::Orange3 => match system {
                    ColorSystem::Standard => "33",
                    _ => "38;5;172",
                },
            }
            .into(),
        );
    }
    if codes.is_empty() {
        None
    } else {
        Some(codes.join(";"))
    }
}

/// One rich segment: styled text is wrapped in its own SGR and reset.
fn seg(buf: &mut String, text: &str, style: Style, mode: Mode) {
    if text.is_empty() {
        return;
    }
    match sgr(style, mode) {
        Some(code) => {
            buf.push_str("\x1b[");
            buf.push_str(&code);
            buf.push('m');
            buf.push_str(text);
            buf.push_str("\x1b[0m");
        }
        None => buf.push_str(text),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Justify {
    Left,
    Right,
    Center,
}

fn width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Split the free space around a cell the way rich justifies text.
fn fill(content: usize, col: usize, justify: Justify) -> (usize, usize) {
    let excess = col.saturating_sub(content);
    match justify {
        Justify::Left => (0, excess),
        Justify::Right => (excess, 0),
        Justify::Center => (excess / 2, excess - excess / 2),
    }
}

/// A cell body. `markup` marks text that awspick 2.x wrapped in rich markup
/// (`[bold red]prod[/bold red]`): rich kept such a span apart from the
/// justification padding, while plain text and its padding came out as one
/// segment.
struct Cell {
    text: String,
    style: Style,
    markup: bool,
}

impl Cell {
    fn plain(text: &str, base: Style) -> Cell {
        Cell {
            text: text.to_owned(),
            style: base,
            markup: false,
        }
    }
    fn marked(text: &str, style: Style) -> Cell {
        Cell {
            text: text.to_owned(),
            style,
            markup: true,
        }
    }
    fn width(&self) -> usize {
        width(&self.text)
    }
}

fn render_cell(
    buf: &mut String,
    cell: &Cell,
    col: usize,
    justify: Justify,
    base: Style,
    mode: Mode,
) {
    seg(buf, " ", base, mode);
    let (left, right) = fill(cell.width(), col, justify);
    if cell.markup {
        seg(buf, &" ".repeat(left), base, mode);
        seg(buf, &cell.text, cell.style, mode);
        seg(buf, &" ".repeat(right), base, mode);
    } else {
        let text = format!("{}{}{}", " ".repeat(left), cell.text, " ".repeat(right));
        seg(buf, &text, cell.style, mode);
    }
    seg(buf, " ", base, mode);
}

struct Column {
    header: &'static str,
    justify: Justify,
    base: Style,
}

const COLUMNS: [Column; 4] = [
    Column {
        header: "No.",
        justify: Justify::Right,
        base: NUMBER,
    },
    Column {
        header: "Profile",
        justify: Justify::Left,
        base: TEXT,
    },
    Column {
        header: "Group",
        justify: Justify::Left,
        base: TEXT,
    },
    Column {
        header: "Current",
        justify: Justify::Center,
        base: TEXT,
    },
];

fn rule(
    buf: &mut String,
    widths: &[usize],
    left: char,
    line: char,
    mid: char,
    right: char,
    mode: Mode,
) {
    let mut s = String::new();
    s.push(left);
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            s.push(mid);
        }
        s.extend(std::iter::repeat_n(line, w + 2));
    }
    s.push(right);
    seg(buf, &s, BORDER, mode);
    buf.push('\n');
}

/// Render the table (or the empty-list message) to a string.
pub fn render(grouped: &[(String, String)], current: Option<&str>, mode: Mode) -> String {
    let mut buf = String::new();
    if grouped.is_empty() {
        // rich's highlighter split the path into three segments.
        for part in ["No AWS profiles found in ~", "/.aws/", "config"] {
            seg(&mut buf, part, EMPTY_MSG, mode);
        }
        buf.push('\n');
        return buf;
    }
    let current_norm = current.map(|c| crate::py::strip(&c.to_lowercase()).to_owned());

    let rows: Vec<[Cell; 4]> = grouped
        .iter()
        .enumerate()
        .map(|(i, (profile, group))| {
            let is_current = current_norm
                .as_deref()
                .is_some_and(|c| profile.to_lowercase() == c);
            [
                Cell::plain(&(i + 1).to_string(), NUMBER),
                Cell::plain(profile, TEXT),
                Cell::marked(group, group_style(group)),
                if is_current {
                    Cell::marked("*", MARKER)
                } else {
                    Cell::plain("", TEXT)
                },
            ]
        })
        .collect();

    let mut widths = [0usize; 4];
    for (i, c) in COLUMNS.iter().enumerate() {
        widths[i] = width(c.header);
        for r in &rows {
            widths[i] = widths[i].max(r[i].width());
        }
    }
    let total: usize = widths.iter().map(|w| w + 2).sum::<usize>() + COLUMNS.len() + 1;

    // Title, centred over the table.
    let title = "AWS Profiles";
    let (l, r) = fill(width(title), total, Justify::Center);
    seg(
        &mut buf,
        &format!("{}{}{}", " ".repeat(l), title, " ".repeat(r)),
        TITLE,
        mode,
    );
    buf.push('\n');

    rule(
        &mut buf, &widths, '\u{250f}', '\u{2501}', '\u{2533}', '\u{2513}', mode,
    );
    // Header row.
    seg(&mut buf, "\u{2503}", BORDER, mode);
    for (i, c) in COLUMNS.iter().enumerate() {
        render_cell(
            &mut buf,
            &Cell::plain(c.header, HEADER),
            widths[i],
            c.justify,
            HEADER,
            mode,
        );
        seg(&mut buf, "\u{2503}", BORDER, mode);
    }
    buf.push('\n');
    rule(
        &mut buf, &widths, '\u{2521}', '\u{2501}', '\u{2547}', '\u{2529}', mode,
    );

    let mut prev_group: Option<&str> = None;
    for (row, (_, group)) in rows.iter().zip(grouped) {
        if let Some(p) = prev_group
            && p != group
        {
            rule(
                &mut buf, &widths, '\u{251c}', '\u{2500}', '\u{253c}', '\u{2524}', mode,
            );
        }
        prev_group = Some(group);
        seg(&mut buf, "\u{2502}", BORDER, mode);
        for (i, c) in COLUMNS.iter().enumerate() {
            render_cell(&mut buf, &row[i], widths[i], c.justify, c.base, mode);
            seg(&mut buf, "\u{2502}", BORDER, mode);
        }
        buf.push('\n');
    }
    rule(
        &mut buf, &widths, '\u{2514}', '\u{2500}', '\u{2534}', '\u{2518}', mode,
    );
    buf
}

pub fn display(grouped: &[(String, String)], current: Option<&str>) {
    out::err_raw(&render(grouped, current, Mode::detect()));
}
