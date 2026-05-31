use askama::Template;

use crate::domain::{Entity, EntityKind};

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
    pub percent: u8,
    pub max_watts_label: String,
    pub chart_labels_js: String,
    pub chart_values_js: String,
}

pub struct ChargerViewModel {
    pub amps_label: String,
    pub status_label: String,
    pub car_state_label: String,
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
