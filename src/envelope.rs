//! Hand-written wire codecs for the frozen `weavatrix.edit-plan.v1` envelope.
//!
//! [`TextEdit`], [`FileEdit`], and [`EditPlan`] previously derived serde with
//! `#[serde(flatten)]` extension maps. The derive read declared members
//! directly and buffered only undeclared ones into serde's private `Content`
//! tree before a second `FlatMapDeserializer` pass; these codecs read every
//! member exactly once instead. Measured on its own that is close to a wash
//! (see `docs/decoder-comparison.md`) — the point of writing them by hand is
//! that `flatten` cannot express "decode this envelope and skip the undeclared
//! members", which is where the actual cost lives.
//!
//! The map visitors are therefore generic over an [`ExtensionPolicy`]: the
//! capturing decode and the declared-only decode behind
//! [`DeclaredEditPlan`] share one implementation of field matching, duplicate
//! detection, and missing-field reporting, and cannot drift apart.
//!
//! Every observable behaviour of the derive is preserved: the same accepted
//! documents, the same declared-field-then-`BTreeMap`-order serialized bytes,
//! and the same `duplicate field`, `missing field`, and `invalid type` messages
//! at the same input positions. `tests/envelope_wire.rs` pins this against an
//! independent restatement of the original derives on two serde drivers.

use core::fmt;
use core::marker::PhantomData;
use std::collections::BTreeMap;

use blazingly_json::Value;
use serde::de::{DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer, de};

use crate::{
    model::{Completeness, EditPlan, FileEdit, TextEdit},
    provenance::Provenance,
};

/// Declared member names of a [`TextEdit`], in serialization order.
pub(crate) const TEXT_EDIT_FIELDS: [&str; 7] = [
    "startLine",
    "startChar",
    "endLine",
    "endChar",
    "before",
    "after",
    "provenance",
];

/// Declared member names of a [`FileEdit`], in serialization order.
pub(crate) const FILE_EDIT_FIELDS: [&str; 3] = ["path", "sha256", "edits"];

/// Declared member names of an [`EditPlan`], in serialization order.
pub(crate) const EDIT_PLAN_FIELDS: [&str; 4] =
    ["schemaVersion", "operation", "files", "completeness"];

// ---------------------------------------------------------------------------
// Extension policy
// ---------------------------------------------------------------------------

/// How one decode pass treats members that are not declared by the envelope.
///
/// Both policies match declared members identically, so a declared-only decode
/// accepts and rejects exactly the documents the capturing decode does.
trait ExtensionPolicy: Copy {
    /// Retained key representation for an undeclared member.
    type Key;
    /// Accumulator threaded through one object.
    type Sink;

    fn key(name: &str) -> Self::Key;

    fn sink() -> Self::Sink;

    /// Consumes the pending value of an undeclared member.
    fn absorb<'de, A>(sink: &mut Self::Sink, key: Self::Key, map: &mut A) -> Result<(), A::Error>
    where
        A: MapAccess<'de>;

    fn finish(sink: Self::Sink) -> BTreeMap<String, Value>;
}

/// Retains every undeclared member as an owned JSON value.
#[derive(Clone, Copy)]
struct Capture;

impl ExtensionPolicy for Capture {
    type Key = String;
    type Sink = BTreeMap<String, Value>;

    fn key(name: &str) -> String {
        name.to_owned()
    }

    fn sink() -> Self::Sink {
        BTreeMap::new()
    }

    fn absorb<'de, A>(sink: &mut Self::Sink, key: String, map: &mut A) -> Result<(), A::Error>
    where
        A: MapAccess<'de>,
    {
        // Last duplicate wins, matching the derive's `FlatMapDeserializer` pass.
        sink.insert(key, map.next_value()?);
        Ok(())
    }

    fn finish(sink: Self::Sink) -> BTreeMap<String, Value> {
        sink
    }
}

/// Skips every undeclared member without allocating a key or a value tree.
#[derive(Clone, Copy)]
struct Discard;

impl ExtensionPolicy for Discard {
    type Key = ();
    type Sink = ();

    fn key(_name: &str) {}

    fn sink() -> Self::Sink {}

    fn absorb<'de, A>(_sink: &mut (), _key: (), map: &mut A) -> Result<(), A::Error>
    where
        A: MapAccess<'de>,
    {
        map.next_value::<IgnoredAny>()?;
        Ok(())
    }

    fn finish((): Self::Sink) -> BTreeMap<String, Value> {
        BTreeMap::new()
    }
}

