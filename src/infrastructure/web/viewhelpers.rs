//! Shared helpers for building view model JSON across handlers.
//! This module contains pure functions that operate on domain types
//! and should not have any I/O or side effects.

use crate::domain::{DashboardState, Entity};

/// Builds a garage door status JSON value for a given entity.
///
/// Returns a JSON object with:
/// - `status`: "Offen" (open) or "Geschlossen" (closed)
/// - `action`: "Schließen" (close) or "Öffnen" (open)
/// - `button_class`: "garage-btn garage-open" or "garage-btn garage-closed"
///
/// Returns a closed-door default if the entity is not found.
pub fn build_garage_door(
    entity_id: &str,
    default_name: &str,
    state: &DashboardState,
) -> serde_json::Value {
    state
        .entities
        .iter()
        .find(|e| e.id.0 == entity_id)
        .map(|e| {
            let is_open = e.is_on;
            let name = e.name.clone();
            let status = if is_open { "Offen" } else { "Geschlossen" };
            let action = if is_open { "Schließen" } else { "Öffnen" };
            let button_class = if is_open {
                "garage-btn garage-open"
            } else {
                "garage-btn garage-closed"
            };
            serde_json::json!({
                "status": status,
                "action": action,
                "button_class": button_class,
                "name": name,
            })
        })
        .unwrap_or_else(|| {
            serde_json::json!({
                "status": "Geschlossen",
                "action": "Öffnen",
                "button_class": "garage-btn garage-closed",
                "name": default_name,
            })
        })
}

/// Builds a GarageDoorViewModel for a given entity option.
///
/// Used by handlers that need to construct view model structs (not JSON).
///
/// The name is cleaned by stripping a leading "Garage" prefix and trailing "Offen" suffix,
/// since HA entity names like "Garage Links Offen" should display as "Links".
pub fn make_garage_door_vm(
    entity: Option<&Entity>,
    entity_id: &str,
    default_name: &str,
) -> crate::infrastructure::web::viewmodels::GarageDoorViewModel {
    let mut name = entity
        .map(|e| e.name.clone())
        .unwrap_or_else(|| default_name.to_string());
    name = name.strip_prefix("Garage").map(|s| s.trim().to_string()).unwrap_or(name);
    name = name.strip_suffix("Offen").map(|s| s.trim().to_string()).unwrap_or(name);
    name = name.trim().to_string();

    let is_open = entity.map(|e| e.is_on).unwrap_or(false);
    let status_label = if is_open { "Offen" } else { "Geschlossen" };
    let action_label = if is_open { "Schließen" } else { "Öffnen" };
    let button_class = if is_open {
        "garage-btn garage-open"
    } else {
        "garage-btn garage-closed"
    };

    crate::infrastructure::web::viewmodels::GarageDoorViewModel {
        id: entity_id.to_string(),
        name,
        status_label: status_label.to_string(),
        action_label: action_label.to_string(),
        button_class: button_class.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Entity, EntityId};

    fn make_state(has_entity: bool) -> DashboardState {
        let entities = if has_entity {
            vec![Entity {
                id: EntityId("sensor.garage_left".into()),
                name: "Meine Garage".into(),
                kind: crate::domain::EntityKind::Switch,
                is_on: true,
                value: Some("open".into()),
            }]
        } else {
            Vec::new()
        };
        DashboardState { entities }
    }

    #[test]
    fn test_garage_door_open() {
        let state = make_state(true);
        let door = build_garage_door("sensor.garage_left", "Default", &state);
        assert_eq!(door["status"], "Offen");
        assert_eq!(door["action"], "Schließen");
        assert_eq!(door["button_class"], "garage-btn garage-open");
        assert_eq!(door["name"], "Meine Garage");
    }

    #[test]
    fn test_garage_door_closed() {
        let entities = vec![Entity {
            id: EntityId("sensor.garage_left".into()),
            name: "Meine Garage".into(),
            kind: crate::domain::EntityKind::Switch,
            is_on: false,
            value: Some("closed".into()),
        }];
        let state = DashboardState { entities };
        let door = build_garage_door("sensor.garage_left", "Default", &state);
        assert_eq!(door["status"], "Geschlossen");
        assert_eq!(door["action"], "Öffnen");
        assert_eq!(door["button_class"], "garage-btn garage-closed");
        assert_eq!(door["name"], "Meine Garage");
    }

    #[test]
    fn test_garage_door_missing_entity() {
        let state = make_state(false);
        let door = build_garage_door("sensor.nonexistent", "Left Garage", &state);
        assert_eq!(door["status"], "Geschlossen");
        assert_eq!(door["action"], "Öffnen");
        assert_eq!(door["button_class"], "garage-btn garage-closed");
        assert_eq!(door["name"], "Left Garage");
    }

    #[test]
    fn test_garage_door_entity_not_found() {
        let state = make_state(true); // has garage_left, not garage_right
        let door = build_garage_door("sensor.garage_right", "Right Garage", &state);
        assert_eq!(door["status"], "Geschlossen");
        assert_eq!(door["name"], "Right Garage");
    }

    #[test]
    fn test_make_garage_door_vm_open() {
        let entity = Some(&Entity {
            id: EntityId("sensor.garage_left".into()),
            name: "Meine Garage".into(),
            kind: crate::domain::EntityKind::Switch,
            is_on: true,
            value: Some("open".into()),
        });
        let vm = make_garage_door_vm(entity, "sensor.garage_left_action", "Garage Left");
        assert_eq!(vm.status_label, "Offen");
        assert_eq!(vm.action_label, "Schließen");
        assert_eq!(vm.button_class, "garage-btn garage-open");
        assert_eq!(vm.name, "Meine Garage");
    }

    #[test]
    fn test_make_garage_door_vm_closed() {
        let entity = Some(&Entity {
            id: EntityId("sensor.garage_left".into()),
            name: "Garage Links Offen".into(),
            kind: crate::domain::EntityKind::Switch,
            is_on: false,
            value: Some("closed".into()),
        });
        let vm = make_garage_door_vm(entity, "sensor.garage_left_action", "Garage Left");
        assert_eq!(vm.status_label, "Geschlossen");
        assert_eq!(vm.action_label, "Öffnen");
        // strip_prefix("Garage") then strip_suffix("Offen")
        assert_eq!(vm.name, "Links");
    }

    #[test]
    fn test_make_garage_door_vm_none() {
        let vm = make_garage_door_vm(None, "sensor.nonexistent", "Default Garage");
        assert_eq!(vm.status_label, "Geschlossen");
        assert_eq!(vm.action_label, "Öffnen");
        assert_eq!(vm.button_class, "garage-btn garage-closed");
        assert_eq!(vm.name, "Default Garage");
    }
}
