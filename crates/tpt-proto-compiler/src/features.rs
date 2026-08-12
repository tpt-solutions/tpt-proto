//! Editions feature resolution (Phase 4 — §4.3, §6, §16).
//!
//! Models the subset of `google.protobuf.FeatureSet` that tpt-proto resolves,
//! provides per-edition defaults, maps proto2/proto3 to an equivalent feature
//! configuration, and resolves features deterministically through the
//! file → message → field/enum inheritance chain.
//!
//! Resolution is deterministic: a child feature set is always produced by
//! merging a fully-resolved base with a set of explicit overrides, so the same
//! schema always yields the same resolved features regardless of evaluation
//! order.

/// How field presence is tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldPresence {
    /// Presence is tracked explicitly (proto2 `optional`/`required`, editions 2024).
    Explicit,
    /// Presence is implicit; singular scalars have no presence (proto3).
    Implicit,
    /// Legacy proto2 semantics: `required` is honored and presence is explicit.
    LegacyRequired,
}

/// Whether enums are open (unknown values allowed) or closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumType {
    /// Unknown values are preserved (proto3, editions 2023).
    Open,
    /// Unknown values are rejected (proto2, editions 2024).
    Closed,
}

/// Encoding for repeated fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatedFieldEncoding {
    /// `[packed = true]` (proto3, editions).
    Packed,
    /// `[packed = false]` (proto2 legacy default).
    Expanded,
}

/// UTF-8 validation strictness for `string` fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Utf8Validation {
    /// Validate UTF-8 on parse.
    Verify,
    /// Do not validate (legacy proto2).
    None,
}

/// JSON mapping conformance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonFormat {
    /// Strict conformance with the proto3 JSON spec.
    Allow,
    /// Legacy best-effort JSON (proto2).
    LegacyBestEffort,
}

/// Wire encoding for messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageEncoding {
    /// Length-prefixed (proto3, editions).
    LengthPrefixed,
    /// Group-delimited (legacy proto2 groups).
    Delimited,
}

/// A fully-resolved feature set for a schema node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureSet {
    /// Field presence behavior.
    pub field_presence: FieldPresence,
    /// Enum open/closed behavior.
    pub enum_type: EnumType,
    /// Repeated field encoding.
    pub repeated_field_encoding: RepeatedFieldEncoding,
    /// UTF-8 validation.
    pub utf8_validation: Utf8Validation,
    /// JSON format conformance.
    pub json_format: JsonFormat,
    /// Message wire encoding.
    pub message_encoding: MessageEncoding,
}

/// Explicit per-node overrides. Each field, when `Some`, overrides the
/// inherited base value during resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeatureOverrides {
    /// Override for [`FeatureSet::field_presence`].
    pub field_presence: Option<FieldPresence>,
    /// Override for [`FeatureSet::enum_type`].
    pub enum_type: Option<EnumType>,
    /// Override for [`FeatureSet::repeated_field_encoding`].
    pub repeated_field_encoding: Option<RepeatedFieldEncoding>,
    /// Override for [`FeatureSet::utf8_validation`].
    pub utf8_validation: Option<Utf8Validation>,
    /// Override for [`FeatureSet::json_format`].
    pub json_format: Option<JsonFormat>,
    /// Override for [`FeatureSet::message_encoding`].
    pub message_encoding: Option<MessageEncoding>,
}

impl FeatureSet {
    /// Defaults for `proto2` syntax.
    pub fn for_proto2() -> FeatureSet {
        FeatureSet {
            field_presence: FieldPresence::LegacyRequired,
            enum_type: EnumType::Closed,
            repeated_field_encoding: RepeatedFieldEncoding::Expanded,
            utf8_validation: Utf8Validation::None,
            json_format: JsonFormat::LegacyBestEffort,
            message_encoding: MessageEncoding::LengthPrefixed,
        }
    }

    /// Defaults for `proto3` syntax.
    pub fn for_proto3() -> FeatureSet {
        FeatureSet {
            field_presence: FieldPresence::Implicit,
            enum_type: EnumType::Open,
            repeated_field_encoding: RepeatedFieldEncoding::Packed,
            utf8_validation: Utf8Validation::Verify,
            json_format: JsonFormat::Allow,
            message_encoding: MessageEncoding::LengthPrefixed,
        }
    }

