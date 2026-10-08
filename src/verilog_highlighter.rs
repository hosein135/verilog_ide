//! Verilog / SystemVerilog syntax highlighter for the iced text editor.

use iced::advanced::text::highlighter::{self, Format};
use iced::{Color, Font};
use std::ops::Range;
use std::path::Path;

const KEYWORDS: &[&str] = &[
    "alias", "always", "always_comb", "always_ff", "always_latch", "and", "assign", "automatic",
    "begin", "bind", "buf", "bufif0", "bufif1", "case", "casex", "casez", "cell", "cmos",
    "deassign", "default", "defparam", "disable", "edge", "else", "end", "endcase",
    "endfunction", "endgenerate", "endmodule", "endprimitive", "endspecify", "endtable",
    "endtask", "event", "for", "force", "forever", "fork", "function", "generate", "genvar",
    "highz0", "highz1", "if", "ifnone", "initial", "inout", "input", "integer", "join",
    "join_any", "join_none", "large", "localparam", "macromodule", "medium", "module", "nand",
    "negedge", "nmos", "nor", "not", "notif0", "notif1", "or", "output", "parameter", "pmos",
    "posedge", "primitive", "pull0", "pull1", "pulldown", "pullup", "rcmos", "real", "realtime",
    "reg", "release", "repeat", "return", "rnmos", "rpmos", "rtran", "rtranif0", "rtranif1",
    "scalared", "signed", "small", "specify", "specparam", "strong0", "strong1", "supply0",
    "supply1", "table", "task", "time", "tran", "tranif0", "tranif1", "tri", "tri0", "tri1",
    "triand", "trior", "trireg", "unsigned", "until", "until_with", "use", "vectored", "wait",
    "wand", "weak0", "weak1", "while", "wire", "with", "wor", "xnor", "xor",
    // Common SystemVerilog
    "logic", "bit", "byte", "int", "longint", "shortint", "typedef", "struct", "enum",
    "interface", "endinterface", "class", "endclass", "package", "endpackage", "import",
    "export", "virtual", "static", "const", "unique", "priority", "assert", "property",
];

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true",
    "type", "unsafe", "use", "where", "while",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Comment,
    Directive,
    String,
    Number,
    Keyword,
    SystemTask,
    Operator,
}

#[derive(Debug, Clone, Copy)]
pub struct Highlight(TokenKind);

impl Highlight {
    pub fn to_format(&self) -> Format<Font> {
        Format {
            color: Some(self.color()),
            font: None,
        }
    }

    fn color(self) -> Color {
        match self.0 {
            TokenKind::Comment => Color::from_rgb(0.45, 0.58, 0.56),
            TokenKind::Directive => Color::from_rgb(0.45, 0.82, 0.86),
            TokenKind::String => Color::from_rgb(0.93, 0.74, 0.46),
            TokenKind::Number => Color::from_rgb(0.70, 0.88, 0.62),
            TokenKind::Keyword => Color::from_rgb(0.95, 0.62, 0.42),
            TokenKind::SystemTask => Color::from_rgb(0.62, 0.78, 0.98),
            TokenKind::Operator => Color::from_rgb(0.72, 0.80, 0.88),
        }
    }
}

pub fn format_highlight(highlight: &Highlight, _theme: &iced::Theme) -> Format<Font> {
    highlight.to_format()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ParseState {
    block_comment: bool,
    string: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub enabled: bool,
    pub rust: bool,
}

#[derive(Debug)]
pub struct VerilogHighlighter {
    settings: Settings,
    current_line: usize,
    snapshots: Vec<ParseState>,
}

impl highlighter::Highlighter for VerilogHighlighter {
    type Settings = Settings;
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Self::Highlight)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            settings: settings.clone(),
            current_line: 0,
            snapshots: vec![ParseState::default()],
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.settings = new_settings.clone();
        self.change_line(0);
    }

    fn change_line(&mut self, line: usize) {
        self.snapshots.truncate(line + 1);
        if self.snapshots.is_empty() {
            self.snapshots.push(ParseState::default());
        }
        self.current_line = line;
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        if !self.settings.enabled {
            self.current_line += 1;
            return Vec::new().into_iter();
        }

        let mut state = *self
            .snapshots
            .last()
            .unwrap_or(&ParseState::default());

        let spans = highlight_line(line, &mut state, self.settings.rust);

        self.snapshots.push(state);
        self.current_line += 1;

        spans.into_iter().collect::<Vec<_>>().into_iter()
    }

    fn current_line(&self) -> usize {
        self.current_line
    }
}

