/// Parsing of top-level items and statements

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse a function definition
pub fn parse_function(p: &mut Parser) {
    p.expect(SyntaxKind::FN);
    p.bump(); // function name
    
    if p.at(SyntaxKind::L_ANGLE) {
        parse_generic_params(p);
    }
    
    parse_param_list(p);
    
    if p.at(SyntaxKind::ARROW) {
        p.bump();
        super::types::parse_type(p);
    }
    
    if p.at(SyntaxKind::L_BRACE) {
        super::expressions::parse_block_expr(p);
    }
}

/// Parse function parameter list
fn parse_param_list(p: &mut Parser) {
    p.expect(SyntaxKind::L_PAREN);
    
    while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
        if p.at(SyntaxKind::MUT) {
            p.bump();
        }
        if p.at(SyntaxKind::REF) {
            p.bump();
        }
        p.bump(); // param name
        
        if p.at(SyntaxKind::COLON) {
            p.bump();
            super::types::parse_type(p);
        }
        
        if !p.at(SyntaxKind::R_PAREN) {
            p.expect(SyntaxKind::COMMA);
        }
    }
    
    p.expect(SyntaxKind::R_PAREN);
}

/// Parse struct definition
pub fn parse_struct(p: &mut Parser) {
    p.expect(SyntaxKind::STRUCT);
    p.bump(); // struct name
    
    if p.at(SyntaxKind::L_ANGLE) {
        parse_generic_params(p);
    }
    
    if p.at(SyntaxKind::L_BRACE) {
        parse_struct_field_list(p);
    }
}

/// Parse struct field list
fn parse_struct_field_list(p: &mut Parser) {
    p.expect(SyntaxKind::L_BRACE);
    
    while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
        p.bump(); // field name
        
        if p.at(SyntaxKind::COLON) {
            p.bump();
            super::types::parse_type(p);
        }
        
        if !p.at(SyntaxKind::R_BRACE) {
            p.expect(SyntaxKind::COMMA);
        }
    }
    
    p.expect(SyntaxKind::R_BRACE);
}

/// Parse enum definition
pub fn parse_enum(p: &mut Parser) {
    p.expect(SyntaxKind::ENUM);
    p.bump(); // enum name
    
    if p.at(SyntaxKind::L_ANGLE) {
        parse_generic_params(p);
    }
    
    if p.at(SyntaxKind::L_BRACE) {
        parse_enum_variant_list(p);
    }
}

/// Parse enum variant list
fn parse_enum_variant_list(p: &mut Parser) {
    p.expect(SyntaxKind::L_BRACE);
    
    while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
        p.bump(); // variant name
        
        if p.at(SyntaxKind::L_PAREN) {
            p.bump();
            while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
                super::types::parse_type(p);
                if !p.at(SyntaxKind::R_PAREN) {
                    p.expect(SyntaxKind::COMMA);
                }
            }
            p.expect(SyntaxKind::R_PAREN);
        } else if p.at(SyntaxKind::L_BRACE) {
            parse_struct_field_list(p);
        }
        
        if !p.at(SyntaxKind::R_BRACE) {
            p.expect(SyntaxKind::COMMA);
        }
    }
    
    p.expect(SyntaxKind::R_BRACE);
}

/// Parse trait definition
pub fn parse_trait(p: &mut Parser) {
    p.expect(SyntaxKind::TRAIT);
    p.bump(); // trait name
    
    if p.at(SyntaxKind::L_ANGLE) {
        parse_generic_params(p);
    }
    
    if p.at(SyntaxKind::L_BRACE) {
        p.bump();
        while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
            match p.current() {
                SyntaxKind::FN => parse_function(p),
                SyntaxKind::TYPE => parse_type_alias(p),
                SyntaxKind::CONST => parse_const(p),
                _ => p.bump(),
            }
        }
        p.expect(SyntaxKind::R_BRACE);
    }
}

/// Parse impl block
pub fn parse_impl(p: &mut Parser) {
    p.expect(SyntaxKind::IMPL);
    
    if p.at(SyntaxKind::L_ANGLE) {
        parse_generic_params(p);
    }
    
    super::types::parse_type(p);
    
    if p.at(SyntaxKind::FOR) {
        p.bump();
        super::types::parse_type(p);
    }
    
    if p.at(SyntaxKind::L_BRACE) {
        p.bump();
        while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
            match p.current() {
                SyntaxKind::FN => parse_function(p),
                SyntaxKind::CONST => parse_const(p),
                SyntaxKind::TYPE => parse_type_alias(p),
                _ => p.bump(),
            }
        }
        p.expect(SyntaxKind::R_BRACE);
    }
}

/// Parse type alias
pub fn parse_type_alias(p: &mut Parser) {
    p.expect(SyntaxKind::TYPE);
    p.bump(); // type name
    
    if p.at(SyntaxKind::L_ANGLE) {
        parse_generic_params(p);
    }
    
    if p.at(SyntaxKind::ASSIGN) {
        p.bump();
        super::types::parse_type(p);
    }
    
    p.expect(SyntaxKind::SEMICOLON);
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
        super::expressions::parse_expr(p);
    }
    
    p.expect(SyntaxKind::SEMICOLON);
}

/// Parse import statement
pub fn parse_import(p: &mut Parser) {
    p.expect(SyntaxKind::IMPORT);
    
    parse_import_path(p);
    
    if p.at(SyntaxKind::AS) {
        p.bump();
        p.bump(); // alias
    }
    
    p.expect(SyntaxKind::SEMICOLON);
}

/// Parse import path
fn parse_import_path(p: &mut Parser) {
    p.bump();
    
    while p.at(SyntaxKind::COLON_COLON) {
        p.bump();
        if !p.at_eof() {
            p.bump();
        }
    }
}

/// Parse module declaration
pub fn parse_module(p: &mut Parser) {
    p.expect(SyntaxKind::MOD);
    p.bump(); // module name
    
    if p.at(SyntaxKind::SEMICOLON) {
        p.bump();
    } else if p.at(SyntaxKind::L_BRACE) {
        p.bump();
        while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
            super::parse_statement(p);
        }
        p.expect(SyntaxKind::R_BRACE);
    }
}

/// Parse let binding in statements
pub fn parse_let_statement(p: &mut Parser) {
    p.expect(SyntaxKind::LET);
    
    if p.at(SyntaxKind::MUT) {
        p.bump();
    }
    
    p.bump(); // pattern
    
    if p.at(SyntaxKind::COLON) {
        p.bump();
        super::types::parse_type(p);
    }
    
    if p.at(SyntaxKind::ASSIGN) {
        p.bump();
        super::expressions::parse_expr(p);
    }
    
    p.expect(SyntaxKind::SEMICOLON);
}

/// Parse generic parameter list
pub fn parse_generic_params(p: &mut Parser) {
    p.expect(SyntaxKind::L_ANGLE);
    
    while !p.at(SyntaxKind::R_ANGLE) && !p.at_eof() {
        p.bump(); // param name
        
        if p.at(SyntaxKind::COLON) {
            p.bump();
            super::types::parse_type(p);
        }
        
        if !p.at(SyntaxKind::R_ANGLE) {
            p.expect(SyntaxKind::COMMA);
        }
    }
    
    p.expect(SyntaxKind::R_ANGLE);
}
