#![no_std]
#![warn(clippy::pedantic)]
extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

enum Token {
    LParen,
    RParen,
    Atom(String),
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c == '(' {
            tokens.push(Token::LParen);
        } else if c == ')' {
            tokens.push(Token::RParen);
        } else if c == '"' {
            let mut s = String::new();
            s.push('"');
            while let Some(&next) = chars.peek() {
                s.push(next);
                chars.next();
                if next == '"' {
                    break;
                }
            }
            tokens.push(Token::Atom(s));
        } else {
            let mut s = String::new();
            s.push(c);
            while let Some(&next) = chars.peek() {
                if next.is_whitespace() || next == '(' || next == ')' {
                    break;
                }
                s.push(next);
                chars.next();
            }
            tokens.push(Token::Atom(s));
        }
    }

    tokens
}

enum Node {
    Atom(String),
    List(Vec<Node>),
}

fn parse_node(tokens: &[Token], mut i: usize) -> (Node, usize) {
    if i >= tokens.len() {
        return (Node::Atom(String::new()), i);
    }
    match &tokens[i] {
        Token::LParen => {
            i += 1;
            let mut children = Vec::new();
            while i < tokens.len() {
                if let Token::RParen = tokens[i] {
                    i += 1; // consume the RParen
                    break;
                }
                let (child, new_i) = parse_node(tokens, i);
                children.push(child);
                i = new_i;
            }
            (Node::List(children), i)
        }
        Token::RParen => (Node::Atom(String::from(")")), i + 1),
        Token::Atom(s) => (Node::Atom(s.clone()), i + 1),
    }
}

fn parse_all(tokens: &[Token]) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let (node, new_i) = parse_node(tokens, i);
        nodes.push(node);
        i = new_i;
    }
    nodes
}

fn indent_str(indent: usize) -> String {
    let mut s = String::new();
    for _ in 0..indent {
        s.push_str("  ");
    }
    s
}

/// Returns true if the node and all its children can be printed inline.
fn is_flat_node(node: &Node) -> bool {
    match node {
        Node::Atom(_) => true,
        Node::List(children) => children.iter().all(is_flat_node),
    }
}

fn is_flat_list(nodes: &[Node]) -> bool {
    nodes.iter().all(is_flat_node)
}

/// Print node inline without extra formatting.
fn format_node_inline(node: &Node) -> String {
    match node {
        Node::Atom(s) => s.clone(),
        Node::List(children) => {
            let mut s = String::new();
            s.push('(');
            let mut first = true;
            for child in children {
                if !first {
                    s.push(' ');
                }
                s.push_str(&format_node_inline(child));
                first = false;
            }
            s.push(')');
            s
        }
    }
}

/// Returns Some(inline) if the node is “flat” (only contains inline data).
#[allow(dead_code)]
fn format_inline(node: &Node) -> Option<String> {
    if is_flat_node(node) {
        Some(format_node_inline(node))
    } else {
        None
    }
}

/// Check for inline signature markers.
fn is_inline_signature(node: &Node) -> bool {
    if let Node::List(children) = node
        && let Some(Node::Atom(keyword)) = children.first()
    {
        return keyword == "export" || keyword == "param" || keyword == "result";
    }
    false
}

