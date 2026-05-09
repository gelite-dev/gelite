use crate::{ResolvedPath, ResolvedPathStep, ResolvedShape, ResolvedShapeField, ValueExpr};
use alloc::vec;
use schema::{Cardinality, FieldId, FieldRef, ObjectTypeId, ObjectTypeRef};

pub fn post_type() -> ObjectTypeRef {
    ObjectTypeRef::new(ObjectTypeId::new(1), "Post")
}

pub fn user_type() -> ObjectTypeRef {
    ObjectTypeRef::new(ObjectTypeId::new(2), "User")
}

pub fn post_title_field() -> FieldRef {
    FieldRef::new(FieldId::new(1), post_type(), "title")
}

pub fn post_author_field() -> FieldRef {
    FieldRef::new(FieldId::new(2), post_type(), "author")
}

pub fn post_subtitle_field() -> FieldRef {
    FieldRef::new(FieldId::new(3), post_type(), "subtitle")
}

pub fn user_name_field() -> FieldRef {
    FieldRef::new(FieldId::new(4), user_type(), "name")
}

pub fn empty_post_shape() -> ResolvedShape {
    ResolvedShape::new(post_type(), vec![])
}

pub fn post_title_path() -> ResolvedPath {
    ResolvedPath::try_new(
        post_type(),
        vec![ResolvedPathStep::scalar(
            post_title_field(),
            Cardinality::Required,
        )],
    )
    .expect("post title path should be valid")
}

pub fn post_subtitle_path() -> ResolvedPath {
    ResolvedPath::try_new(
        post_type(),
        vec![ResolvedPathStep::scalar(
            post_subtitle_field(),
            Cardinality::Optional,
        )],
    )
    .expect("post subtitle path should be valid")
}

pub fn post_title_path_value() -> ValueExpr {
    ValueExpr::Path(post_title_path())
}

pub fn post_subtitle_path_value() -> ValueExpr {
    ValueExpr::Path(post_subtitle_path())
}

pub fn user_name_shape() -> ResolvedShape {
    let shape_field =
        ResolvedShapeField::new("name", user_name_field(), Cardinality::Required, None);

    ResolvedShape::new(user_type(), vec![shape_field])
}
