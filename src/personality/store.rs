//! 人格数据结构和持久化

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Traits {
    pub humor: f32,
    pub warmth: f32,
    pub curiosity: f32,
    pub formality: f32,
    pub verbosity: f32,
    pub empathy: f32,
}

impl Default for Traits {
    fn default() -> Self {
        Self {
            humor: 0.5,
            warmth: 0.6,
            curiosity: 0.5,
            formality: 0.3,
            verbosity: 0.4,
            empathy: 0.6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Personality {
    pub name: String,
    pub template: String,
    pub traits: Traits,
    pub custom_prompt: String,
}

impl Default for Personality {
    fn default() -> Self {
        Self {
            name: "default".into(),
            template: "default".into(),
            traits: Traits::default(),
            custom_prompt: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[derive(Default)]
pub struct PersonalityStore {
    pub current: Personality,
    pub snapshots: HashMap<String, Personality>,
}


fn personality_path() -> std::path::PathBuf {
    crate::config::data_dir().join("personality.json")
}

impl PersonalityStore {
    pub fn load() -> Self {
        let path = personality_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = personality_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).ok();
        }
    }
}
