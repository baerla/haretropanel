use std::collections::HashMap;

use askama::Template;

use crate::domain::{DashboardState, Entity, EntityId, EntityKind};

#[derive(Debug)]
pub struct EntityViewModel {
    pub id: String,
    pub name: String,
    pub kind_label: String,
    pub is_on: bool,
    pub has_value: bool,
    pub value: String,
    pub can_toggle: bool,
    pub can_run_script: bool,
}

impl From<&Entity> for EntityViewModel {
    fn from(e: &Entity) -> Self {
        let kind_label = match e.kind {
            EntityKind::Light => "Light",
            EntityKind::Switch => "Switch",
            EntityKind::Sensor => "Sensor",
            EntityKind::Climate => "Climate",
            EntityKind::Script => "Script",
        }
        .to_string();

        let (has_value, value) = if let Some(v) = &e.value {
            (true, v.clone())
        } else {
            (false, String::new())
        };

        let can_toggle = matches!(e.kind, EntityKind::Light | EntityKind::Switch);
        let can_run_script = matches!(e.kind, EntityKind::Script);

        Self {
            id: e.id.to_string(),
            name: e.name.clone(),
            kind_label,
            is_on: e.is_on,
            has_value,
            value,
            can_toggle,
            can_run_script,
        }
    }
}

pub struct DashboardPageViewModel {
    pub entities: Vec<EntityViewModel>,
    pub current_page: usize,
    pub total_pages: usize,
}

impl DashboardPageViewModel {
    pub fn from_state_and_pages(
        state: DashboardState,
        entity_pages: &HashMap<String, usize>,
        requested_page: usize,
    ) -> Self {
        let items: Vec<(usize, EntityViewModel)> = state
            .entities
            .iter()
            .map(|e| {
                let id = e.id.to_string();
                let page = entity_pages.get(&id).cloned().unwrap_or(1).max(1);
                (page, EntityViewModel::from(e))
            })
            .collect();

        let total_pages = items.iter().map(|(p, _)| *p).max().unwrap_or(1);
        let page = requested_page.clamp(1, total_pages);

        let entities = items
            .into_iter()
            .filter(|(p, _)| *p == page)
            .map(|(_, vm)| vm)
            .collect();

        Self {
            entities,
            current_page: page,
            total_pages,
        }
    }
}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate<'a> {
    pub entities: &'a [EntityViewModel],
    pub current_page: usize,
    pub total_pages: usize,
}

#[derive(Debug)]
pub struct EntitySettingsViewModel {
    pub id: String,
    pub name: String,
    pub is_selected: bool,
    pub page: usize,
}

pub struct EntitiesSettingsPageViewModel {
    pub entities: Vec<EntitySettingsViewModel>,
}

