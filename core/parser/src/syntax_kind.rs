//! The shared syntactic vocabulary for the parser.
//!
//! [`SyntaxKind`] enumerates every lexical token kind and every syntax-tree node
//! kind in a single `#[repr(u16)]` enum, so the lexer, parser engine, CST and
//! lowering all speak the same alphabet (issue #62, internal design §2).
//!
//! # Discriminant layout
//!
//! **All token kinds are declared first** (discriminants `0..`), followed by all
//! node kinds. This ordering is load-bearing: [`crate::token_set::TokenSet`] is a
//! `u128` bitset indexed by discriminant and only ever stores *token* kinds
//! (recovery and expectation sets are token-only). Keeping token discriminants in
//! `0..128` lets the bitset hold any token. The assertion below pins this.
//!
//! # Node-kind → grammar-rule mapping
//!
//! Node-kind variants mirror the grammar production names, so that lowering can
//! switch on node kind one rule at a time:
//!
//! | `SyntaxKind`                  | grammar rule                     |
//! |-------------------------------|----------------------------------|
//! | `SourceFile`                  | `source_file`                    |
//! | `UseDirective`                | `use_directive`                  |
//! | `SpecDefinition`              | `spec_definition`                |
//! | `FunctionDefinition`          | `function_definition`            |
//! | `ExternalFunctionDefinition`  | `external_function_definition`   |
//! | `StructDefinition`            | `struct_definition`              |
//! | `StructField`                 | `struct_field`                   |
//! | `EnumDefinition`              | `enum_definition`                |
//! | `ConstantDefinition`          | `constant_definition`            |
//! | `TypeDefinitionStatement`     | `type_definition_statement`      |
//! | `ArgumentList`                | `argument_list`                  |
//! | `ArgumentDeclaration`         | `argument_declaration`           |
//! | `SelfReference`               | `self_reference`                 |
//! | `IgnoreArgument`              | `ignore_argument`                |
//! | `TypeArgumentListDefinition`  | `type_argument_list_definition`  |
//! | `TypeArgumentList`            | `type_argument_list`             |
//! | `Visibility`                  | `visibility`                     |
//! | `MutKeyword`                  | `mut_keyword`                    |
//! | `Block`                       | `block`                          |
//! | `AssumeBlock`                 | `assume_block`                   |
//! | `ForallBlock`                 | `forall_block`                   |
//! | `ExistsBlock`                 | `exists_block`                   |
//! | `UniqueBlock`                 | `unique_block`                   |
//! | `ExpressionStatement`         | `expression_statement`           |
//! | `AssignStatement`             | `assign_statement`               |
//! | `ReturnStatement`             | `return_statement`               |
//! | `LoopStatement`               | `loop_statement`                 |
//! | `IfStatement`                 | `if_statement`                   |
//! | `VariableDefinitionStatement` | `variable_definition_statement`  |
//! | `AssertStatement`             | `assert_statement`               |
//! | `BreakStatement`              | `break_statement`                |
//! | `BinaryExpression`            | `binary_expression`              |
//! | `PrefixUnaryExpression`       | `prefix_unary_expression`        |
//! | `ParenthesizedExpression`     | `parenthesized_expression`       |
//! | `FunctionCallExpression`      | `function_call_expression`       |
//! | `ArrayIndexAccessExpression`  | `array_index_access_expression`  |
//! | `MemberAccessExpression`      | `member_access_expression`       |
//! | `TypeMemberAccessExpression`  | `type_member_access_expression`  |
//! | `StructExpression`            | `struct_expression`              |
//! | `ArrayLiteral`                | `array_literal`                  |
//! | `BoolLiteral`                 | `bool_literal`                   |
//! | `StringLiteral`               | `string`                         |
//! | `NumberLiteral`               | `number`                         |
//! | `UnitLiteral`                 | `unit`                           |
//! | `UzumakiKeyword`              | `uzumaki_keyword`                |
//! | `Identifier`                  | `identifier`                     |
//! | `GenericName`                 | `generic_name`                   |
//! | `TypeQualifiedName`           | `type_qualified_name`            |
//! | `TypeUnit`                    | `unit_type`                      |
//! | `TypeBool`                    | `primitive_type` (`bool`)        |
//! | `TypeI8` … `TypeU64`          | `primitive_type` (`i8` … `u64`)  |
//! | `TypeArray`                   | `array_type`                     |
//! | `TypeFn`                      | `function_type`                  |
//! | `UnaryNot`/`UnaryMinus`/`UnaryBitnot` | `prefix_unary_expression` operator variants |

/// Every lexical token kind and syntax-tree node kind, sharing one discriminant
/// space so the lexer, parser, CST and lowering all agree on the vocabulary.
///
/// Token kinds occupy the low discriminants (`0..`) so they fit a `u128`
/// [`crate::token_set::TokenSet`]; node kinds follow.
#[repr(u16)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[allow(dead_code)] // node kinds are referenced from Phase 2 onward.
pub enum SyntaxKind {
    // -- Trivia (token) --
    Whitespace,
    /// A `//` line comment, up to end of line.
    Comment,
    /// A `///` doc comment, up to end of line (checked before `Comment`).
    DocComment,

