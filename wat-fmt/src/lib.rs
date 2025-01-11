#![no_std]

extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::iter::Peekable;
use core::slice::Iter;

/// Formats a WAT string with an indentation of 2 spaces.
pub fn format(input: &str) -> String {
    format_with_indent(input, 2)
}

/// Formats a WAT string using a configurable indentation level.
pub fn format_with_indent(input: &str, indent_size: usize) -> String {
    let tokens = tokenize(input);
    let mut tokens_iter = tokens.iter().peekable();
    format_module(&mut tokens_iter, 0, indent_size)
}

/// Splits the input into tokens (parentheses and sequences of non-whitespace, non-parenthesis chars).
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for c in input.chars() {
        match c {
            '(' | ')' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
                tokens.push(c.to_string());
            }
            ' ' | '\n' | '\r' | '\t' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }
    tokens
}

fn format_module(
    tokens_iter: &mut Peekable<Iter<String>>,
    current_indent: usize,
    indent_size: usize,
) -> String {
    let mut result = String::new();
    let mut start_of_line = true;
    while let Some(token) = tokens_iter.peek().cloned() {
        match token.as_str() {
            "(" => {
                tokens_iter.next(); // Consume '('
                                    // Peek next token to see what kind of form this might be
                if let Some(next_token) = tokens_iter.peek() {
                    match next_token.as_str() {
                        "module" => {
                            tokens_iter.next(); // consume "module"
                            if !start_of_line {
                                result.push('\n');
                            }
                            fill_indentation(&mut result, current_indent, indent_size);
                            result.push_str("(module\n");
                            result.push_str(&format_module_children(
                                tokens_iter,
                                current_indent + 1,
                                indent_size,
                            ));
                            start_of_line = true;
                        }
                        _ => {
                            // The module is probably malformed so we fallback to the default s-expressions formatter
                            todo!("Format the module as a sequence of s-expressions");
                        }
                    }
                }
            }
            ")" => {
                tokens_iter.next(); // Consume ')'
                result.push(')');
                if tokens_iter.peek().is_none() {
                    result.push('\n');
                } else {
                    panic!(
                        "Unexpected token after module closing parenthesis {:?}",
                        tokens_iter.peek()
                    );
                }
            }
            _ => {
                // The module is probably malformed so we fallback to the default s-expressions formatter
                todo!("Format the module as a sequence of s-expressions");
            }
        }
    }
    result
}

fn format_module_children(
    tokens_iter: &mut Peekable<Iter<String>>,
    current_indent: usize,
    indent_size: usize,
) -> String {
    let mut result = String::new();
    let mut start_of_line = true;
    while let Some(token) = tokens_iter.peek().cloned() {
        match token.as_str() {
            "(" => {
                tokens_iter.next(); // Consume '('
                if let Some(next_token) = tokens_iter.peek() {
                    match next_token.as_str() {
                        "func" => {
                            tokens_iter.next(); // consume "func"
                            if !start_of_line {
                                result.push('\n');
                            }
                            fill_indentation(&mut result, current_indent, indent_size);
                            result.push_str("(func");
                            result.push_str(&format_func_signature(
                                tokens_iter,
                                current_indent,
                                indent_size,
                            ));
                            result.push_str(&format_func_body(
                                tokens_iter,
                                current_indent,
                                indent_size,
                            ));
                            start_of_line = true;
                        }
                        _ => {
                            // The function is probably malformed so we fallback
                            // to the default s-expressions formatter
                            todo!("Format the module function as a sequence of s-expressions");
                        }
                    }
                }
            }
            ")" => {
                tokens_iter.next(); // Consume ')'
                result.push_str(")\n");
            }
            _ => {}
        }
    }
    result
}

fn format_func_signature(
    tokens_iter: &mut Peekable<Iter<String>>,
    current_indent: usize,
    indent_size: usize,
) -> String {
    let mut result = String::new();
    let mut start_of_line = true;
    while let Some(token) = tokens_iter.peek().cloned() {
        match token.as_str() {
            "(" => {
                let mut lookahead = tokens_iter.clone();
                lookahead.next(); // Consume '('
                if let Some(next_token) = lookahead.peek() {
                    match next_token.as_str() {
                        "export" | "param" | "result" => {
                            tokens_iter.next(); // consume sub_token
                            result.push(' ');
                            result.push('(');
                            result.push_str(next_token);
                            result.push_str(&format_inline_group(tokens_iter));
                        }
                        _ => {
                            // The function signature is probably malformed so we fallback
                            // to the default s-expressions formatter
                            todo!("Format the function signature as a sequence of s-expressions");
                        }
                    }
                }
            }
            ")" => {
                tokens_iter.next(); // Consume ')'
                result.push(')');
                return result;
            }
            _ => {
                todo!("Format the function signature as a sequence of s-expressions");
                // tokens_iter.next(); // consume it
                // if !start_of_line {
                //     result.push(' ');
                // } else {
                //     fill_indentation(&mut result, current_indent, indent_size);
                // }
                // result.push_str(token.as_str());
                // start_of_line = false;
            }
        }
    }
    result
}