#[derive(Template)]
#[template(path = "entities_settings.html")]
pub struct EntitiesSettingsTemplate<'a> {
    pub entities: &'a [EntitySettingsViewModel],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entity(id: &str, name: &str, kind: EntityKind, is_on: bool, value: Option<String>) -> Entity {
        Entity {
            id: EntityId::from(id),
            name: name.to_string(),
            kind,
            is_on,
            value,
        }
    }

    // -- EntityViewModel --

    #[test]
    fn test_viewmodel_from_light_on() {
        let entity = make_entity("light.lamp", "Lamp", EntityKind::Light, true, None);
        let vm = EntityViewModel::from(&entity);
        assert_eq!(vm.id, "light.lamp");
        assert_eq!(vm.name, "Lamp");
        assert_eq!(vm.kind_label, "Light");
        assert!(vm.is_on);
        assert!(!vm.has_value);
        assert_eq!(vm.value, "");
        assert!(vm.can_toggle);
        assert!(!vm.can_run_script);
    }

    #[test]
    fn test_viewmodel_from_switch_off() {
        let entity = make_entity("switch.outdoor", "Outdoor", EntityKind::Switch, false, None);
        let vm = EntityViewModel::from(&entity);
        assert!(!vm.is_on);
        assert!(vm.can_toggle);
        assert!(!vm.can_run_script);
    }

    #[test]
    fn test_viewmodel_from_sensor_with_value() {
        let entity = make_entity("sensor.temp", "Temperature", EntityKind::Sensor, false, Some("22".to_string()));
        let vm = EntityViewModel::from(&entity);
        assert_eq!(vm.kind_label, "Sensor");
        assert!(vm.has_value);
        assert_eq!(vm.value, "22");
        assert!(!vm.can_toggle);
        assert!(!vm.can_run_script);
    }

    #[test]
    fn test_viewmodel_from_climate() {
        let entity = make_entity("climate.home", "Thermostat", EntityKind::Climate, true, None);
        let vm = EntityViewModel::from(&entity);
        assert_eq!(vm.kind_label, "Climate");
        assert!(!vm.can_toggle);
        assert!(!vm.can_run_script);
    }

    #[test]
    fn test_viewmodel_from_script() {
        let entity = make_entity("script.away_mode", "Away Mode", EntityKind::Script, false, None);
        let vm = EntityViewModel::from(&entity);
        assert_eq!(vm.kind_label, "Script");
        assert!(!vm.can_toggle);
        assert!(vm.can_run_script);
    }

    // -- DashboardPageViewModel --

    #[test]
    fn test_page_view_model_all_on_one_page() {
        let state = DashboardState {
            entities: vec![
                make_entity("light.a", "A", EntityKind::Light, true, None),
                make_entity("light.b", "B", EntityKind::Light, false, None),
            ],
        };
        let pages = HashMap::new();
        let vm = DashboardPageViewModel::from_state_and_pages(state, &pages, 1);
        assert_eq!(vm.current_page, 1);
        assert_eq!(vm.total_pages, 1);
        assert_eq!(vm.entities.len(), 2);
    }

    #[test]
    fn test_page_view_model_two_pages() {
        let state = DashboardState {
            entities: vec![
                make_entity("light.a", "A", EntityKind::Light, true, None),
                make_entity("light.b", "B", EntityKind::Light, false, None),
                make_entity("light.c", "C", EntityKind::Light, true, None),
            ],
        };
        let mut pages = HashMap::new();
        pages.insert("light.a".to_string(), 1);
        pages.insert("light.b".to_string(), 2);
        pages.insert("light.c".to_string(), 1);

        let vm = DashboardPageViewModel::from_state_and_pages(state.clone(), &pages, 1);
        assert_eq!(vm.current_page, 1);
        assert_eq!(vm.total_pages, 2);
        assert_eq!(vm.entities.len(), 2); // light.a + light.c

        let vm2 = DashboardPageViewModel::from_state_and_pages(state, &pages, 2);
        assert_eq!(vm2.entities.len(), 1); // light.b only
    }

    #[test]
    fn test_page_view_model_default_page_one() {
        let state = DashboardState {
            entities: vec![
                make_entity("light.a", "A", EntityKind::Light, true, None),
            ],
        };
        let mut pages = HashMap::new();
        pages.insert("light.a".to_string(), 0); // page 0 should clamp to 1
        let vm = DashboardPageViewModel::from_state_and_pages(state, &pages, 1);
        assert_eq!(vm.entities.len(), 1);
    }

    #[test]
    fn test_page_view_model_clamp_page() {
        let state = DashboardState {
            entities: vec![
                make_entity("light.a", "A", EntityKind::Light, true, None),
            ],
        };
        let vm = DashboardPageViewModel::from_state_and_pages(state, &HashMap::new(), 100);
        assert_eq!(vm.current_page, 1);
        assert_eq!(vm.total_pages, 1);
    }

    #[test]
    fn test_page_view_model_empty_entities() {
        let state = DashboardState { entities: vec![] };
        let vm = DashboardPageViewModel::from_state_and_pages(state, &HashMap::new(), 1);
        assert_eq!(vm.entities.len(), 0);
        assert_eq!(vm.current_page, 1);
        assert_eq!(vm.total_pages, 1);
    }

    #[test]
    fn test_page_view_model_page_from_hashmap() {
        let state = DashboardState {
            entities: vec![
                make_entity("light.a", "A", EntityKind::Light, true, None),
                make_entity("light.b", "B", EntityKind::Light, true, None),
            ],
        };
        let mut pages = HashMap::new();
        pages.insert("light.a".to_string(), 1);
        pages.insert("light.b".to_string(), 3);
        let vm = DashboardPageViewModel::from_state_and_pages(state, &pages, 1);
        // light.a is on page 1, light.b is on page 3, so total_pages becomes 3
        assert_eq!(vm.current_page, 1);
        assert_eq!(vm.total_pages, 3);
        assert_eq!(vm.entities.len(), 1); // only light.a
    }

    #[test]
    fn test_page_view_model_no_hashmap_entry_uses_default_1() {
        let state = DashboardState {
            entities: vec![
                make_entity("light.a", "A", EntityKind::Light, true, None),
            ],
        };
        let pages = HashMap::new(); // no entries
        let vm = DashboardPageViewModel::from_state_and_pages(state, &pages, 1);
        assert_eq!(vm.entities.len(), 1);
        assert_eq!(vm.total_pages, 1);
    }

    // -- EntitySettingsViewModel --

    #[test]
    fn test_entity_settings_viewmodel() {
        let entity = make_entity("light.lamp", "Lamp", EntityKind::Light, true, None);
        let vm = EntityViewModel::from(&entity);
        let settings = EntitySettingsViewModel {
            id: vm.id,
            name: vm.name,
            is_selected: true,
            page: 1,
        };
        assert_eq!(settings.id, "light.lamp");
        assert!(settings.is_selected);
    }
}
