mod fixtures;

use crate::{
    Keyword::{self},
    LexErrorKind, TokenKind, lex, parse_select,
};
use alloc::string::ToString;
use fixtures::{assert_literal_expr, assert_path_expr};
use query_ast::{CompareOp, Expr, InOp, Literal, OrderDirection};

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
fn lexer_can_tokenize_membership_list_brackets() {
    let tokens = lex("filter .status in [\"draft\", \"published\"]").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Filter));
    assert_eq!(tokens[1].kind(), &TokenKind::Dot);
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("status".to_string()));
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("in".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::LBracket);
    assert_eq!(tokens[5].kind(), &TokenKind::String("draft".to_string()));
    assert_eq!(tokens[6].kind(), &TokenKind::Comma);
    assert_eq!(
        tokens[7].kind(),
        &TokenKind::String("published".to_string())
    );
    assert_eq!(tokens[8].kind(), &TokenKind::RBracket);
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
fn lexer_treats_boolean_operator_words_as_identifiers() {
    let tokens = lex("and or not").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Ident("and".to_string()));
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("or".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("not".to_string()));
}

#[test]
fn lexer_treats_membership_operator_word_as_identifier() {
    let tokens = lex("in").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Ident("in".to_string()));
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
fn parser_can_parse_shape_item_named_boolean_operator_word() {
    let query = parse_select("select Post { or }").expect("query should parse");

    assert_eq!(query.shape().items().len(), 1);

    let item = &query.shape().items()[0];
    assert_eq!(item.path().steps()[0].field_name(), "or");
    assert!(item.child_shape().is_none());
}

#[test]
fn parser_can_parse_shape_item_named_membership_operator_word() {
    let query = parse_select("select Post { in }").expect("query should parse");

    assert_eq!(query.shape().items().len(), 1);

    let item = &query.shape().items()[0];
    assert_eq!(item.path().steps()[0].field_name(), "in");
    assert!(item.child_shape().is_none());
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
            assert_path_expr(compare.left(), &["title"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::String("Hello".to_string()));
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
            assert_path_expr(compare.left(), &["author", "name"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::String("Sheri".to_string()));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_compare_path_equals_integer_literal() {
    let query =
        parse_select("select Post { title } filter .view_count = 42").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["view_count"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::Int64(42));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_compare_path_equals_true_literal() {
    let query =
        parse_select("select Post { title } filter .published = true").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["published"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::Bool(true));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_compare_path_equals_false_literal() {
    let query = parse_select("select Post { title } filter .published = false")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["published"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::Bool(false));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_compare_path_equals_null_literal() {
    let query = parse_select("select Post { title } filter .deleted_at = null")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["deleted_at"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::Null);
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_in_literal_list() {
    let query = parse_select("select Post { title } filter .status in [\"draft\", \"published\"]")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["status"]);
            assert_eq!(in_expr.op(), InOp::In);
            assert_eq!(in_expr.right().len(), 2);
            assert_literal_expr(&in_expr.right()[0], &Literal::String("draft".to_string()));
            assert_literal_expr(
                &in_expr.right()[1],
                &Literal::String("published".to_string()),
            );
        }
        _ => panic!("filter should be an in expression"),
    }
}

#[test]
fn parser_can_parse_filter_not_in_literal_list() {
    let query = parse_select("select Post { title } filter .status not in [\"archived\"]")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["status"]);
            assert_eq!(in_expr.op(), InOp::NotIn);
            assert_eq!(in_expr.right().len(), 1);
            assert_literal_expr(
                &in_expr.right()[0],
                &Literal::String("archived".to_string()),
            );
        }
        _ => panic!("filter should be a not in expression"),
    }
}

#[test]
fn parser_can_parse_filter_in_empty_list() {
    let query =
        parse_select("select Post { title } filter .status in []").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["status"]);
            assert_eq!(in_expr.op(), InOp::In);
            assert!(in_expr.right().is_empty());
        }
        _ => panic!("filter should be an in expression"),
    }
}

#[test]
fn parser_preserves_null_and_path_membership_items_for_resolver() {
    let query = parse_select("select Post { title } filter .status in [null, .other_status]")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["status"]);
            assert_eq!(in_expr.op(), InOp::In);
            assert_eq!(in_expr.right().len(), 2);
            assert_literal_expr(&in_expr.right()[0], &Literal::Null);
            assert_path_expr(&in_expr.right()[1], &["other_status"]);
        }
        _ => panic!("filter should be an in expression"),
    }
}

