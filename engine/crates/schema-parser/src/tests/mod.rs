use crate::{Keyword, LexErrorKind, TokenKind, lex};
use alloc::string::ToString;

#[test]
fn lexer_tokenizes_empty_type_declaration() {
    let tokens = lex("type User {}").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Type));
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("User".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::LBrace);
    assert_eq!(tokens[3].kind(), &TokenKind::RBrace);
}

#[test]
fn lexer_tracks_line_and_column_for_field_tokens() {
    let tokens = lex("type User {\n  required name: str\n}").expect("schema should lex");
    let required_span = tokens[3].span();
    let name_span = tokens[4].span();

    assert_eq!(required_span.start().line(), 2);
    assert_eq!(required_span.start().column(), 3);
    assert_eq!(required_span.end().line(), 2);
    assert_eq!(required_span.end().column(), 11);

    assert_eq!(name_span.start().line(), 2);
    assert_eq!(name_span.start().column(), 12);
    assert_eq!(name_span.end().line(), 2);
    assert_eq!(name_span.end().column(), 16);
}

#[test]
fn lexer_tokenizes_scalar_field_modifiers_and_types() {
    let tokens = lex("required unique property email: str").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Required));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::Unique));
    assert_eq!(tokens[2].kind(), &TokenKind::Keyword(Keyword::Property));
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("email".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::Colon);
    assert_eq!(tokens[5].kind(), &TokenKind::Keyword(Keyword::Str));
}

#[test]
fn lexer_tokenizes_link_modifiers() {
    let tokens = lex("multi link posts: Post").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Multi));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::Link));
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("posts".to_string()));
    assert_eq!(tokens[3].kind(), &TokenKind::Colon);
    assert_eq!(tokens[4].kind(), &TokenKind::Ident("Post".to_string()));
}

#[test]
fn lexer_tokenizes_all_scalar_type_keywords() {
    let tokens = lex("str int64 float64 bool uuid datetime").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Str));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::Int64));
    assert_eq!(tokens[2].kind(), &TokenKind::Keyword(Keyword::Float64));
    assert_eq!(tokens[3].kind(), &TokenKind::Keyword(Keyword::Bool));
    assert_eq!(tokens[4].kind(), &TokenKind::Keyword(Keyword::Uuid));
    assert_eq!(tokens[5].kind(), &TokenKind::Keyword(Keyword::DateTime));
}

#[test]
fn lexer_distinguishes_keyword_prefix_identifiers() {
    let tokens = lex("typeName requiredField linkTarget").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Ident("typeName".to_string()));
    assert_eq!(
        tokens[1].kind(),
        &TokenKind::Ident("requiredField".to_string())
    );
    assert_eq!(
        tokens[2].kind(),
        &TokenKind::Ident("linkTarget".to_string())
    );
}

#[test]
fn lexer_reports_unexpected_character_position() {
    let error = lex("type User {\n  @\n}").expect_err("schema should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('@'));
    assert_eq!(error.position().line(), 2);
    assert_eq!(error.position().column(), 3);
    assert_eq!(error.position().byte(), 14);
}
