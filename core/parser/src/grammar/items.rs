/// Item parsing - Top-level declarations

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse function definition
pub fn parse_function(p: &mut Parser) {
    p.expect(SyntaxKind::FN); // consume 'fn'
    p.bump(); // skip function name
    
    parse_generic_params(p);
    parse_param_list(p);
    
    if p.at(SyntaxKind::ARROW) {
        p.bump();
        super::types::parse_type(p);
    }
    
    parse_function_body(p);
}

/// Parse function parameter list
fn parse_param_list(p: &mut Parser) {
    if !p.at(SyntaxKind::L_PAREN) {
        return;
    }
    p.bump(); // consume '('
    
    while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
        if p.at(SyntaxKind::MUT) {
            p.bump();
        }
        if p.at(SyntaxKind::REF) {
            p.bump();
        }
        
        p.bump(); // parameter name
        
        if p.at(SyntaxKind::COLON) {
            p.bump();
            super::types::parse_type(p);
        }
        
        if p.at(SyntaxKind::COMMA) {
            p.bump();
        }
    }
    
    if p.at(SyntaxKind::R_PAREN) {
        p.bump();
    }
}

/// Parse function body
fn parse_function_body(p: &mut Parser) {
    if p.at(SyntaxKind::L_BRACE) {
        parse_block(p);
    }
}

/// Parse a block of statements
pub fn parse_block(p: &mut Parser) {
    if !p.at(SyntaxKind::L_BRACE) {
        return;
    }
    p.bump(); // consume '{'
    
    let mut depth = 1;
    while depth > 0 && !p.at_eof() {
        if p.at(SyntaxKind::L_BRACE) {
            depth += 1;
        } else if p.at(SyntaxKind::R_BRACE) {
            depth -= 1;
            if depth == 0 {
                p.bump();
                break;
            }
        }
        p.bump();
    }
}

/// Parse struct definition
pub fn parse_struct(p: &mut Parser) {
    p.expect(SyntaxKind::STRUCT);
    p.bump(); // struct name
    
    parse_generic_params(p);
    
    if p.at(SyntaxKind::L_BRACE) {
        parse_struct_fields(p);
    }
}

/// Parse struct fields
fn parse_struct_fields(p: &mut Parser) {
    p.bump(); // consume '{'
    
    while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
        p.bump(); // field name
        
        if p.at(SyntaxKind::COLON) {
            p.bump();
            super::types::parse_type(p);
        }
        
        if p.at(SyntaxKind::COMMA) {
            p.bump();
        }
    }
    
    if p.at(SyntaxKind::R_BRACE) {
        p.bump();
    }
}

/// Parse enum definition
pub fn parse_enum(p: &mut Parser) {
    p.expect(SyntaxKind::ENUM);
    p.bump(); // enum name
    
    parse_generic_params(p);
    
    if p.at(SyntaxKind::L_BRACE) {
        p.bump(); // consume '{'
        
        while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
            p.bump(); // variant name
            
            if p.at(SyntaxKind::L_PAREN) {
                p.bump();
                while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
                    super::types::parse_type(p);
                    if p.at(SyntaxKind::COMMA) {
                        p.bump();
                    }
                }
                if p.at(SyntaxKind::R_PAREN) {
                    p.bump();
                }
            }
            
            if p.at(SyntaxKind::COMMA) {
                p.bump();
            }
        }
        
        if p.at(SyntaxKind::R_BRACE) {
            p.bump();
        }
    }
}

/// Parse trait definition
pub fn parse_trait(p: &mut Parser) {
    p.expect(SyntaxKind::TRAIT);
    p.bump(); // trait name
    
    parse_generic_params(p);
    
    if p.at(SyntaxKind::L_BRACE) {
        p.bump();
        
        while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
            if p.at(SyntaxKind::FN) {
                parse_function(p);
            } else if p.at(SyntaxKind::TYPE) {
                parse_type_alias(p);
            } else if p.at(SyntaxKind::CONST) {
                parse_const(p);
            } else {
                p.bump();
            }
        }
        
        if p.at(SyntaxKind::R_BRACE) {
            p.bump();
        }
    }
}