// ---------------------------------------------------------------------------
// Member names
// ---------------------------------------------------------------------------

enum Field<K> {
    /// Index into the declaring struct's field-name table.
    Known(usize),
    Unknown(K),
}

/// Resolves a member name to a declared field index.
///
/// Each implementation is a `match` over string literals, not a scan of a
/// runtime slice, so the compiler lowers it to a length switch plus at most a
/// couple of comparisons -- the same shape the derive generated.
trait FieldTable {
    fn lookup(name: &str) -> Option<usize>;
}

struct TextEditFields;

impl FieldTable for TextEditFields {
    fn lookup(name: &str) -> Option<usize> {
        Some(match name {
            "startLine" => 0,
            "startChar" => 1,
            "endLine" => 2,
            "endChar" => 3,
            "before" => 4,
            "after" => 5,
            "provenance" => 6,
            _ => return None,
        })
    }
}

struct FileEditFields;

impl FieldTable for FileEditFields {
    fn lookup(name: &str) -> Option<usize> {
        Some(match name {
            "path" => 0,
            "sha256" => 1,
            "edits" => 2,
            _ => return None,
        })
    }
}

struct EditPlanFields;

impl FieldTable for EditPlanFields {
    fn lookup(name: &str) -> Option<usize> {
        Some(match name {
            "schemaVersion" => 0,
            "operation" => 1,
            "files" => 2,
            "completeness" => 3,
            _ => return None,
        })
    }
}

struct FieldSeed<P, T>(PhantomData<(P, T)>);

impl<P, T> Clone for FieldSeed<P, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P, T> Copy for FieldSeed<P, T> {}

impl<P, T> FieldSeed<P, T> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<'de, P: ExtensionPolicy, T: FieldTable> DeserializeSeed<'de> for FieldSeed<P, T> {
    type Value = Field<P::Key>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_identifier(self)
    }
}

impl<P: ExtensionPolicy, T: FieldTable> Visitor<'_> for FieldSeed<P, T> {
    type Value = Field<P::Key>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("field identifier")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        // `visit_borrowed_str` and `visit_string` forward here by default, so an
        // escaped spelling of a declared name still binds to that field.
        Ok(match T::lookup(value) {
            Some(index) => Field::Known(index),
            None => Field::Unknown(P::key(value)),
        })
    }
}

// ---------------------------------------------------------------------------
// Sequences of seeded elements
// ---------------------------------------------------------------------------

/// Mirrors `Vec<T>`'s own codec, threading an [`ExtensionPolicy`] into elements.
struct VecSeed<S>(S);

impl<'de, S> DeserializeSeed<'de> for VecSeed<S>
where
    S: DeserializeSeed<'de> + Copy,
{
    type Value = Vec<S::Value>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, S> Visitor<'de> for VecSeed<S>
where
    S: DeserializeSeed<'de> + Copy,
{
    type Value = Vec<S::Value>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a sequence")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::with_capacity(cautious_capacity::<S::Value>(seq.size_hint()));
        while let Some(item) = seq.next_element_seed(self.0)? {
            items.push(item);
        }
        Ok(items)
    }
}

/// Bounds a hinted preallocation the way serde's own sequence codecs do, so a
/// hostile length hint cannot reserve unbounded memory before any element is
/// read.
fn cautious_capacity<T>(hint: Option<usize>) -> usize {
    const MAX_PREALLOCATED_BYTES: usize = 1024 * 1024;
    MAX_PREALLOCATED_BYTES
        .checked_div(size_of::<T>())
        .map_or(0, |ceiling| hint.unwrap_or(0).min(ceiling))
}

// ---------------------------------------------------------------------------
// TextEdit
// ---------------------------------------------------------------------------

struct TextEditSeed<P>(PhantomData<P>);

impl<P> Clone for TextEditSeed<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P> Copy for TextEditSeed<P> {}

impl<P> TextEditSeed<P> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<'de, P: ExtensionPolicy> DeserializeSeed<'de> for TextEditSeed<P> {
    type Value = TextEdit;

    fn deserialize<D>(self, deserializer: D) -> Result<TextEdit, D::Error>
    where
        D: Deserializer<'de>,
    {
        // `deserialize_map`, not `deserialize_struct`: the derive this replaces
        // also used the map entry point, and a driver that accepts a positional
        // sequence for a struct must keep rejecting one here.
        deserializer.deserialize_map(self)
    }
}

