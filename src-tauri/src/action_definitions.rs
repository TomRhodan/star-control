use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionDefinitions {
    pub categories: HashMap<String, Vec<ActionInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInfo {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub default_device: Option<String>,
}

impl ActionDefinitions {
    pub fn new() -> Self {
        Self { categories: HashMap::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteBinding {
    pub category: String,
    pub action_name: String,
    pub display_name: String,
    pub current_input: String,
    pub device_type: String,
    pub description: Option<String>,
    pub is_custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingStats {
    pub total: usize,
    pub custom: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingListResponse {
    pub bindings: Vec<CompleteBinding>,
    pub stats: BindingStats,
}
