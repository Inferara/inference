#![no_std]

extern crate alloc;

use alloc::{
    borrow::ToOwned,
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
    while let Some(token) = tokens_iter.peek().cloned() {
        match token.as_str() {
            "(" => {
                tokens_iter.next(); // Consume '('
                if let Some(next_token) = tokens_iter.peek() {
                    match next_token.as_str() {
                        "module" => {
                            tokens_iter.next(); // consume "module"
                            result.push_str("(module\n");
                            result.push_str(&format_module_children(
                                tokens_iter,
                                current_indent + 1,
                                indent_size,
                            ));
                        }
                        _ => {
                            // The module is probably malformed so we fallback to the default s-expressions formatter
                            todo!("Format the module as a sequence of s-expressions");
                        }
                    }
                }
            }
            ")" => {
                result.push('\n');
                tokens_iter.next(); // Consume ')'
                result.push(')');
                if tokens_iter.peek().is_some() {
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
    let mut depth = 0;
    while let Some(token) = tokens_iter.peek().cloned() {
        match token.as_str() {
            "(" => {
                depth += 1;
                tokens_iter.next(); // Consume '('
                if let Some(next_token) = tokens_iter.peek() {
                    match next_token.as_str() {
                        "func" => {
                            tokens_iter.next(); // consume "func"
                            result.push('\n');
                            fill_indentation(&mut result, current_indent, indent_size);
                            result.push_str("(func");
                            result.push_str(&format_func_signature(tokens_iter));
                            result.push_str(&format_block(
                                tokens_iter,
                                current_indent,
                                indent_size,
                            ));
                            // result.push_str(tokens_iter.next().unwrap()); // Consume ')'
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
                depth -= 1;
                if depth < 0 {
                    break;
                }
                tokens_iter.next(); // Consume ')'
                fill_indentation(&mut result, current_indent, indent_size);
                result.push_str(")\n");
            }
            _ => {}
        }
    }
    result
}

fn format_func_signature(tokens_iter: &mut Peekable<Iter<String>>) -> String {
    let mut result = String::new();
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
                            result.push_str(&format_inline_group(tokens_iter));
                        }
                        _ => {
                            break;
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
                break;
                // todo!("Format the function signature as a sequence of s-expressions");
            }
        }
    }
    result
}

fn format_block(
    tokens_iter: &mut Peekable<Iter<String>>,
    current_indent: usize,
    indent_size: usize,
) -> String {
    let mut result = String::new();
    while let Some(token) = tokens_iter.peek().cloned() {
        result.push('\n');
        match token.as_str() {
            "(" => {
                let mut lookahead = tokens_iter.clone();
                lookahead.next(); // Consume '('
                if let Some(sub_token) = lookahead.peek() {
                    match sub_token.as_str() {
                        "loop" | "if" | "forall" | "exists" | "assume" | "unique" => {
                            fill_indentation(&mut result, current_indent + 1, indent_size);
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
                        // instructions with > 1 operands
                        "local" => {
                            fill_indentation(&mut result, current_indent + 1, indent_size);
                            result.push('(');
                            tokens_iter.next(); // consume "("
                            result.push_str(&format_inline_group(tokens_iter));
                        }
                        _ => break,
                    }
                }
            }
            ")" => {
                break;
            }
            "i32.uzumaki" | "i64.uzumaki" | "i32.add" | "i64.add" => {
                format_single_token(
                    &mut result,
                    tokens_iter.next().unwrap().to_owned(),
                    current_indent + 1,
                    indent_size,
                );
            }
            "local.set" | "local.get" | "i32.const" | "i64.const" => {
                format_instruction_with_operands(
                    &mut result,
                    tokens_iter,
                    current_indent,
                    indent_size,
                    1,
                );
            }
            _ => {
                todo!("Unexpected token in function body: {:?}", token);
            }
        }
    }
    result
}

fn format_instruction_with_operands(
    result: &mut String,
    tokens_iter: &mut Peekable<Iter<String>>,
    current_indent: usize,
    indent_size: usize,
    operands: usize,
) {
    fill_indentation(result, current_indent + 1, indent_size);
    result.push_str(tokens_iter.next().unwrap());
    for _ in 0..operands {
        result.push(' ');
        format_single_token(result, tokens_iter.next().unwrap().to_owned(), 0, 0);
    }
}

fn format_single_token(
    result: &mut String,
    token: String,
    current_indent: usize,
    indent_size: usize,
) {
    fill_indentation(result, current_indent, indent_size);
    result.push_str(&token);
}

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
                    out.pop(); // remove the last ')'
                    out.pop(); // remove the last ' '
                    out.push(')');
                    break;
                }
            }
            _ => {
                out.push_str(token);
                out.push(' ');
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
