#![no_std]
extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

/// A simple token type.
enum Token {
    LParen,
    RParen,
    Atom(String),
}

/// Given an input string slice, break it into tokens.
/// String–literals (delimited by quotes) are kept as a single atom.
fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        } else if c == '(' {
            tokens.push(Token::LParen);
        } else if c == ')' {
            tokens.push(Token::RParen);
        } else if c == '"' {
            // Read a string literal including the quotes.
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
            // Read an atom until whitespace or a parenthesis is encountered.
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

/// An AST node representing either an atom or a list of nodes.
enum Node {
    Atom(String),
    List(Vec<Node>),
}

/// A recursive parser that builds a (possibly malformed) AST.
/// It is tolerant – if extra closing parentheses occur it simply produces an Atom.
fn parse_node(tokens: &[Token], mut i: usize) -> (Node, usize) {
    if i >= tokens.len() {
        return (Node::Atom(String::new()), i);
    }
    match &tokens[i] {
        Token::LParen => {
            i += 1;
            let mut children = Vec::new();
            while i < tokens.len() {
                match tokens[i] {
                    Token::RParen => {
                        i += 1; // consume the RParen
                        break;
                    }
                    _ => {
                        let (child, new_i) = parse_node(tokens, i);
                        children.push(child);
                        i = new_i;
                    }
                }
            }
            (Node::List(children), i)
        }
        Token::RParen => {
            // Stray closing parenthesis: output it as an atom.
            (Node::Atom(String::from(")")), i + 1)
        }
        Token::Atom(ref s) => (Node::Atom(s.clone()), i + 1),
    }
}

/// Parse all tokens into a vector of nodes.
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

/// Returns a string with `indent` levels (2 spaces per level).
fn indent_str(indent: usize) -> String {
    let mut s = String::new();
    for _ in 0..indent {
        s.push_str("  ");
    }
    s
}

/// Returns true if the given node is “flat” (an atom or a list whose children are all flat).
fn is_flat_node(node: &Node) -> bool {
    match node {
        Node::Atom(_) => true,
        Node::List(children) => children.iter().all(is_flat_node),
    }
}

/// Returns true if every node in the slice is flat.
fn is_flat_list(nodes: &[Node]) -> bool {
    nodes.iter().all(is_flat_node)
}

/// Format a node “inline” (without inserting any newlines).
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

/// If a node is flat then return its inline formatting.
#[allow(dead_code)]
fn format_inline(node: &Node) -> Option<String> {
    if is_flat_node(node) {
        Some(format_node_inline(node))
    } else {
        None
    }
}

/// Returns true if the first atom of a list node is one of these keywords.
/// Such nodes (like `(export ...)`, `(param ...)`, `(result ...)`) are inlined when part of a func signature.
fn is_inline_signature(node: &Node) -> bool {
    if let Node::List(children) = node {
        if let Some(Node::Atom(ref keyword)) = children.first() {
            return keyword == "export" || keyword == "param" || keyword == "result";
        }
    }
    false
}

/// A simple heuristic: returns true if the given token is an opcode.
/// In our case, an opcode is an atom that does not start with '$',
/// is not a numeric literal, and is not a string literal.
fn is_opcode(token: &str) -> bool {
    if token.starts_with('$') {
        return false;
    }
    if token.starts_with('"') {
        return false;
    }
    let mut chars = token.chars();
    if let Some(first) = chars.next() {
        // Check for a numeric literal: allow an optional sign then digits.
        if (first == '-' || first == '+') && chars.clone().all(|c| c.is_ascii_digit()) {
            return false;
        }
        if first.is_ascii_digit() && token.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    true
}

/// Helper that groups consecutive instruction tokens (atoms) in a list.
/// For each opcode atom, it appends any following atoms that are not opcodes.
fn format_instructions(nodes: &[Node], indent: usize) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < nodes.len() {
        match &nodes[i] {
            Node::Atom(token) => {
                if is_opcode(token) {
                    // Start a new instruction line.
                    let mut line = token.clone();
                    i += 1;
                    while i < nodes.len() {
                        if let Node::Atom(next_token) = &nodes[i] {
                            if is_opcode(next_token) {
                                break;
                            } else {
                                line.push(' ');
                                line.push_str(next_token);
                                i += 1;
                            }
                        } else {
                            break;
                        }
                    }
                    result.push('\n');
                    result.push_str(&indent_str(indent));
                    result.push_str(&line);
                } else {
                    // Non-opcode atom printed on its own line.
                    result.push('\n');
                    result.push_str(&indent_str(indent));
                    result.push_str(token);
                    i += 1;
                }
            }
            Node::List(_) => {
                // For a nested list, simply delegate to format_node.
                result.push('\n');
                result.push_str(&indent_str(indent));
                result.push_str(&format_node(&nodes[i], indent));
                i += 1;
            }
        }
    }
    result
}

/// Recursively format a node with the given indent level.
///
/// Special rules:
/// - A `(module …)` prints its children on new indented lines.
/// - A `(func …)` prints its signature groups inline and then uses `format_instructions`
///   for the remaining function body.
/// - A `(forall …)` is handled similarly, grouping its children (after the first "forall" atom)
///   as instructions.
fn format_node(node: &Node, indent: usize) -> String {
    match node {
        Node::Atom(s) => s.clone(),
        Node::List(children) => {
            if children.is_empty() {
                return String::from("()");
            }
            if let Some(Node::Atom(ref ident)) = children.first() {
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
                    // Always inline the first element ("func")
                    s.push_str(&format_node_inline(&children[0]));
                    let mut i = 1;
                    // Inline any signature tokens like (export ...), (param ...), (result ...).
                    while i < children.len() {
                        if is_inline_signature(&children[i]) {
                            s.push(' ');
                            s.push_str(&format_node_inline(&children[i]));
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    // Process the remaining children as the function body,
                    // grouping consecutive instruction tokens.
                    s.push_str(&format_instructions(&children[i..], indent + 1));
                    s.push('\n');
                    s.push_str(&indent_str(indent));
                    s.push(')');
                    return s;
                } else if ["forall", "exists", "assume", "unique"].contains(&ident.as_str()) {
                    // For a forall block, print the first atom inline then group its remaining children.
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
            // For any other list: if it is flat, print inline; otherwise, one element per line.
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

/// The public function to format unformatted WAT code.
///
/// This function receives unformatted WAT as an `&str` and returns a formatted `String`.
/// Even if the input is malformed, the formatter does its best.
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