fn format_func_body(
    tokens_iter: &mut Peekable<Iter<String>>,
    current_indent: usize,
    indent_size: usize,
) -> String {
    let mut result = String::new();
    let mut start_of_line = true;
    result
}

/// Top-level formatter that processes tokens, detecting `(func ...)`, `(module ...)`, etc.
fn format_tokens(
    tokens_iter: &mut Peekable<Iter<String>>,
    current_indent: usize,
    indent_size: usize,
) -> String {
    let mut result = String::new();
    let mut start_of_line = true;

    while let Some(token) = tokens_iter.peek().cloned() {
        match token.as_str() {
            "(" => {
                // Consume '('
                tokens_iter.next();
                // Peek next token to see what kind of form this might be
                if let Some(next_token) = tokens_iter.peek() {
                    match next_token.as_str() {
                        "func" => {
                            // We found (func ...
                            tokens_iter.next(); // consume "func"
                            if !start_of_line {
                                result.push('\n');
                            }
                            fill_indentation(&mut result, current_indent, indent_size);
                            // Write "(func"
                            result.push_str("(func");
                            // Now delegate to the dedicated func formatter
                            // which returns everything up to the matching ')'
                            // including sub-blocks inside the function.
                            result.push_str(&format_func(tokens_iter, current_indent, indent_size));
                            start_of_line = true;
                        }
                        "module" => {
                            // We found (module ...
                            tokens_iter.next(); // consume "module"
                            if !start_of_line {
                                result.push('\n');
                            }
                            fill_indentation(&mut result, current_indent, indent_size);
                            result.push_str("(module\n");
                            // Format everything until matching ')'
                            result.push_str(&format_tokens(
                                tokens_iter,
                                current_indent + 1,
                                indent_size,
                            ));
                            start_of_line = true;
                        }
                        _ => {
                            // Some other form, just handle generically as a block
                            if !start_of_line {
                                result.push('\n');
                            }
                            fill_indentation(&mut result, current_indent, indent_size);
                            result.push('(');
                            result.push_str(next_token);
                            tokens_iter.next(); // skip that token
                                                // format the sub-block
                            result.push_str(&format_block(
                                tokens_iter,
                                current_indent + 1,
                                indent_size,
                            ));
                            start_of_line = true;
                        }
                    }
                }
            }
            ")" => {
                // Usually handled inside sub-formatters, break at top-level
                break;
            }
            _ => {
                // Normal token at top-level
                tokens_iter.next();
                if !start_of_line {
                    result.push(' ');
                } else {
                    fill_indentation(&mut result, current_indent, indent_size);
                }
                result.push_str(token.as_str());
                start_of_line = false;
            }
        }
    }

    result
}