    // -- Literals (token) --
    /// `-?\d+` integer literal.
    Number,
    /// A whole `"..."` string, including the quotes.
    String,
    /// The `true` keyword (lexed as a dedicated kind, not `Ident`).
    TrueKw,
    /// The `false` keyword (lexed as a dedicated kind, not `Ident`).
    FalseKw,
    /// An identifier `[A-Za-z_]\w*` that is not a keyword.
    Ident,

    // -- Keywords (token) --
    FnKw,
    LetKw,
    MutKw,
    SpecKw,
    StructKw,
    EnumKw,
    ConstKw,
    TypeKw,
    ExternalKw,
    ReturnKw,
    LoopKw,
    IfKw,
    ElseKw,
    AssertKw,
    BreakKw,
    UseKw,
    FromKw,
    SelfKw,
    PubKw,
    AssumeKw,
    ForallKw,
    ExistsKw,
    UniqueKw,

    // -- Type keywords (token) --
    I8Kw,
    I16Kw,
    I32Kw,
    I64Kw,
    U8Kw,
    U16Kw,
    U32Kw,
    U64Kw,
    BoolKw,

    // -- Punctuation (token) --
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semi,
    Colon,
    ColonColon,
    Comma,
    Dot,
    At,
    Tick,
    Arrow,
    Underscore,

    // -- Operators (token) --
    Star,
    StarStar,
    Slash,
    Percent,
    Plus,
    Minus,
    Amp,
    AmpAmp,
    Pipe,
    PipePipe,
    Caret,
    Shl,
    Shr,
    Lt,
    Le,
    Gt,
    Ge,
    EqEq,
    Ne,
    Eq,
    Bang,
    Tilde,

    // -- Special (token) --
    /// An unrecognized or malformed run (e.g. an unterminated string).
    Error,
    /// Zero-width end-of-input sentinel, always the last token.
    Eof,

    // -- Nodes (parser output, unused before Phase 2) --
    SourceFile,
    UseDirective,
    SpecDefinition,
    FunctionDefinition,
    ExternalFunctionDefinition,
    StructDefinition,
    StructField,
    EnumDefinition,
    ConstantDefinition,
    TypeDefinitionStatement,
    ArgumentList,
    ArgumentDeclaration,
    SelfReference,
    IgnoreArgument,
    TypeArgumentListDefinition,
    TypeArgumentList,
    Visibility,
    MutKeyword,
    Block,
    AssumeBlock,
    ForallBlock,
    ExistsBlock,
    UniqueBlock,
    ExpressionStatement,
    AssignStatement,
    ReturnStatement,
    LoopStatement,
    IfStatement,
    VariableDefinitionStatement,
    AssertStatement,
    BreakStatement,
    BinaryExpression,
    PrefixUnaryExpression,
    ParenthesizedExpression,
    FunctionCallExpression,
    ArrayIndexAccessExpression,
    MemberAccessExpression,
    TypeMemberAccessExpression,
    StructExpression,
    ArrayLiteral,
    BoolLiteral,
    StringLiteral,
    NumberLiteral,
    UnitLiteral,
    UzumakiKeyword,
    Identifier,
    GenericName,
    TypeQualifiedName,
    TypeUnit,
    TypeBool,
    TypeI8,
    TypeI16,
    TypeI32,
    TypeI64,
    TypeU8,
    TypeU16,
    TypeU32,
    TypeU64,
    TypeArray,
    TypeFn,
    UnaryNot,
    UnaryMinus,
    UnaryBitnot,
}

/// The first node-kind discriminant; every value below it is a token kind.
///
/// [`crate::token_set::TokenSet`] relies on this being `<= 128` so all token
/// discriminants fit its `u128` bitset.
pub(crate) const FIRST_NODE: u16 = SyntaxKind::SourceFile as u16;

// All token discriminants must fit a `u128` bitset (issue #62 design §5).
const _: () = assert!(FIRST_NODE <= 128, "token kinds must fit a u128 TokenSet");

impl SyntaxKind {
    /// Whether this kind is a lexical token (as opposed to a syntax-tree node).
    ///
    /// Token kinds occupy the low discriminants, so only they may be stored in a
    /// [`crate::token_set::TokenSet`].
    #[must_use]
    pub fn is_token(self) -> bool {
        (self as u16) < FIRST_NODE
    }

