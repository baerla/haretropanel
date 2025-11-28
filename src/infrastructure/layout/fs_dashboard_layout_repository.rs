use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::{
    application::DashboardLayoutRepository,
    domain::EntityId,
    shared::error::{AppError, AppResult},
};

pub struct FsDashboardLayoutRepository {
    path: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LayoutJson {
    /// List of entity ids that should be visible on the dashboard.
    visible: Vec<String>,
    /// Optional per-entity page assignment (1-based page index).
    #[serde(default)]
    entity_pages: HashMap<String, usize>,
}

impl FsDashboardLayoutRepository {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    async fn ensure_parent_dir(&self) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to create layout dir: {e}")))?;
        }
        Ok(())
    }

    async fn load_raw(&self) -> AppResult<LayoutJson> {
        use std::io::ErrorKind;

        let data = match fs::read(&self.path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                // No file yet, return default layout.
                return Ok(LayoutJson::default());
            }
            Err(e) => {
                return Err(AppError::Internal(format!(
                    "Failed to read layout file: {e}"
                )))
            }
        };

        // First try the new LayoutJson format.
        if let Ok(layout) = serde_json::from_slice::<LayoutJson>(&data) {
            return Ok(layout);
        }

        // Then try legacy Vec<EntityId> format.
        if let Ok(ids) = serde_json::from_slice::<Vec<EntityId>>(&data) {
            let visible = ids.into_iter().map(|id| id.0).collect();
            return Ok(LayoutJson {
                visible,
                entity_pages: HashMap::new(),
            });
        }

        // Finally try legacy Vec<String> format.
        if let Ok(ids) = serde_json::from_slice::<Vec<String>>(&data) {
            return Ok(LayoutJson {
                visible: ids,
                entity_pages: HashMap::new(),
            });
        }

        Err(AppError::Internal(
            "Failed to parse dashboard layout JSON".into(),
        ))
    }

    async fn save_raw(&self, layout: &LayoutJson) -> AppResult<()> {
        self.ensure_parent_dir().await?;

        let json = serde_json::to_string_pretty(layout)
            .map_err(|e| AppError::Internal(format!("Failed to serialize layout JSON: {e}")))?;

        fs::write(&self.path, json)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to write layout file: {e}")))?;

        Ok(())
    }
}

#[async_trait]
impl DashboardLayoutRepository for FsDashboardLayoutRepository {
    async fn load_visible_entities(&self) -> AppResult<Vec<EntityId>> {
        let layout = self.load_raw().await?;
        Ok(layout.visible.into_iter().map(EntityId).collect())
    }

    async fn save_visible_entities(&self, ids: Vec<EntityId>) -> AppResult<()> {
        let mut layout = self.load_raw().await?;
        layout.visible = ids.into_iter().map(|id| id.0).collect();
        self.save_raw(&layout).await
    }

    async fn load_entity_pages(&self) -> AppResult<HashMap<String, usize>> {
        let layout = self.load_raw().await?;
        Ok(layout.entity_pages)
    }

    async fn save_entity_pages(&self, map: HashMap<String, usize>) -> AppResult<()> {
        let mut layout = self.load_raw().await?;
        layout.entity_pages = map
            .into_iter()
            .map(|(id, page)| {
                let clamped = if page == 0 { 1 } else { page };
                (id, clamped)
            })
            .collect();
        self.save_raw(&layout).await
    }
}
