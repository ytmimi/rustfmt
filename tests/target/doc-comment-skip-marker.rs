// rustfmt-wrap_comments: true
// rustfmt-format_code_in_doc_comments: true

/// # Header
/// Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod
/// tempor incididunt ut labore et dolore magna aliqua.
///
/// ```rust
/// fn main() {}
/// ```
fn format_me() {}

/// # Header
/// Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
///
/// ```rust
/// fn    main() {}
/// ```
/// <!--- rustfmt::skip --->
fn dont_format_me() {}
