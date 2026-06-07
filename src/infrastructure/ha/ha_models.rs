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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ha_state_response_with_full_attributes() {
        let json = r#"{
            "entity_id": "light.lamp",
            "state": "on",
            "attributes": {
                "friendly_name": "Living Room Lamp",
                "unit_of_measurement": "°C"
            }
        }"#;
        let resp: HaStateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.entity_id, "light.lamp");
        assert_eq!(resp.state, "on");
        assert_eq!(resp.attributes.friendly_name, Some("Living Room Lamp".to_string()));
    }

    #[test]
    fn test_ha_state_response_minimal_no_attributes() {
        let json = r#"{"entity_id": "sensor.temp", "state": "22"}"#;
        let resp: HaStateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.entity_id, "sensor.temp");
        assert_eq!(resp.state, "22");
        assert!(resp.attributes.friendly_name.is_none());
    }

    #[test]
    fn test_ha_state_response_empty_attributes() {
        let json = r#"{"entity_id": "switch.outdoor", "state": "off", "attributes": {}}"#;
        let resp: HaStateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.attributes.friendly_name, None);
    }

    #[test]
    fn test_ha_state_response_missing_friendly_name() {
        let json = r#"{"entity_id": "light.bulb", "state": "on", "attributes": {"brightness": 255}}"#;
        let resp: HaStateResponse = serde_json::from_str(json).unwrap();
        assert!(resp.attributes.friendly_name.is_none());
    }
}
