use crate::{Keyword, LexErrorKind, TokenKind, lex};
use alloc::string::ToString;

#[test]
fn lexer_can_tokenize_select_shape() {
    let tokens = lex("select Post { title }").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Select));
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("Post".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::LBrace);
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::RBrace);
}

#[test]
fn lexer_tracks_line_and_column_for_tokens() {
    let tokens = lex("select Post {\n  title\n}").expect("query should lex");
    let title_span = tokens[3].span();

    assert_eq!(title_span.start().line(), 2);
    assert_eq!(title_span.start().column(), 3);
    assert_eq!(title_span.end().line(), 2);
    assert_eq!(title_span.end().column(), 8);
}

#[test]
fn lexer_can_tokenize_filter_path_and_string_literal() {
    let tokens = lex("filter .title = \"Hello\"").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Filter));
    assert_eq!(tokens[1].kind(), &TokenKind::Dot);
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(tokens[3].kind(), &TokenKind::Eq);
    assert_eq!(tokens[4].kind(), &TokenKind::String("Hello".to_string()));
}

#[test]
fn lexer_reports_unexpected_character_position() {
    let error = lex("select Post {\n  @\n}").expect_err("query should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('@'));
    assert_eq!(error.position().line(), 2);
    assert_eq!(error.position().column(), 3);
}

#[test]
fn lexer_reports_unterminated_string_start_position() {
    let error = lex("filter .title = \"Hello").expect_err("query should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnterminatedString);
    assert_eq!(error.position().line(), 1);
    assert_eq!(error.position().column(), 17);
}

#[test]
fn lexer_can_tokenize_nested_shape() {
    let tokens =
        lex("select Post {\n  title,\n  author: {\n    name\n  }\n}").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Select));
    assert_eq!(tokens[5].kind(), &TokenKind::Ident("author".to_string()));
    assert_eq!(tokens[6].kind(), &TokenKind::Colon);
    assert_eq!(tokens[7].kind(), &TokenKind::LBrace);
    assert_eq!(tokens[8].kind(), &TokenKind::Ident("name".to_string()));
    assert_eq!(tokens[9].kind(), &TokenKind::RBrace);
}

#[test]
fn lexer_tracks_columns_after_newline() {
    let tokens = lex("select Post {\n    title\n}").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Select));
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("title".to_string()));

    let title_span = tokens[3].span();
    assert_eq!(title_span.start().byte(), 18);
    assert_eq!(title_span.start().line(), 2);
    assert_eq!(title_span.start().column(), 5);
    assert_eq!(title_span.end().byte(), 23);
    assert_eq!(title_span.end().line(), 2);
    assert_eq!(title_span.end().column(), 10);
}

#[test]
fn lexer_tracks_byte_and_column_inside_unicode_string_literal() {
    let tokens = lex("filter .title = \"안녕\" title").expect("query should lex");

    let string_span = tokens[4].span();
    assert_eq!(tokens[4].kind(), &TokenKind::String("안녕".to_string()));
    assert_eq!(string_span.start().line(), 1);
    assert_eq!(string_span.start().column(), 17);
    assert_eq!(string_span.start().byte(), 16);
    assert_eq!(string_span.end().column(), 21);
    assert_eq!(string_span.end().byte(), 24);

    let title_span = tokens[5].span();
    assert_eq!(tokens[5].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(title_span.start().line(), 1);
    assert_eq!(title_span.start().column(), 22);
    assert_eq!(title_span.start().byte(), 25);
}

#[test]
fn lexer_can_tokenize_order_limit_offset() {
    let tokens = lex("order by .title desc limit 10 offset 20").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Order));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::By));
    assert_eq!(tokens[2].kind(), &TokenKind::Dot);
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::Keyword(Keyword::Desc));
    assert_eq!(tokens[5].kind(), &TokenKind::Keyword(Keyword::Limit));
    assert_eq!(tokens[6].kind(), &TokenKind::Int("10".to_string()));
    assert_eq!(tokens[7].kind(), &TokenKind::Keyword(Keyword::Offset));
    assert_eq!(tokens[8].kind(), &TokenKind::Int("20".to_string()));
}

#[test]
fn lexer_distinguishes_keyword_prefix_identifiers() {
    let tokens =
        lex("select Post { orderValue, offsetCount, filterText }").expect("query should lex");

    assert_eq!(
        tokens[3].kind(),
        &TokenKind::Ident("orderValue".to_string())
    );
    assert_eq!(
        tokens[5].kind(),
        &TokenKind::Ident("offsetCount".to_string())
    );
    assert_eq!(
        tokens[7].kind(),
        &TokenKind::Ident("filterText".to_string())
    );
}

#[test]
fn lexer_can_tokenize_boolean_filter_keywords() {
    let tokens = lex("filter not .published = true or .status = null").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Filter));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::Not));
    assert_eq!(tokens[2].kind(), &TokenKind::Dot);
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("published".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::Eq);
    assert_eq!(tokens[5].kind(), &TokenKind::Keyword(Keyword::True));
    assert_eq!(tokens[6].kind(), &TokenKind::Keyword(Keyword::Or));
    assert_eq!(tokens[7].kind(), &TokenKind::Dot);
    assert_eq!(tokens[8].kind(), &TokenKind::Ident("status".to_string()));
    assert_eq!(tokens[9].kind(), &TokenKind::Eq);
    assert_eq!(tokens[10].kind(), &TokenKind::Keyword(Keyword::Null));
}

#[test]
fn lexer_reports_unexpected_character_byte_offset() {
    let error = lex("select Post { @ }").expect_err("query should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('@'));
    assert_eq!(error.position().line(), 1);
    assert_eq!(error.position().column(), 15);
    assert_eq!(error.position().byte(), 14);
}
