// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Declarative macros shared across embassy port crates.

/// Generate a validated newtype identifier for an embassy port.
///
/// # Syntax
///
/// ```text
/// simple_id!(
///     $(#[$attr:meta])*
///     $TypeName:ident, $ErrorType:path,
///     |$s:ident: &str| $validate_body:block
/// );
/// ```
///
/// The `$validate_body` block receives the already-trimmed, non-empty raw
/// string as `$s` and must return `Result<String, $ErrorType>`.  Return
/// `Ok(the_canonical_form)` on success or `Err(…)` on failure.  The macro
/// handles the empty-string guard before invoking the closure.
///
/// # Generated items
///
/// - `pub struct $TypeName(String)` with `#[serde(transparent)]` and the
///   standard derives.
/// - `impl $TypeName { pub fn parse(raw: impl AsRef<str>) -> Result<Self, $ErrorType> }`
/// - `impl $TypeName { #[must_use] pub fn as_str(&self) -> &str }`
/// - `impl std::fmt::Display for $TypeName`
///
/// # Examples
///
/// ```rust,ignore
/// use embassy_pack::simple_id;
/// use crate::error::PubmedError;
///
/// simple_id!(
///     /// PubMed article identifier (one or more ASCII digits).
///     Pmid, PubmedError,
///     |s: &str| {
///         if !s.chars().all(|c| c.is_ascii_digit()) {
///             return Err(PubmedError::InvalidIdentifier(
///                 "invalid Pmid: must contain only ASCII digits".into(),
///             ));
///         }
///         Ok(s.to_string())
///     }
/// );
/// ```
#[macro_export]
macro_rules! simple_id {
    (
        $(#[$attr:meta])*
        $name:ident, $err:path,
        |$s:ident: &str| $body:block
    ) => {
        $(#[$attr])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn parse(raw: impl AsRef<str>) -> Result<Self, $err> {
                let $s = raw.as_ref().trim();
                if $s.is_empty() {
                    return Err(<$err>::InvalidIdentifier("empty".into()));
                }
                let canonical: Result<String, $err> = (|$s: &str| $body)($s);
                canonical.map(Self)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}