/// Parse impl block
pub fn parse_impl(p: &mut Parser) {
    p.expect(SyntaxKind::IMPL);
    
    parse_generic_params(p);
    
    super::types::parse_type(p);
    
    if p.at(SyntaxKind::FOR) {
        p.bump();
        super::types::parse_type(p);
    }
    
    if p.at(SyntaxKind::L_BRACE) {
        p.bump();
        
        while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
            if p.at(SyntaxKind::FN) {
                parse_function(p);
            } else if p.at(SyntaxKind::CONST) {
                parse_const(p);
            } else {
                p.bump();
            }
        }
        
        if p.at(SyntaxKind::R_BRACE) {
            p.bump();
        }
    }
}

/// Parse type alias
pub fn parse_type_alias(p: &mut Parser) {
    p.expect(SyntaxKind::TYPE);
    p.bump(); // type name
    
    if p.at(SyntaxKind::ASSIGN) {
        p.bump();
        super::types::parse_type(p);
    }
    
    if p.at(SyntaxKind::SEMICOLON) {
        p.bump();
    }
}

/// Parse const declaration
pub fn parse_const(p: &mut Parser) {
    p.expect(SyntaxKind::CONST);
    p.bump(); // const name
    
    if p.at(SyntaxKind::COLON) {
        p.bump();
        super::types::parse_type(p);
    }
    
    if p.at(SyntaxKind::ASSIGN) {
        p.bump();
        // Parse initializer expression
        super::expressions::parse_expression(p);
    }
    
    if p.at(SyntaxKind::SEMICOLON) {
        p.bump();
    }
}

/// Parse import statement
pub fn parse_import(p: &mut Parser) {
    p.expect(SyntaxKind::IMPORT);
    
    parse_path(p);
    
    if p.at(SyntaxKind::AS) {
        p.bump();
        p.bump(); // alias name
    }
    
    if p.at(SyntaxKind::SEMICOLON) {
        p.bump();
    }
}

/// Parse module declaration
pub fn parse_module(p: &mut Parser) {
    p.expect(SyntaxKind::MOD);
    p.bump(); // module name
    
    if p.at(SyntaxKind::SEMICOLON) {
        p.bump();
    } else if p.at(SyntaxKind::L_BRACE) {
        parse_block(p);
    }
}

/// Parse let binding
pub fn parse_let_binding(p: &mut Parser) {
    p.expect(SyntaxKind::LET);
    
    if p.at(SyntaxKind::MUT) {
        p.bump();
    }
    
    p.bump(); // variable name
    
    if p.at(SyntaxKind::COLON) {
        p.bump();
        super::types::parse_type(p);
    }
    
    if p.at(SyntaxKind::ASSIGN) {
        p.bump();
        super::expressions::parse_expression(p);
    }
    
    if p.at(SyntaxKind::SEMICOLON) {
        p.bump();
    }
}

/// Parse generic parameters <T, U>
fn parse_generic_params(p: &mut Parser) {
    if !p.at(SyntaxKind::L_ANGLE) {
        return;
    }
    p.bump();
    
    while !p.at(SyntaxKind::R_ANGLE) && !p.at_eof() {
        p.bump(); // parameter name
        
        if p.at(SyntaxKind::COLON) {
            p.bump();
            // Parse bounds
            super::types::parse_type(p);
            
            while p.at(SyntaxKind::PLUS) {
                p.bump();
                super::types::parse_type(p);
            }
        }
        
        if p.at(SyntaxKind::COMMA) {
            p.bump();
        }
    }
    
    if p.at(SyntaxKind::R_ANGLE) {
        p.bump();
    }
}

/// Parse a path (for imports, types, etc.)
fn parse_path(p: &mut Parser) {
    p.bump(); // first segment
    
    while p.at(SyntaxKind::COLON_COLON) {
        p.bump();
        if !p.at_eof() {
            p.bump(); // next segment
        }
    }
}
