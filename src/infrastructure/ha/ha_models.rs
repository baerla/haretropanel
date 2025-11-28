use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct HaStateResponse {
    pub entity_id: String,
    pub state: String,
    
    #[serde(default)]
    pub attributes: HaAttributes,
}

#[derive(Debug, Deserialize, Default)]
pub struct HaAttributes {
    #[serde(default)]
    pub friendly_name: Option<String>,

    #[serde(default)]
    pub unit_of_measurement: Option<String>,
}