impl<'de, P: ExtensionPolicy> Visitor<'de> for TextEditSeed<P> {
    type Value = TextEdit;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("struct TextEdit")
    }

    fn visit_map<A>(self, mut map: A) -> Result<TextEdit, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut start_line: Option<u32> = None;
        let mut start_char: Option<u32> = None;
        let mut end_line: Option<u32> = None;
        let mut end_char: Option<u32> = None;
        let mut before: Option<String> = None;
        let mut after: Option<String> = None;
        let mut provenance: Option<Provenance> = None;
        let mut sink = P::sink();

        while let Some(field) = map.next_key_seed(FieldSeed::<P, TextEditFields>::new())? {
            match field {
                Field::Known(0) => {
                    if start_line.is_some() {
                        return Err(de::Error::duplicate_field("startLine"));
                    }
                    start_line = Some(map.next_value()?);
                }
                Field::Known(1) => {
                    if start_char.is_some() {
                        return Err(de::Error::duplicate_field("startChar"));
                    }
                    start_char = Some(map.next_value()?);
                }
                Field::Known(2) => {
                    if end_line.is_some() {
                        return Err(de::Error::duplicate_field("endLine"));
                    }
                    end_line = Some(map.next_value()?);
                }
                Field::Known(3) => {
                    if end_char.is_some() {
                        return Err(de::Error::duplicate_field("endChar"));
                    }
                    end_char = Some(map.next_value()?);
                }
                Field::Known(4) => {
                    if before.is_some() {
                        return Err(de::Error::duplicate_field("before"));
                    }
                    before = Some(map.next_value()?);
                }
                Field::Known(5) => {
                    if after.is_some() {
                        return Err(de::Error::duplicate_field("after"));
                    }
                    after = Some(map.next_value()?);
                }
                // The final declared index; the table has no further entries.
                Field::Known(_) => {
                    if provenance.is_some() {
                        return Err(de::Error::duplicate_field("provenance"));
                    }
                    provenance = Some(map.next_value()?);
                }
                Field::Unknown(key) => P::absorb(&mut sink, key, &mut map)?,
            }
        }

        Ok(TextEdit {
            start_line: start_line.ok_or_else(|| de::Error::missing_field("startLine"))?,
            start_char: start_char.ok_or_else(|| de::Error::missing_field("startChar"))?,
            end_line: end_line.ok_or_else(|| de::Error::missing_field("endLine"))?,
            end_char: end_char.ok_or_else(|| de::Error::missing_field("endChar"))?,
            before: before.ok_or_else(|| de::Error::missing_field("before"))?,
            after: after.ok_or_else(|| de::Error::missing_field("after"))?,
            provenance: provenance.ok_or_else(|| de::Error::missing_field("provenance"))?,
            extensions: P::finish(sink),
        })
    }
}

impl<'de> Deserialize<'de> for TextEdit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        TextEditSeed::<Capture>::new().deserialize(deserializer)
    }
}

impl Serialize for TextEdit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // A map of unknown length, exactly as the flattened derive emitted, so
        // both compact and pretty output stay byte-identical.
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry(TEXT_EDIT_FIELDS[0], &self.start_line)?;
        map.serialize_entry(TEXT_EDIT_FIELDS[1], &self.start_char)?;
        map.serialize_entry(TEXT_EDIT_FIELDS[2], &self.end_line)?;
        map.serialize_entry(TEXT_EDIT_FIELDS[3], &self.end_char)?;
        map.serialize_entry(TEXT_EDIT_FIELDS[4], &self.before)?;
        map.serialize_entry(TEXT_EDIT_FIELDS[5], &self.after)?;
        map.serialize_entry(TEXT_EDIT_FIELDS[6], &self.provenance)?;
        serialize_extensions(&mut map, &self.extensions)?;
        map.end()
    }
}

// ---------------------------------------------------------------------------
// FileEdit
// ---------------------------------------------------------------------------

struct FileEditSeed<P>(PhantomData<P>);

impl<P> Clone for FileEditSeed<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P> Copy for FileEditSeed<P> {}

impl<P> FileEditSeed<P> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<'de, P: ExtensionPolicy> DeserializeSeed<'de> for FileEditSeed<P> {
    type Value = FileEdit;