#[test]
fn parser_preserves_membership_precedence_with_boolean_or() {
    let query =
        parse_select("select Post { title } filter .status in [\"draft\"] or .title = \"Hello\"")
            .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Or(left, right) => {
            assert!(matches!(left.as_ref(), Expr::In(_)));
            assert!(matches!(right.as_ref(), Expr::Compare(_)));
        }
        _ => panic!("filter should be an or expression"),
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
        &crate::ParseErrorKind::UnexpectedToken {
            expected: "expression"
        }
    );
}

#[test]
fn parser_rejects_filter_without_comparison_operator() {
    let error = parse_select("select Post { title } filter title \"Hello\"")
        .expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &crate::ParseErrorKind::UnexpectedToken {
            expected: "comparison operator"
        }
    );
}

#[test]
fn parser_rejects_filter_without_literal() {
    let error =
        parse_select("select Post { title } filter title =").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &crate::ParseErrorKind::UnexpectedEof {
            expected: "expression"
        }
    );
}

#[test]
fn parser_can_parse_order_by_path_desc() {
    let query =
        parse_select("select Post {title} order by .title desc").expect("query should parse");

    assert_eq!(query.order_by().len(), 1);

    let order = &query.order_by()[0];
    assert_eq!(order.path().steps().len(), 1);
    assert_eq!(order.path().steps()[0].field_name(), "title");
    assert_eq!(order.direction(), OrderDirection::Desc);
}

#[test]
fn parser_defaults_order_direction_to_asc() {
    let query = parse_select("select Post {title} order by .title").expect("query should parse");

    assert_eq!(query.order_by().len(), 1);

    let order = &query.order_by()[0];
    assert_eq!(order.path().steps().len(), 1);
    assert_eq!(order.path().steps()[0].field_name(), "title");
    assert_eq!(order.direction(), OrderDirection::Asc);
}

#[test]
fn parser_can_parse_order_by_nested_path() {
    let query =
        parse_select("select Post {title} order by .author.birthday").expect("query should parse");

    assert_eq!(query.order_by().len(), 1);

    let order = &query.order_by()[0];
    assert_eq!(order.path().steps().len(), 2);
    assert_eq!(order.path().steps()[0].field_name(), "author");
    assert_eq!(order.path().steps()[1].field_name(), "birthday");
    assert_eq!(order.direction(), OrderDirection::Asc);
}

#[test]
fn parser_can_parse_multiple_order_by_items() {
    let query = parse_select("select Post {title} order by .title desc, .created_at asc")
        .expect("query should parse");

    assert_eq!(query.order_by().len(), 2);

    let order = &query.order_by()[0];
    assert_eq!(order.path().steps().len(), 1);
    assert_eq!(order.path().steps()[0].field_name(), "title");
    assert_eq!(order.direction(), OrderDirection::Desc);

    let order = &query.order_by()[1];
    assert_eq!(order.path().steps().len(), 1);
    assert_eq!(order.path().steps()[0].field_name(), "created_at");
    assert_eq!(order.direction(), OrderDirection::Asc);
}

#[test]
fn parser_rejects_order_by_without_path() {
    let error = parse_select("select Post { title } order by desc").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &crate::ParseErrorKind::UnexpectedToken { expected: "IDENT" }
    );
}

#[test]
fn parser_rejects_order_without_by() {
    let error = parse_select("select Post { title } order .title").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &crate::ParseErrorKind::UnexpectedToken { expected: "by" }
    );
}

#[test]
fn parser_can_parse_limit() {
    let query = parse_select("select Post { title } limit 10").expect("query should parse");

    assert_eq!(query.limit(), Some(10))
}

#[test]
fn parser_can_parse_offset() {
    let query = parse_select("select Post { title } offset 10").expect("query should parse");

    assert_eq!(query.offset(), Some(10))
}

#[test]
fn parser_can_parse_limit_and_offset() {
    let query =
        parse_select("select Post { title } limit 20 offset 10").expect("query should parse");

    assert_eq!(query.limit(), Some(20));
    assert_eq!(query.offset(), Some(10));
}

#[test]
fn parser_rejects_negative_limit() {
    let error = parse_select("select Post { title } limit -10").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &crate::ParseErrorKind::UnexpectedValue {
            expected: "non-negative integer"
        }
    );
}

#[test]
fn parser_rejects_negative_offset() {
    let error = parse_select("select Post { title } offset -10").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &crate::ParseErrorKind::UnexpectedValue {
            expected: "non-negative integer"
        }
    );
}

#[test]
fn parser_rejects_integer_literal_outside_i64_range() {
    let error = parse_select("select Post { title } filter .view_count = 9223372036854775808")
        .expect_err("query should fail");

    assert_eq!(error.kind(), &crate::ParseErrorKind::InvalidIntegerLiteral);
}
