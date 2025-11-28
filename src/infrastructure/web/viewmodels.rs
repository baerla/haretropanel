use std::collections::HashMap;

use askama::Template;

use crate::domain::{DashboardState, Entity, EntityKind};

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