    fn deserialize<D>(self, deserializer: D) -> Result<FileEdit, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de, P: ExtensionPolicy> Visitor<'de> for FileEditSeed<P> {
    type Value = FileEdit;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("struct FileEdit")
    }

    fn visit_map<A>(self, mut map: A) -> Result<FileEdit, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut path: Option<String> = None;
        let mut sha256: Option<String> = None;
        let mut edits: Option<Vec<TextEdit>> = None;
        let mut sink = P::sink();

        while let Some(field) = map.next_key_seed(FieldSeed::<P, FileEditFields>::new())? {
            match field {
                Field::Known(0) => {
                    if path.is_some() {
                        return Err(de::Error::duplicate_field("path"));
                    }
                    path = Some(map.next_value()?);
                }
                Field::Known(1) => {
                    if sha256.is_some() {
                        return Err(de::Error::duplicate_field("sha256"));
                    }
                    sha256 = Some(map.next_value()?);
                }
                Field::Known(_) => {
                    if edits.is_some() {
                        return Err(de::Error::duplicate_field("edits"));
                    }
                    edits = Some(map.next_value_seed(VecSeed(TextEditSeed::<P>::new()))?);
                }
                Field::Unknown(key) => P::absorb(&mut sink, key, &mut map)?,
            }
        }

        Ok(FileEdit {
            path: path.ok_or_else(|| de::Error::missing_field("path"))?,
            sha256: sha256.ok_or_else(|| de::Error::missing_field("sha256"))?,
            edits: edits.ok_or_else(|| de::Error::missing_field("edits"))?,
            extensions: P::finish(sink),
        })
    }
}

impl<'de> Deserialize<'de> for FileEdit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        FileEditSeed::<Capture>::new().deserialize(deserializer)
    }
}

impl Serialize for FileEdit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry(FILE_EDIT_FIELDS[0], &self.path)?;
        map.serialize_entry(FILE_EDIT_FIELDS[1], &self.sha256)?;
        map.serialize_entry(FILE_EDIT_FIELDS[2], &self.edits)?;
        serialize_extensions(&mut map, &self.extensions)?;
        map.end()
    }
}

// ---------------------------------------------------------------------------
// EditPlan
// ---------------------------------------------------------------------------

struct EditPlanSeed<P>(PhantomData<P>);

impl<P> Clone for EditPlanSeed<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P> Copy for EditPlanSeed<P> {}

impl<P> EditPlanSeed<P> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<'de, P: ExtensionPolicy> DeserializeSeed<'de> for EditPlanSeed<P> {
    type Value = EditPlan;

    fn deserialize<D>(self, deserializer: D) -> Result<EditPlan, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de, P: ExtensionPolicy> Visitor<'de> for EditPlanSeed<P> {
    type Value = EditPlan;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("struct EditPlan")
    }

    fn visit_map<A>(self, mut map: A) -> Result<EditPlan, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut schema_version: Option<String> = None;
        let mut operation: Option<String> = None;
        let mut files: Option<Vec<FileEdit>> = None;
        // The outer `Option` records presence, so an explicit `null` still
        // counts as a occupied member and a repeat is a duplicate field.
        let mut completeness: Option<Option<Completeness>> = None;
        let mut sink = P::sink();

        while let Some(field) = map.next_key_seed(FieldSeed::<P, EditPlanFields>::new())? {
            match field {
                Field::Known(0) => {
                    if schema_version.is_some() {
                        return Err(de::Error::duplicate_field("schemaVersion"));
                    }
                    schema_version = Some(map.next_value()?);
                }
                Field::Known(1) => {
                    if operation.is_some() {
                        return Err(de::Error::duplicate_field("operation"));
                    }
                    operation = Some(map.next_value()?);
                }
                Field::Known(2) => {
                    if files.is_some() {
                        return Err(de::Error::duplicate_field("files"));
                    }
                    files = Some(map.next_value_seed(VecSeed(FileEditSeed::<P>::new()))?);
                }
                Field::Known(_) => {
                    if completeness.is_some() {
                        return Err(de::Error::duplicate_field("completeness"));
                    }
                    completeness = Some(map.next_value()?);
                }
                Field::Unknown(key) => P::absorb(&mut sink, key, &mut map)?,
            }
        }

        Ok(EditPlan {
            schema_version: schema_version
                .ok_or_else(|| de::Error::missing_field("schemaVersion"))?,
            operation: operation.ok_or_else(|| de::Error::missing_field("operation"))?,
            files: files.ok_or_else(|| de::Error::missing_field("files"))?,
            completeness: completeness.unwrap_or_default(),
            extensions: P::finish(sink),
        })
    }
}

impl<'de> Deserialize<'de> for EditPlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        EditPlanSeed::<Capture>::new().deserialize(deserializer)
    }
}