/// Dedicated formatter for `(func ...)`.
/// - `(export ...)`, `(param ...)`, `(result ...)` remain on the **same line** as `(func`.
/// - Everything else (e.g. `(local ...)`, single instructions like `i32.add`, or sub-blocks)
///   is on **new, indented lines**.
/// - Returns everything up to the matching `)`.
fn format_func(
    tokens_iter: &mut Peekable<Iter<String>>,
    current_indent: usize,
    indent_size: usize,
) -> String {
    let mut result = String::new();
    let mut signature_closed = false;

    // We'll keep reading tokens until we find the closing `)` that ends this function.
    while let Some(token) = tokens_iter.peek() {
        match token.as_str() {
            // End of the function
            ")" => {
                tokens_iter.next(); // consume ')'
                if !signature_closed {
                    // Close the function signature if not already closed
                    result.push(')');
                } else {
                    // We already placed a newline and block for the body,
                    // so let's close on a new line properly.
                    result.push('\n');
                    fill_indentation(&mut result, current_indent, indent_size);
                    result.push(')');
                }
                return result;
            }

            // Start of a new sub-expression
            "(" => {
                // Peek further to see if it's (export ...), (param ...), (result ...),
                // or something else (like (local ...), (loop ...), etc.)
                let mut lookahead = tokens_iter.clone();
                lookahead.next(); // skip "("
                if let Some(sub_token) = lookahead.peek() {
                    match sub_token.as_str() {
                        "export" | "param" | "result" => {
                            // This belongs to the function signature, on the same line
                            tokens_iter.next(); // consume "("
                            tokens_iter.next(); // consume sub_token
                            result.push(' ');
                            result.push('(');
                            result.push_str(sub_token);
                            // Keep everything inline until matching ')'
                            result.push_str(&format_inline_group(tokens_iter));
                        }
                        "local" => {
                            // This is part of the body
                            if !signature_closed {
                                // Close the signature now
                                // result.push(')');
                                signature_closed = true;
                            }
                            // Indent on a new line
                            result.push('\n');
                            fill_indentation(&mut result, current_indent + 1, indent_size);
                            // Print e.g. "(local ..."
                            tokens_iter.next(); // consume "("
                            tokens_iter.next(); // consume "local"
                            result.push_str("(local");
                            result.push_str(&format_inline_group(tokens_iter));
                        }
                        // // Possibly a control structure like (loop ...), (if ...), or something else
                        "loop" | "if" | "forall" | "exists" | "assume" | "unique" => {
                            signature_closed = true;
                            result.push('\n');
                            fill_indentation(&mut result, current_indent + 1, indent_size);
                            // Print e.g. "(loop" then parse sub-block
                            tokens_iter.next(); // consume "("
                            let block_type = tokens_iter.next().unwrap(); // e.g. "loop"
                            result.push('(');
                            result.push_str(block_type);
                            result.push_str(&format_block(
                                tokens_iter,
                                current_indent + 2,
                                indent_size,
                            ));
                        }
                        _ => {
                            signature_closed = true;
                            result.push('\n');
                            fill_indentation(&mut result, current_indent + 1, indent_size);
                        }
                    }
                } else {
                    // Malformed input
                    break;
                }
            }

            // Plain token (e.g. `i32.uzumaki`, `local.set`, `local.get`, `i32.add`, etc.)
            _ => {
                if !signature_closed {
                    // close the signature
                    result.push(')');
                    signature_closed = true;
                }
                // Now place these tokens on new lines
                result.push('\n');
                fill_indentation(&mut result, current_indent + 1, indent_size);
                result.push_str(token.as_str());
                tokens_iter.next(); // consume it
            }
        }
    }

    // If we exit the loop, we never found the closing `)`.
    // Naively just close the function. Real logic might error out.
    if !signature_closed {
        result.push(')');
    } else {
        result.push('\n');
        fill_indentation(&mut result, current_indent, indent_size);
        result.push(')');
    }
    result
}

/// Reads a sub-block `( ... )` until its matching `)`, recursively formatting contents.
fn format_block(
    tokens_iter: &mut Peekable<Iter<String>>,
    current_indent: usize,
    indent_size: usize,
) -> String {
    let mut result = String::new();
    let mut depth = 1;
    let mut start_of_line = false;

    while let Some(token) = tokens_iter.next() {
        match token.as_str() {
            "(" => {
                depth += 1;
                if let Some(sub_token) = tokens_iter.peek() {
                    match sub_token.as_str() {
                        "loop" | "if" | "forall" => {
                            result.push('\n');
                            fill_indentation(&mut result, current_indent, indent_size);
                            result.push('(');
                            result.push_str(sub_token);
                            tokens_iter.next();
                            result.push_str(&format_block(
                                tokens_iter,
                                current_indent + 1,
                                indent_size,
                            ));
                        }
                        _ => {
                            result.push('\n');
                            fill_indentation(&mut result, current_indent, indent_size);
                            result.push('(');
                            result.push_str(&format_block(
                                tokens_iter,
                                current_indent + 1,
                                indent_size,
                            ));
                        }
                    }
                }
            }
            ")" => {
                depth -= 1;
                if depth == 0 {
                    result.push(')');
                    return result;
                } else {
                    result.push(')');
                }
            }
            _ => {
                if start_of_line {
                    result.push('\n');
                    fill_indentation(&mut result, current_indent, indent_size);
                } else if !result.ends_with('(') && !result.ends_with('\n') {
                    result.push(' ');
                }
                result.push_str(token);
                start_of_line = false;
            }
        }
    }

    result
}

/// Reads a group `( ... )` inline (without newlines), until the matching `)`.
/// Used for `(export ...)`, `(param ...)`, `(result ...)`, `(local ...)` in the header/body.
fn format_inline_group(tokens_iter: &mut Peekable<Iter<String>>) -> String {
    let mut out = String::new();
    let mut depth = 1;

    for token in tokens_iter {
        match token.as_str() {
            "(" => {
                depth += 1;
                out.push('(');
            }
            ")" => {
                depth -= 1;
                out.push(')');
                if depth == 0 {
                    break;
                }
            }
            _ => {
                out.push(' ');
                out.push_str(token);
            }
        }
    }

    out
}

fn fill_indentation(result: &mut String, current_indent: usize, indent_size: usize) {
    for _ in 0..(current_indent * indent_size) {
        result.push(' ');
    }
}
