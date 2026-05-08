use crate::{Keyword, LexErrorKind, TokenKind, lex, parse_select};
use alloc::string::ToString;
use query_ast::{CompareOp, Expr, Literal};

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
fn lexer_can_tokenize_multiline_string_literal() {
    let tokens = lex("filter .body = \"hello\nworld\" title").expect("query should lex");

    assert_eq!(
        tokens[4].kind(),
        &TokenKind::String("hello\nworld".to_string())
    );

    let string_span = tokens[4].span();
    assert_eq!(string_span.start().line(), 1);
    assert_eq!(string_span.start().column(), 16);
    assert_eq!(string_span.end().line(), 2);
    assert_eq!(string_span.end().column(), 7);

    assert_eq!(tokens[5].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(tokens[5].span().start().line(), 2);
    assert_eq!(tokens[5].span().start().column(), 8);
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
fn lexer_can_tokenize_literal_keywords() {
    let tokens = lex("true false null").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::True));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::False));
    assert_eq!(tokens[2].kind(), &TokenKind::Keyword(Keyword::Null));
}

#[test]
fn lexer_treats_boolean_operator_words_as_identifiers_until_supported() {
    let tokens = lex("and or not").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Ident("and".to_string()));
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("or".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("not".to_string()));
}

#[test]
fn lexer_reports_unexpected_character_byte_offset() {
    let error = lex("select Post { @ }").expect_err("query should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('@'));
    assert_eq!(error.position().line(), 1);
    assert_eq!(error.position().column(), 15);
    assert_eq!(error.position().byte(), 14);
}

#[test]
fn parser_can_parse_select_shape() {
    let query = parse_select("select Post { title }").expect("query should parse");

    assert_eq!(query.root_type_name(), "Post");
    assert_eq!(query.shape().items().len(), 1);

    let item = &query.shape().items()[0];
    assert_eq!(item.path().steps()[0].field_name(), "title");
    assert!(item.child_shape().is_none());

    assert!(query.filter().is_none());
    assert!(query.order_by().is_empty());
    assert_eq!(query.limit(), None);
    assert_eq!(query.offset(), None);
}

#[test]
fn parser_preserves_shape_item_order() {
    let query = parse_select("select Post { id, title }").expect("query should parse");

    assert_eq!(query.root_type_name(), "Post");
    assert_eq!(query.shape().items().len(), 2);

    let item = &query.shape().items()[0];
    assert_eq!(item.path().steps()[0].field_name(), "id");
    assert!(item.child_shape().is_none());

    let item = &query.shape().items()[1];
    assert_eq!(item.path().steps()[0].field_name(), "title");
    assert!(item.child_shape().is_none());

    assert!(query.filter().is_none());
    assert!(query.order_by().is_empty());
    assert_eq!(query.limit(), None);
    assert_eq!(query.offset(), None);
}

#[test]
fn parser_rejects_adjacent_shape_items_without_comma() {
    let error =
        parse_select("select Post { id title }").expect_err("query should fail without comma");

    assert_eq!(
        error.kind(),
        &crate::ParseErrorKind::UnexpectedToken { expected: ", or }" }
    );

    let span = error
        .span()
        .expect("parse error should point to unexpected token");

    assert_eq!(span.start().line(), 1);
    assert_eq!(span.start().column(), 18);
    assert_eq!(span.start().byte(), 17);
    assert_eq!(span.end().line(), 1);
    assert_eq!(span.end().column(), 23);
    assert_eq!(span.end().byte(), 22);
}

#[test]
fn parser_can_parse_nested_shape_item() {
    let query = parse_select("select Post { author: { name } }").expect("query should parse");

    assert_eq!(query.root_type_name(), "Post");

    let root_items = query.shape().items();
    assert_eq!(root_items.len(), 1);

    let author_item = &root_items[0];
    assert_eq!(author_item.path().steps().len(), 1);
    assert_eq!(author_item.path().steps()[0].field_name(), "author");

    let author_shape = author_item
        .child_shape()
        .expect("author should have child shape");

    let author_items = author_shape.items();
    assert_eq!(author_items.len(), 1);

    let name_item = &author_items[0];
    assert_eq!(name_item.path().steps().len(), 1);
    assert_eq!(name_item.path().steps()[0].field_name(), "name");
    assert!(name_item.child_shape().is_none());
}

#[test]
fn parser_can_parse_deeply_nested_shape_item() {
    let query = parse_select("select Post { author: { human: { birthday } } }")
        .expect("query should parse");

    let author_item = &query.shape().items()[0];
    assert_eq!(author_item.path().steps()[0].field_name(), "author");

    let author_shape = author_item
        .child_shape()
        .expect("author should have child shape");
    let human_item = &author_shape.items()[0];
    assert_eq!(human_item.path().steps()[0].field_name(), "human");

    let human_shape = human_item
        .child_shape()
        .expect("human should have child shape");
    let birthday_item = &human_shape.items()[0];
    assert_eq!(birthday_item.path().steps()[0].field_name(), "birthday");
    assert!(birthday_item.child_shape().is_none());
}

#[test]
fn parser_can_parse_filter_compare_path_equals_string_literal() {
    let query =
        parse_select("select Post {title} filter .title = \"Hello\"").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_eq!(compare.left().steps().len(), 1);
            assert_eq!(compare.left().steps()[0].field_name(), "title");
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_eq!(compare.right(), &Literal::String("Hello".to_string()));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_compare_nested_path_equals_string_literal() {
    let query = parse_select("select Post { title } filter .author.name = \"Sheri\"")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_eq!(compare.left().steps().len(), 2);
            assert_eq!(compare.left().steps()[0].field_name(), "author");
            assert_eq!(compare.left().steps()[1].field_name(), "name");
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_eq!(compare.right(), &Literal::String("Sheri".to_string()));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_select_without_filter() {
    let query = parse_select("select Post { title }").expect("query should parse");

    assert!(query.filter().is_none());
}

#[test]
fn parser_rejects_filter_without_path() {
    let error =
        parse_select("select Post { title } filter = \"Hello\"").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &crate::ParseErrorKind::UnexpectedToken { expected: "IDENT" }
    );
}

#[test]
fn parser_rejects_filter_without_comparison_operator() {
    let error =
        parse_select("select Post { title } filter title \"Hello\"").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &crate::ParseErrorKind::UnexpectedToken {
            expected: "comparison operator"
        }
    );
}

#[test]
fn parser_rejects_filter_without_literal() {
    let error = parse_select("select Post { title } filter title =").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &crate::ParseErrorKind::UnexpectedEof {
            expected: "literal"
        }
    );
}