    /// Whether this kind is trivia the parser skips: whitespace or comments.
    #[must_use]
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::DocComment
        )
    }

    /// Maps an identifier spelling to its keyword kind, or `None` for a plain
    /// identifier.
    ///
    /// This covers the language keywords, the nine type keywords, and the
    /// `true`/`false`/`self` literals. The reserved identifiers `constructor`,
    /// `proof` and `uzumaki` are intentionally absent: the grammar treats them as
    /// ordinary identifiers, so they fall through to [`SyntaxKind::Ident`].
    #[must_use]
    pub fn from_keyword(text: &str) -> Option<SyntaxKind> {
        let kind = match text {
            "fn" => SyntaxKind::FnKw,
            "let" => SyntaxKind::LetKw,
            "mut" => SyntaxKind::MutKw,
            "spec" => SyntaxKind::SpecKw,
            "struct" => SyntaxKind::StructKw,
            "enum" => SyntaxKind::EnumKw,
            "const" => SyntaxKind::ConstKw,
            "type" => SyntaxKind::TypeKw,
            "external" => SyntaxKind::ExternalKw,
            "return" => SyntaxKind::ReturnKw,
            "loop" => SyntaxKind::LoopKw,
            "if" => SyntaxKind::IfKw,
            "else" => SyntaxKind::ElseKw,
            "assert" => SyntaxKind::AssertKw,
            "break" => SyntaxKind::BreakKw,
            "use" => SyntaxKind::UseKw,
            "from" => SyntaxKind::FromKw,
            "self" => SyntaxKind::SelfKw,
            "pub" => SyntaxKind::PubKw,
            "assume" => SyntaxKind::AssumeKw,
            "forall" => SyntaxKind::ForallKw,
            "exists" => SyntaxKind::ExistsKw,
            "unique" => SyntaxKind::UniqueKw,
            "i8" => SyntaxKind::I8Kw,
            "i16" => SyntaxKind::I16Kw,
            "i32" => SyntaxKind::I32Kw,
            "i64" => SyntaxKind::I64Kw,
            "u8" => SyntaxKind::U8Kw,
            "u16" => SyntaxKind::U16Kw,
            "u32" => SyntaxKind::U32Kw,
            "u64" => SyntaxKind::U64Kw,
            "bool" => SyntaxKind::BoolKw,
            "true" => SyntaxKind::TrueKw,
            "false" => SyntaxKind::FalseKw,
            _ => return None,
        };
        Some(kind)
    }

    /// The raw `u16` discriminant of this kind.
    #[must_use]
    pub fn to_u16(self) -> u16 {
        self as u16
    }

    /// Reconstructs a kind from its raw `u16` discriminant.
    ///
    /// Returns `None` if `raw` is outside the enum's range.
    #[must_use]
    pub fn from_u16(raw: u16) -> Option<SyntaxKind> {
        if raw <= SyntaxKind::UnaryBitnot as u16 {
            // Safe: discriminants are dense `0..=UnaryBitnot` with `#[repr(u16)]`.
            Some(unsafe { std::mem::transmute::<u16, SyntaxKind>(raw) })
        } else {
            None
        }
    }
}

impl From<SyntaxKind> for u16 {
    fn from(kind: SyntaxKind) -> u16 {
        kind.to_u16()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_discriminants_fit_u128() {
        // The TokenSet bitset requires every token kind below bit 128; the
        // compile-time assertion in this module already pins `FIRST_NODE`, so
        // here we just confirm the token/node boundary classifies correctly.
        assert!(SyntaxKind::Eof.is_token());
        assert!(SyntaxKind::Whitespace.is_token());
        assert!(!SyntaxKind::SourceFile.is_token());
        assert!(!SyntaxKind::UnaryBitnot.is_token());
    }

    #[test]
    fn trivia_classification() {
        assert!(SyntaxKind::Whitespace.is_trivia());
        assert!(SyntaxKind::Comment.is_trivia());
        assert!(SyntaxKind::DocComment.is_trivia());
        assert!(!SyntaxKind::Ident.is_trivia());
        assert!(!SyntaxKind::Eof.is_trivia());
    }

    #[test]
    fn keyword_table_maps_keywords() {
        assert_eq!(SyntaxKind::from_keyword("fn"), Some(SyntaxKind::FnKw));
        assert_eq!(SyntaxKind::from_keyword("bool"), Some(SyntaxKind::BoolKw));
        assert_eq!(SyntaxKind::from_keyword("true"), Some(SyntaxKind::TrueKw));
        assert_eq!(SyntaxKind::from_keyword("false"), Some(SyntaxKind::FalseKw));
        assert_eq!(SyntaxKind::from_keyword("self"), Some(SyntaxKind::SelfKw));
    }

    #[test]
    fn reserved_idents_are_not_keywords() {
        assert_eq!(SyntaxKind::from_keyword("constructor"), None);
        assert_eq!(SyntaxKind::from_keyword("proof"), None);
        assert_eq!(SyntaxKind::from_keyword("uzumaki"), None);
        assert_eq!(SyntaxKind::from_keyword("foo"), None);
    }

    #[test]
    fn u16_round_trip() {
        for kind in [
            SyntaxKind::Whitespace,
            SyntaxKind::FnKw,
            SyntaxKind::Number,
            SyntaxKind::ColonColon,
            SyntaxKind::Eof,
            SyntaxKind::SourceFile,
            SyntaxKind::UnaryBitnot,
        ] {
            assert_eq!(SyntaxKind::from_u16(kind.to_u16()), Some(kind));
            assert_eq!(u16::from(kind), kind.to_u16());
        }
    }

    #[test]
    fn from_u16_rejects_out_of_range() {
        let past_end = SyntaxKind::UnaryBitnot as u16 + 1;
        assert_eq!(SyntaxKind::from_u16(past_end), None);
        assert_eq!(SyntaxKind::from_u16(u16::MAX), None);
    }
}
