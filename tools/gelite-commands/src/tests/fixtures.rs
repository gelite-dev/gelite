pub fn blog_schema_source() -> &'static str {
    "type User {
  required email: str
}

type Post {
  required title: str
  required link author: User
}"
}
