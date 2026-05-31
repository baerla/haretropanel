use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub String);

impl From<&str> for EntityId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_id_from_str() {
        let id = EntityId::from("light.lamp");
        assert_eq!(id.0, "light.lamp");
    }

    #[test]
    fn test_entity_id_display() {
        let id = EntityId::from("switch.outdoor");
        assert_eq!(format!("{}", id), "switch.outdoor");
    }

    #[test]
    fn test_entity_id_eq_and_hash() {
        let id1 = EntityId::from("light.a");
        let id2 = EntityId::from("light.a");
        let id3 = EntityId::from("light.b");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);

        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(id1);
        set.insert(id2);
        set.insert(id3);
        assert_eq!(set.len(), 2);
    }
}
