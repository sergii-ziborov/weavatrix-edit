use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Evidence tier attached to an exact source edit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Provenance(ProvenanceValue);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum ProvenanceValue {
    ExactLsp,
    Resolved,
    Extracted,
    LexicalExact,
    // The extra indirection keeps every applicable value allocation-free and
    // the enum smaller than `String`; unknown wire values are rejected later.
    #[allow(clippy::box_collection)]
    Other(Box<String>),
}

impl Provenance {
    pub const EXACT_LSP: &'static str = "EXACT_LSP";
    pub const RESOLVED: &'static str = "RESOLVED";
    pub const EXTRACTED: &'static str = "EXTRACTED";
    pub const LEXICAL_EXACT: &'static str = "LEXICAL_EXACT";

    #[must_use]
    pub fn new(value: impl AsRef<str>) -> Self {
        let value = value.as_ref();
        Self(
            known_value(value)
                .unwrap_or_else(|| ProvenanceValue::Other(Box::new(value.to_owned()))),
        )
    }

    fn from_owned(value: String) -> Self {
        Self(known_value(&value).unwrap_or_else(|| ProvenanceValue::Other(Box::new(value))))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match &self.0 {
            ProvenanceValue::ExactLsp => Self::EXACT_LSP,
            ProvenanceValue::Resolved => Self::RESOLVED,
            ProvenanceValue::Extracted => Self::EXTRACTED,
            ProvenanceValue::LexicalExact => Self::LEXICAL_EXACT,
            ProvenanceValue::Other(value) => value,
        }
    }

    #[must_use]
    pub const fn is_applicable(&self) -> bool {
        !matches!(self.0, ProvenanceValue::Other(_))
    }
}

fn known_value(value: &str) -> Option<ProvenanceValue> {
    match value {
        Provenance::EXACT_LSP => Some(ProvenanceValue::ExactLsp),
        Provenance::RESOLVED => Some(ProvenanceValue::Resolved),
        Provenance::EXTRACTED => Some(ProvenanceValue::Extracted),
        Provenance::LEXICAL_EXACT => Some(ProvenanceValue::LexicalExact),
        _ => None,
    }
}

impl Serialize for Provenance {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Provenance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::Provenance;

    #[test]
    fn known_values_are_compact_and_unknown_values_roundtrip() {
        assert!(core::mem::size_of::<Provenance>() < core::mem::size_of::<String>());
        assert!(Provenance::new(Provenance::EXACT_LSP).is_applicable());

        let unknown = Provenance::new("FUTURE_TIER");
        assert!(!unknown.is_applicable());
        let encoded = blazingly_json::to_string(&unknown).unwrap();
        assert_eq!(encoded, r#""FUTURE_TIER""#);
        assert_eq!(
            blazingly_json::from_str::<Provenance>(&encoded).unwrap(),
            unknown
        );
    }
}
