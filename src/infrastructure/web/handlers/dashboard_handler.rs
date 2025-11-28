use askama::Template;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect},
    Form,
};
use serde::Deserialize;
use tracing::info;

use crate::{
    domain::EntityId,
    infrastructure::web::{
        viewmodels::{DashboardPageViewModel, DashboardTemplate},
        AppState,
    },
    shared::error::AppResult,
};

#[derive(Debug, Deserialize)]
pub struct DashboardQuery {
    pub page: Option<usize>,
    pub force_refresh: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ToggleForm {
    pub entity_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RunScriptForm {
    pub entity_id: String,
}

pub async fn get_dashboard(
    State(state): State<AppState>,
    Query(query): Query<DashboardQuery>,
) -> AppResult<impl IntoResponse> {
    let requested_page = query.page.unwrap_or(1);
    let force_refresh = query.force_refresh.is_some();

    let dashboard_state = state
        .dashboard_service
        .get_dashboard_with_refresh(force_refresh)
        .await?;
    let entity_pages = state.dashboard_service.get_entity_pages().await?;

    let vm =
        DashboardPageViewModel::from_state_and_pages(dashboard_state, &entity_pages, requested_page);

    let template = DashboardTemplate {
        entities: &vm.entities,
        current_page: vm.current_page,
        total_pages: vm.total_pages,
    };

    let rendered = template.render()?;
    Ok(Html(rendered))
}

pub async fn post_toggle(
    State(state): State<AppState>,
    Form(form): Form<ToggleForm>,
) -> AppResult<impl IntoResponse> {
    let id = EntityId(form.entity_id);
    info!("Toggling entity via POST /toggle: {}", id);

    state.dashboard_service.toggle_entity(&id).await?;

    Ok(Redirect::to("/"))
}

pub async fn post_run_script(
    State(state): State<AppState>,
    Form(form): Form<RunScriptForm>,
) -> AppResult<impl IntoResponse> {
    let id = EntityId(form.entity_id);
    info!("Running script via POST /run_script: {}", id);

    state.dashboard_service.run_script(&id).await?;

    Ok(Redirect::to("/"))
}

pub async fn get_redirect_to_root() -> impl IntoResponse {
    Redirect::to("/")
}