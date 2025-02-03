#![no_std]
extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

enum Context {
    TopLevel,
    Function,
    Export,
    Param,
    Result,
    Local,
    ControlFlow,
}

pub fn format(input: &str) -> String {
    let mut output = String::new();
    let mut indent = 0;
    let mut context = Vec::new();
    let mut pending_space = false;
    let mut current_line_indent = 0;
    let mut in_string = false;
    let mut in_comment = false;

    context.push(Context::TopLevel);

    for c in input.chars() {
        match c {
            '(' => {
                if !in_comment && !in_string {
                    let keyword = get_keyword(input);
                    handle_opening(
                        &keyword,
                        &mut context,
                        &mut indent,
                        &mut current_line_indent,
                        &mut output,
                    );
                    pending_space = false;
                } else {
                    output.push(c);
                }
            }
            ')' => {
                if !in_comment && !in_string {
                    handle_closing(
                        &mut context,
                        &mut indent,
                        &mut current_line_indent,
                        &mut output,
                    );
                    pending_space = false;
                } else {
                    output.push(c);
                }
            }
            ';' if peek_next_char(input) == Some(';') => {
                in_comment = true;
                output.push_str(";;");
            }
            '"' if !in_comment => {
                in_string = !in_string;
                output.push(c);
            }
            '\n' => {
                in_comment = false;
                output.push(c);
                current_line_indent = 0;
                pending_space = false;
            }
            _ if in_comment || in_string => {
                output.push(c);
            }
            ' ' | '\t' => {
                if !pending_space {
                    pending_space = true;
                }
            }
            _ => {
                if current_line_indent < indent {
                    write!(&mut output, "\n{}", "  ".repeat(indent)).unwrap();
                    current_line_indent = indent;
                }
                if pending_space {
                    output.push(' ');
                    pending_space = false;
                }
                output.push(c);
            }
        }
    }

    output
}

fn handle_opening(
    keyword: &str,
    context: &mut Vec<Context>,
    indent: &mut usize,
    current_line_indent: &mut usize,
    output: &mut String,
) {
    match context.last() {
        Some(Context::TopLevel) => {
            context.push(Context::Function);
            *indent = 1;
            write!(output, "(").unwrap();
        }
        Some(Context::Function) => match keyword {
            "export" => {
                context.push(Context::Export);
                write!(output, " (export").unwrap();
            }
            "param" => {
                context.push(Context::Param);
                write!(output, " (param").unwrap();
            }
            "result" => {
                context.push(Context::Result);
                write!(output, " (result").unwrap();
            }
            "local" => {
                context.push(Context::Local);
                write!(output, "\n{}(", "  ".repeat(*indent)).unwrap();
                *indent += 1;
                *current_line_indent = *indent;
            }
            _ => {
                context.push(Context::ControlFlow);
                write!(output, "\n{}(", "  ".repeat(*indent)).unwrap();
                *indent += 1;
                *current_line_indent = *indent;
            }
        },
        Some(Context::ControlFlow) => {
            *indent += 1;
            write!(output, " (").unwrap();
        }
        _ => write!(output, " (").unwrap(),
    }
}

fn handle_closing(
    context: &mut Vec<Context>,
    indent: &mut usize,
    current_line_indent: &mut usize,
    output: &mut String,
) {
    if let Some(last_context) = context.pop() {
        match last_context {
            Context::Function => {
                *indent = 0;
                write!(output, ")").unwrap();
            }
            Context::Export | Context::Param | Context::Result => {
                write!(output, ")").unwrap();
            }
            Context::Local => {
                *indent -= 1;
                write!(output, ")").unwrap();
            }
            Context::ControlFlow => {
                *indent -= 1;
                write!(output, "\n{})", "  ".repeat(*indent)).unwrap();
                *current_line_indent = *indent;
            }
            _ => write!(output, ")").unwrap(),
        }
    }
}

fn get_keyword(input: &str) -> String {
    let mut keyword = String::new();
    let mut paren_depth = 0;

    for c in input.chars() {
        match c {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ' ' | '\t' | '\n' if paren_depth == 0 => break,
            _ if paren_depth == 0 => keyword.push(c),
            _ => {}
        }
    }

    keyword
}

fn peek_next_char(input: &str) -> Option<char> {
    input.chars().next()
}