    /// Defaults for a specific editions `edition` string.
    ///
    /// Unknown editions fall back to the 2023 defaults (proto3-like).
    pub fn for_edition(edition: &str) -> FeatureSet {
        match edition {
            "2024" => FeatureSet {
                field_presence: FieldPresence::Explicit,
                enum_type: EnumType::Closed,
                repeated_field_encoding: RepeatedFieldEncoding::Packed,
                utf8_validation: Utf8Validation::Verify,
                json_format: JsonFormat::Allow,
                message_encoding: MessageEncoding::LengthPrefixed,
            },
            "2023" => FeatureSet {
                field_presence: FieldPresence::Implicit,
                enum_type: EnumType::Open,
                repeated_field_encoding: RepeatedFieldEncoding::Packed,
                utf8_validation: Utf8Validation::Verify,
                json_format: JsonFormat::Allow,
                message_encoding: MessageEncoding::LengthPrefixed,
            },
            _ => FeatureSet::for_edition("2023"),
        }
    }

    /// Resolve the feature set for a syntax keyword (`"proto2"`/`"proto3"`)
    /// or an editions `edition` value.
    pub fn for_syntax(syntax: &str, edition: Option<&str>) -> FeatureSet {
        match (syntax, edition) {
            (_, Some(e)) => FeatureSet::for_edition(e),
            ("proto2", None) => FeatureSet::for_proto2(),
            ("proto3", None) => FeatureSet::for_proto3(),
            // Omitted syntax defaults to proto2.
            _ => FeatureSet::for_proto2(),
        }
    }

    /// Merge explicit overrides on top of a resolved base, producing a new
    /// resolved feature set. This is the deterministic inheritance step.
    pub fn merge(&self, ov: &FeatureOverrides) -> FeatureSet {
        FeatureSet {
            field_presence: ov.field_presence.unwrap_or(self.field_presence),
            enum_type: ov.enum_type.unwrap_or(self.enum_type),
            repeated_field_encoding: ov
                .repeated_field_encoding
                .unwrap_or(self.repeated_field_encoding),
            utf8_validation: ov.utf8_validation.unwrap_or(self.utf8_validation),
            json_format: ov.json_format.unwrap_or(self.json_format),
            message_encoding: ov.message_encoding.unwrap_or(self.message_encoding),
        }
    }

    /// The legacy syntax whose default feature configuration most closely
    /// matches this set, for diagnostics and compatibility mapping.
    pub fn legacy_equivalent(&self) -> &'static str {
        // Editions 2024 (explicit presence, closed enums) is closest to proto2;
        // everything else is closest to proto3.
        if self.field_presence == FieldPresence::Explicit && self.enum_type == EnumType::Closed {
            "proto2"
        } else {
            "proto3"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto2_proto3_differ() {
        assert_eq!(FeatureSet::for_proto2().enum_type, EnumType::Closed);
        assert_eq!(FeatureSet::for_proto3().enum_type, EnumType::Open);
        assert_eq!(
            FeatureSet::for_proto3().field_presence,
            FieldPresence::Implicit
        );
    }

    #[test]
    fn edition_2024_is_proto2_like() {
        let f = FeatureSet::for_edition("2024");
        assert_eq!(f.field_presence, FieldPresence::Explicit);
        assert_eq!(f.enum_type, EnumType::Closed);
        assert_eq!(f.legacy_equivalent(), "proto2");
    }

    #[test]
    fn merge_overrides_base() {
        let base = FeatureSet::for_proto3();
        let mut ov = FeatureOverrides::default();
        ov.field_presence = Some(FieldPresence::Explicit);
        let merged = base.merge(&ov);
        assert_eq!(merged.field_presence, FieldPresence::Explicit);
        // Untouched features fall through from the base deterministically.
        assert_eq!(merged.enum_type, EnumType::Open);
        assert_eq!(
            merged.repeated_field_encoding,
            RepeatedFieldEncoding::Packed
        );
    }

    #[test]
    fn unknown_edition_falls_back() {
        assert_eq!(
            FeatureSet::for_edition("2999"),
            FeatureSet::for_edition("2023")
        );
    }

    #[test]
    fn for_syntax_handles_editions() {
        assert_eq!(
            FeatureSet::for_syntax("proto3", Some("2024")).field_presence,
            FieldPresence::Explicit
        );
    }
}
