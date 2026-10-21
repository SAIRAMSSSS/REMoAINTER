use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorPoint {
    pub id: usize,
    pub name: String,
    pub source_layer_id: usize,
    pub channel: AnchorChannel,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorChannel {
    All,
    BaseColor,
    Height,
    Normal,
    Roughness,
    Metallic,
    MaskOnly,
}

impl AnchorPoint {
    pub fn new(
        id: usize,
        name: impl Into<String>,
        source_layer_id: usize,
        channel: AnchorChannel,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            source_layer_id,
            channel,
            created_at: chrono::Local::now().format("%H:%M:%S").to_string(),
        }
    }
}
