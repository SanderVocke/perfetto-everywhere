use core::fmt;

/// Stable identifier for metadata transported separately from event records.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct MetadataId(pub u32);

impl MetadataId {
    /// Constructs a deterministic ID from a namespace tag and UTF-8 label.
    pub const fn for_label(namespace: u8, label: &str) -> Self {
        let bytes = label.as_bytes();
        let mut hash = 0x811c_9dc5_u32 ^ namespace as u32;
        let mut index = 0;
        while index < bytes.len() {
            hash ^= bytes[index] as u32;
            hash = hash.wrapping_mul(0x0100_0193);
            index += 1;
        }
        // Zero is reserved for "not specified" on the wire.
        Self(if hash == 0 { 1 } else { hash })
    }
}

/// A statically registered event, span, log-message, counter, or track name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaticName {
    pub id: MetadataId,
    pub label: &'static str,
}

impl StaticName {
    pub const fn new(label: &'static str) -> Self {
        Self {
            id: MetadataId::for_label(1, label),
            label,
        }
    }
}

/// A statically registered category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Category {
    pub id: MetadataId,
    pub label: &'static str,
}

impl Category {
    pub const fn new(label: &'static str) -> Self {
        Self {
            id: MetadataId::for_label(2, label),
            label,
        }
    }
}

/// A statically registered structured-field name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldName {
    pub id: MetadataId,
    pub label: &'static str,
}

impl FieldName {
    pub const fn new(label: &'static str) -> Self {
        Self {
            id: MetadataId::for_label(3, label),
            label,
        }
    }
}

/// A metadata definition supplied during backend initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataDef {
    pub id: MetadataId,
    pub namespace: u8,
    pub label: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataCollision {
    pub id: MetadataId,
    pub first: &'static str,
    pub second: &'static str,
}

impl fmt::Display for MetadataCollision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "metadata ID {:#010x} maps to both {:?} and {:?}",
            self.id.0, self.first, self.second
        )
    }
}

/// Rejects equal IDs with different namespaces or labels without allocating.
pub fn validate_metadata(definitions: &[MetadataDef]) -> Result<(), MetadataCollision> {
    for (index, first) in definitions.iter().enumerate() {
        for second in &definitions[index + 1..] {
            if first.id == second.id
                && (first.namespace != second.namespace || first.label != second.label)
            {
                return Err(MetadataCollision {
                    id: first.id,
                    first: first.label,
                    second: second.label,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_ids_are_deterministic_and_namespaced() {
        assert_eq!(StaticName::new("render"), StaticName::new("render"));
        assert_ne!(StaticName::new("render").id, Category::new("render").id);
        assert_ne!(StaticName::new("render").id.0, 0);
    }

    #[test]
    fn collision_validation_distinguishes_aliases_from_conflicts() {
        let id = MetadataId(42);
        assert!(
            validate_metadata(&[
                MetadataDef {
                    id,
                    namespace: 1,
                    label: "same",
                },
                MetadataDef {
                    id,
                    namespace: 1,
                    label: "same",
                },
            ])
            .is_ok()
        );
        assert!(
            validate_metadata(&[
                MetadataDef {
                    id,
                    namespace: 1,
                    label: "first",
                },
                MetadataDef {
                    id,
                    namespace: 1,
                    label: "second",
                },
            ])
            .is_err()
        );
    }
}
