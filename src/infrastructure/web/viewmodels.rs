use askama::Template;

use crate::domain::{Entity, EntityKind, DashboardState};
use crate::domain::value_objects::EntityId;

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
    pub can_toggle_cover: bool,
}

impl From<&Entity> for EntityViewModel {
    fn from(e: &Entity) -> Self {
        let kind_label = match e.kind {
            EntityKind::Light => "Light",
            EntityKind::Switch => "Switch",
            EntityKind::Sensor => "Sensor",
            EntityKind::Climate => "Climate",
            EntityKind::Script => "Script",
            EntityKind::Cover => "Cover",
        }
        .to_string();

        let (has_value, value) = if let Some(v) = &e.value {
            (true, v.clone())
        } else {
            (false, String::new())
        };

        let can_toggle = matches!(e.kind, EntityKind::Light | EntityKind::Switch);
        let can_run_script = matches!(e.kind, EntityKind::Script);
        let can_toggle_cover = matches!(e.kind, EntityKind::Cover);

        Self {
            id: e.id.to_string(),
            name: e.name.clone(),
            kind_label,
            is_on: e.is_on,
            has_value,
            value,
            can_toggle,
            can_run_script,
            can_toggle_cover,
        }
    }
}

pub struct SolarViewModel {
    pub watts_label: String,
    pub max_watts_label: String,
    pub chart_labels_js: String,
    pub chart_values_js: String,
}

pub struct ChargerViewModel {
    pub amps_label: String,
    pub status_label: String,
    pub car_state_label: String,
    pub car_state_class: String,
    pub car_connected: bool,
    pub car_charging: bool,
    pub pill_paused: bool,
}

pub struct GarageDoorViewModel {
    pub id: String,
    pub name: String,
    pub status_label: String,
    pub action_label: String,
    pub button_class: String,
}

pub struct DashboardViewModel {
    pub solar: SolarViewModel,
    pub charger: ChargerViewModel,
    pub garage_left: GarageDoorViewModel,
    pub garage_right: GarageDoorViewModel,
    pub demo_mode: bool,
    pub last_updated: String,
    pub page_tabs: Vec<String>,
    pub active_tab: usize,
    pub page_entities: Vec<EntityViewModel>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate<'a> {
    pub dashboard: &'a DashboardViewModel,
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
