use serde::{Deserialize, Serialize};

use super::value_objects::EntityId;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntityKind {
    Light,
    Switch,
    Sensor,
    Climate,
    Script,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entity {
    pub id: EntityId,
    pub name: String,
    pub kind: EntityKind,
    pub is_on: bool,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardState {
    pub entities: Vec<Entity>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_kind_serialization() {
        let kinds = [
            EntityKind::Light,
            EntityKind::Switch,
            EntityKind::Sensor,
            EntityKind::Climate,
            EntityKind::Script,
        ];
        for kind in &kinds {
            let json = serde_json::to_string(kind).unwrap();
            let deserialized: EntityKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, &deserialized);
        }
    }

    #[test]
    fn test_entity_serialization_roundtrip() {
        let entity = Entity {
            id: EntityId::from("light.lamp"),
            name: "My Lamp".to_string(),
            kind: EntityKind::Light,
            is_on: true,
            value: None,
        };
        let json = serde_json::to_string(&entity).unwrap();
        let deserialized: Entity = serde_json::from_str(&json).unwrap();
        assert_eq!(entity.id.0, deserialized.id.0);
        assert_eq!(entity.name, deserialized.name);
        assert_eq!(entity.kind, deserialized.kind);
        assert_eq!(entity.is_on, deserialized.is_on);
        assert_eq!(entity.value, deserialized.value);
    }

    #[test]
    fn test_dashboard_state_serialization() {
        let state = DashboardState {
            entities: vec![
                Entity {
                    id: EntityId::from("light.lamp"),
                    name: "Lamp".to_string(),
                    kind: EntityKind::Light,
                    is_on: true,
                    value: None,
                },
                Entity {
                    id: EntityId::from("sensor.temp"),
                    name: "Temperature".to_string(),
                    kind: EntityKind::Sensor,
                    is_on: false,
                    value: Some("22".to_string()),
                },
            ],
        };
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: DashboardState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.entities.len(), deserialized.entities.len());
    }

    #[test]
    fn test_entity_kind_to_json_string() {
        assert_eq!(serde_json::to_string(&EntityKind::Light).unwrap(), "\"Light\"");
        assert_eq!(serde_json::to_string(&EntityKind::Switch).unwrap(), "\"Switch\"");
        assert_eq!(serde_json::to_string(&EntityKind::Sensor).unwrap(), "\"Sensor\"");
        assert_eq!(serde_json::to_string(&EntityKind::Climate).unwrap(), "\"Climate\"");
        assert_eq!(serde_json::to_string(&EntityKind::Script).unwrap(), "\"Script\"");
    }
}