/// Check whether a token looks like an opcode rather than a parameter or literal.
fn is_opcode(token: &str) -> bool {
    if token.starts_with('$') || token.starts_with('"') {
        return false;
    }
    let mut chars = token.chars();
    if let Some(first) = chars.next() {
        if (first == '-' || first == '+') && chars.clone().all(|c| c.is_ascii_digit()) {
            return false;
        }
        if first.is_ascii_digit() && token.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    true
}

/// Format the instructions in a more readable way.
fn format_instructions(nodes: &[Node], base_indent: usize) -> String {
    let mut result = String::new();
    let mut current_indent = base_indent;
    let mut i = 0;
    while i < nodes.len() {
        match &nodes[i] {
            Node::Atom(token) => {
                if token == "if" {
                    result.push('\n');
                    result.push_str(&indent_str(current_indent));
                    result.push_str("if");
                    current_indent += 1;
                    i += 1;
                } else if token == "else" {
                    // Outdent to match the "if"
                    current_indent -= 1;
                    result.push('\n');
                    result.push_str(&indent_str(current_indent));
                    result.push_str("else");
                    // indent the else body
                    current_indent += 1;
                    i += 1;
                } else if token == "end" {
                    current_indent = current_indent.saturating_sub(1);
                    result.push('\n');
                    result.push_str(&indent_str(current_indent));
                    result.push_str("end");
                    i += 1;
                } else if is_opcode(token) {
                    // Start a new instruction line: group arguments (non-opcodes) with this opcode.
                    let mut line = token.clone();
                    i += 1;
                    while i < nodes.len() {
                        if let Node::Atom(next_token) = &nodes[i] {
                            if is_opcode(next_token)
                                || next_token == "if"
                                || next_token == "else"
                                || next_token == "end"
                            {
                                break;
                            }
                            line.push(' ');
                            line.push_str(next_token);
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    result.push('\n');
                    result.push_str(&indent_str(current_indent));
                    result.push_str(&line);
                } else {
                    // For non-opcode atoms, print them on their own line.
                    result.push('\n');
                    result.push_str(&indent_str(current_indent));
                    result.push_str(token);
                    i += 1;
                }
            }
            Node::List(_) => {
                result.push('\n');
                result.push_str(&indent_str(current_indent));
                result.push_str(&format_node(&nodes[i], current_indent));
                i += 1;
            }
        }
    }
    result
}

/// Format a node with indentation.
fn format_node(node: &Node, indent: usize) -> String {
    match node {
        Node::Atom(s) => s.clone(),
        Node::List(children) => {
            if children.is_empty() {
                return String::from("()");
            }
            // Special handling for “module”:
            if let Some(Node::Atom(ident)) = children.first() {
                if ident == "module" {
                    let mut s = String::new();
                    s.push('(');
                    s.push_str(ident);
                    for child in children.iter().skip(1) {
                        s.push('\n');
                        s.push_str(&indent_str(indent + 1));
                        s.push_str(&format_node(child, indent + 1));
                    }
                    s.push('\n');
                    s.push_str(&indent_str(indent));
                    s.push(')');
                    return s;
                } else if ident == "func" {
                    let mut s = String::new();
                    s.push('(');
                    // Always print the “func” keyword inline.
                    s.push_str(&format_node_inline(&children[0]));
                    let mut i = 1;
                    // Inline printing for function name and inline signatures.
                    while i < children.len() {
                        // If this is an atom and it looks like an opcode (i.e. an instruction),
                        // then stop printing inline.
                        if let Node::Atom(ref tok) = children[i]
                            && is_opcode(tok)
                        {
                            break;
                        }
                        if let Node::List(_) = children[i]
                            && !is_inline_signature(&children[i])
                        {
                            break;
                        }
                        s.push(' ');
                        s.push_str(&format_node_inline(&children[i]));
                        i += 1;
                    }
                    // Format the remaining nodes as instructions.
                    s.push_str(&format_instructions(&children[i..], indent + 1));
                    s.push('\n');
                    s.push_str(&indent_str(indent));
                    s.push(')');
                    return s;
                } else if ["forall", "exists", "assume", "unique"].contains(&ident.as_str()) {
                    let mut s = String::new();
                    s.push('(');
                    s.push_str(ident);
                    s.push_str(&format_instructions(&children[1..], indent + 1));
                    s.push('\n');
                    s.push_str(&indent_str(indent));
                    s.push(')');
                    return s;
                }
            }
            // For lists that are flat, use the inline formatter.
            if is_flat_list(children) {
                format_node_inline(node)
            } else {
                let mut s = String::new();
                s.push('(');
                let mut first = true;
                for child in children {
                    if first {
                        s.push_str(&format_node(child, indent + 1));
                        first = false;
                    } else {
                        s.push('\n');
                        s.push_str(&indent_str(indent + 1));
                        s.push_str(&format_node(child, indent + 1));
                    }
                }
                s.push('\n');
                s.push_str(&indent_str(indent));
                s.push(')');
                s
            }
        }
    }
}

/// Format the input `WAT` string into a readable format.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[must_use]
pub fn format(input: &str) -> String {
    let tokens = tokenize(input);
    let nodes = parse_all(&tokens);
    if nodes.len() == 1 {
        format_node(&nodes[0], 0)
    } else {
        let mut s = String::new();
        for node in nodes {
            s.push_str(&format_node(&node, 0));
            s.push('\n');
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format() {
        let input = r#"(module (func $add (param $a i32) (param $b i32) (result i32) (local $c i32) i32.uzumaki local.set $c local.get $a local.get $c i32.add) (export "add" (func $add) ) )"#;
        let expected = r#"(module
  (func $add (param $a i32) (param $b i32) (result i32)
    (local $c i32)
    i32.uzumaki
    local.set $c
    local.get $a
    local.get $c
    i32.add
  )
  (export "add" (func $add))
)"#;
        let output = format(input);
        assert_eq!(output, expected);
    }
}
