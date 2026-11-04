//! Reading and writing GEDCOM text.

use crate::node::Node;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Line number (from 1).
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ligne {} : {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Formatting details of the source file, restored on writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextFormat {
    pub bom: bool,
    pub crlf: bool,
    pub final_newline: bool,
}

impl Default for TextFormat {
    fn default() -> Self {
        // GEDCOM 7: UTF-8, optional BOM; follow Ancestris (BOM + LF).
        Self { bom: true, crlf: false, final_newline: true }
    }
}

/// Parses a GEDCOM text into a list of level 0 structures.
pub fn parse_nodes(input: &str) -> Result<(Vec<Node>, TextFormat), ParseError> {
    let bom = input.starts_with('\u{feff}');
    let body = input.strip_prefix('\u{feff}').unwrap_or(input);
    let crlf = body.contains("\r\n");
    let final_newline = body.ends_with('\n') || body.ends_with('\r');

    let mut roots: Vec<Node> = Vec::new();
    // Stack of open nodes: stack[i] is at level i.
    let mut stack: Vec<Node> = Vec::new();

    for (idx, raw) in body.split('\n').enumerate() {
        let line_no = idx + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim().is_empty() {
            continue;
        }
        let line = line.trim_start();
        let (level, node) = parse_line(line).map_err(|message| ParseError { line: line_no, message })?;

        if level > stack.len() {
            return Err(ParseError {
                line: line_no,
                message: t!("error.parse.unexpected_level", level = level, max = stack.len()).into(),
            });
        }
        while stack.len() > level {
            let done = stack.pop().unwrap();
            attach(&mut stack, &mut roots, done);
        }
        stack.push(node);
    }
    while let Some(done) = stack.pop() {
        attach(&mut stack, &mut roots, done);
    }
    Ok((roots, TextFormat { bom, crlf, final_newline }))
}

fn attach(stack: &mut [Node], roots: &mut Vec<Node>, node: Node) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None => roots.push(node),
    }
}

fn parse_line(line: &str) -> Result<(usize, Node), String> {
    let (level_s, rest) =
        line.split_once(' ').ok_or_else(|| t!("error.parse.incomplete_line", line = line).into_owned())?;
    let level: usize = level_s.parse().map_err(|_| t!("error.parse.invalid_level", level = level_s).into_owned())?;
    if level_s.len() > 1 && level_s.starts_with('0') {
        return Err(t!("error.parse.invalid_level", level = level_s).into());
    }

    let mut rest = rest;
    let mut xref = None;
    if rest.starts_with('@') {
        let (x, r) = rest.split_once(' ').ok_or_else(|| t!("error.parse.xref_without_tag"))?;
        if x.len() < 3 || !x.ends_with('@') {
            return Err(t!("error.parse.invalid_xref", xref = x).into());
        }
        xref = Some(x[1..x.len() - 1].to_string());
        rest = r;
    }
    let (tag, value) = match rest.split_once(' ') {
        Some((t, v)) => (t, Some(v.to_string())),
        None => (rest, None),
    };
    if tag.is_empty() || !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(t!("error.parse.invalid_tag", tag = tag).into());
    }
    Ok((level, Node { xref, tag: tag.to_string(), value, children: Vec::new() }))
}

/// Serializes structures (from level `base_level`).
pub fn write_nodes<'a>(nodes: impl IntoIterator<Item = &'a Node>, base_level: usize, newline: &str, out: &mut String) {
    for n in nodes {
        write_node(n, base_level, newline, out);
    }
}

fn write_node(n: &Node, level: usize, newline: &str, out: &mut String) {
    use std::fmt::Write;
    let _ = write!(out, "{level} ");
    if let Some(x) = &n.xref {
        let _ = write!(out, "@{x}@ ");
    }
    out.push_str(&n.tag);
    if let Some(v) = &n.value {
        out.push(' ');
        out.push_str(v);
    }
    out.push_str(newline);
    for c in &n.children {
        write_node(c, level + 1, newline, out);
    }
}

/// GEDCOM text of a single record (for the raw editor).
pub fn node_to_string(n: &Node) -> String {
    let mut s = String::new();
    write_node(n, 0, "\n", &mut s);
    s
}

/// Reads back the text of a record typed in the raw editor.
pub fn node_from_string(text: &str) -> Result<Node, ParseError> {
    let (mut nodes, _) = parse_nodes(text)?;
    match nodes.len() {
        1 => Ok(nodes.remove(0)),
        0 => Err(ParseError { line: 1, message: t!("error.parse.no_record").into() }),
        _ => Err(ParseError { line: 1, message: t!("error.parse.one_record").into() }),
    }
}