impl Serialize for EditPlan {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry(EDIT_PLAN_FIELDS[0], &self.schema_version)?;
        map.serialize_entry(EDIT_PLAN_FIELDS[1], &self.operation)?;
        map.serialize_entry(EDIT_PLAN_FIELDS[2], &self.files)?;
        if self.completeness.is_some() {
            map.serialize_entry(EDIT_PLAN_FIELDS[3], &self.completeness)?;
        }
        serialize_extensions(&mut map, &self.extensions)?;
        map.end()
    }
}

/// Emits extension members in `BTreeMap` order, after the declared members, so
/// an extension key that shadows a declared name produces the duplicate JSON
/// key the flattened derive produced.
fn serialize_extensions<M>(
    map: &mut M,
    extensions: &BTreeMap<String, Value>,
) -> Result<(), M::Error>
where
    M: SerializeMap,
{
    for (key, value) in extensions {
        map.serialize_entry(key, value)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Declared-only decode
// ---------------------------------------------------------------------------

/// An [`EditPlan`] decoded without materializing any extension member.
///
/// Decoding an envelope through [`EditPlan`] retains every undeclared member as
/// an owned JSON value at all three levels. That is required to round-trip a
/// plan, and it is the dominant cost of decoding a large multi-file plan. A
/// consumer that only validates or applies a plan never reads those values and
/// should not pay for them.
///
/// This wrapper accepts and rejects exactly the documents [`EditPlan`] accepts
/// and rejects, with the same error messages, but skips undeclared members
/// structurally instead of building a key and a value tree for each one. The
/// [`EditPlan`] it yields has empty `extensions` at every level.
///
/// # Extensions are dropped, not hidden
///
/// The recovered plan is **not** round-trippable: re-serializing it emits only
/// declared members. Decode through [`EditPlan`] whenever the extension members
/// must survive. Validation is unaffected either way â€” a reserved member name
/// can never reach an extension map through the wire, because a JSON member
/// spelled like a declared field always binds to that field.
///
/// # Examples
///
/// ```
/// use weavatrix_edit::DeclaredEditPlan;
///
/// let json = r#"{
///     "schemaVersion": "weavatrix.edit-plan.v1",
///     "operation": "rename_symbol",
///     "createdAt": "2026-08-01T12:00:00Z",
///     "files": [{
///         "path": "src/user.ts",
///         "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
///         "language": "typescript",
///         "edits": [{
///             "startLine": 10, "startChar": 8, "endLine": 10, "endChar": 15,
///             "before": "getUser", "after": "getCustomer", "provenance": "EXACT_LSP"
///         }]
///     }]
/// }"#;
///
/// let declared: DeclaredEditPlan = blazingly_json::from_str(json)?;
/// let plan = declared.into_plan();
/// assert!(plan.validate().is_ok());
/// assert!(plan.extensions.is_empty());
/// assert!(plan.files[0].extensions.is_empty());
/// # Ok::<(), blazingly_json::Error>(())
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct DeclaredEditPlan(EditPlan);

impl DeclaredEditPlan {
    /// Borrows the recovered plan.
    #[must_use]
    pub const fn plan(&self) -> &EditPlan {
        &self.0
    }

    /// Takes the recovered plan, whose extension maps are all empty.
    #[must_use]
    pub fn into_plan(self) -> EditPlan {
        self.0
    }
}

impl AsRef<EditPlan> for DeclaredEditPlan {
    fn as_ref(&self) -> &EditPlan {
        &self.0
    }
}

impl From<DeclaredEditPlan> for EditPlan {
    fn from(declared: DeclaredEditPlan) -> Self {
        declared.0
    }
}

impl<'de> Deserialize<'de> for DeclaredEditPlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        EditPlanSeed::<Discard>::new()
            .deserialize(deserializer)
            .map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::{EDIT_PLAN_FIELDS, FILE_EDIT_FIELDS, TEXT_EDIT_FIELDS};
    use crate::validation::FILE_EDIT_RESERVED_EXTENSION_KEYS;

    #[test]
    fn reserved_extension_keys_track_the_wire_field_names() {
        // Validation rejects an extension key that shadows a declared member.
        // That list must be the wire field list, or a future field rename would
        // silently open a collision.
        assert_eq!(FILE_EDIT_RESERVED_EXTENSION_KEYS, FILE_EDIT_FIELDS);
        assert_eq!(FILE_EDIT_FIELDS.len(), 3);
        assert_eq!(TEXT_EDIT_FIELDS.len(), 7);
        assert_eq!(EDIT_PLAN_FIELDS.len(), 4);
    }
}