fn highlight_line(line: &str, state: &mut ParseState, rust: bool) -> Vec<(Range<usize>, Highlight)> {
    let mut spans = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if state.block_comment {
            if let Some(rel) = line[i..].find("*/") {
                let end = i + rel + 2;
                push_span(&mut spans, i, end, TokenKind::Comment);
                i = end;
                state.block_comment = false;
            } else {
                push_span(&mut spans, i, bytes.len(), TokenKind::Comment);
                break;
            }
            continue;
        }

        if state.string {
            if let Some(rel) = line[i..].find('"') {
                let end = i + rel + 1;
                push_span(&mut spans, i, end, TokenKind::String);
                i = end;
                state.string = false;
            } else {
                push_span(&mut spans, i, bytes.len(), TokenKind::String);
                break;
            }
            continue;
        }

        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            push_span(&mut spans, i, bytes.len(), TokenKind::Comment);
            break;
        }

        if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            if let Some(rel) = line[i + 2..].find("*/") {
                let end = i + 2 + rel + 2;
                push_span(&mut spans, i, end, TokenKind::Comment);
                i = end;
            } else {
                push_span(&mut spans, i, bytes.len(), TokenKind::Comment);
                state.block_comment = true;
                break;
            }
            continue;
        }

        if b == b'"' {
            state.string = true;
            i += 1;
            continue;
        }

        if b == b'`' && !rust {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            push_span(&mut spans, start, i, TokenKind::Directive);
            continue;
        }

        if b == b'$' && !rust {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            push_span(&mut spans, start, i, TokenKind::SystemTask);
            continue;
        }

        if !rust && b == b'\'' && bytes.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
            let start = i;
            i += 2;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'?')
            {
                i += 1;
            }
            push_span(&mut spans, start, i, TokenKind::Number);
            continue;
        }

        if b.is_ascii_digit()
            || (b == b'.' && bytes.get(i + 1).is_some_and(|c| c.is_ascii_digit()))
        {
            let start = i;
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            push_span(&mut spans, start, i, TokenKind::Number);
            continue;
        }

        if !rust && b == b'\'' && bytes.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
            let start = i;
            i += 2;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'?')
            {
                i += 1;
            }
            push_span(&mut spans, start, i, TokenKind::Number);
            continue;
        }

        if rust && b == b'#' {
            let start = i;
            i += 1;
            if bytes.get(i) == Some(&b'[') {
                i += 1;
            }
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b':')
            {
                i += 1;
            }
            push_span(&mut spans, start, i, TokenKind::Directive);
            continue;
        }

        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &line[start..i];
            let kind = if is_keyword(word, rust) {
                TokenKind::Keyword
            } else {
                i = start + word.len();
                continue;
            };
            push_span(&mut spans, start, i, kind);
            continue;
        }

        if is_operator_byte(b) {
            push_span(&mut spans, i, i + 1, TokenKind::Operator);
            i += 1;
            continue;
        }

        i += 1;
    }

    merge_spans(spans)
}

fn push_span(spans: &mut Vec<(Range<usize>, Highlight)>, start: usize, end: usize, kind: TokenKind) {
    if start < end {
        spans.push((start..end, Highlight(kind)));
    }
}

fn merge_spans(spans: Vec<(Range<usize>, Highlight)>) -> Vec<(Range<usize>, Highlight)> {
    let mut merged: Vec<(Range<usize>, Highlight)> = Vec::new();
    for (range, highlight) in spans {
        if let Some((last_range, last_highlight)) = merged.last_mut() {
            if last_highlight.0 == highlight.0 && last_range.end == range.start {
                last_range.end = range.end;
                continue;
            }
        }
        merged.push((range, highlight));
    }
    merged
}

fn is_keyword(word: &str, rust: bool) -> bool {
    if rust {
        RUST_KEYWORDS.contains(&word)
    } else {
        KEYWORDS.contains(&word)
    }
}

fn is_operator_byte(b: u8) -> bool {
    matches!(
        b,
        b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'<' | b'>' | b'!' | b'&' | b'|' | b'^'
            | b'~' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b';' | b':' | b',' | b'.' | b'?'
    )
}

pub fn syntax_enabled_for_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("v" | "sv" | "vh" | "svh" | "vl")
    )
}
