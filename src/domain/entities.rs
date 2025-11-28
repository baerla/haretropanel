use serde::{Deserialize, Serialize};

use super::value_objects::EntityId;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EntityKind {
    Light,
    Switch,
    Sensor,
    Climate,
    Script,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub name: String,
    pub kind: EntityKind,
    pub is_on: bool,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DashboardState {
    pub entities: Vec<Entity>,
}
