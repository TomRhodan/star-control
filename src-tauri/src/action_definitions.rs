//! Action definitions and binding data structures.
//!
//! Defines the types used for categorized action definitions (e.g., spaceship,
//! vehicle, on-foot controls) and the complete binding list that merges
//! master defaults with user customizations.

use serde::{ Deserialize, Serialize };
use std::collections::HashMap;

/// All known action definitions grouped by category.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionDefinitions {
    pub categories: HashMap<String, Vec<ActionInfo>>,
}

/// Metadata for a single bindable action.
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

/// A fully resolved binding entry combining master defaults with user overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteBinding {
    pub category: String, // Technical name (e.g., spaceship_general)
    pub category_label: String, // Display name (e.g., Space Ship - General)
    pub action_name: String, // Technical name (e.g., v_flightready)
    pub display_name: String, // Translated name (e.g., Flight Ready)
    pub current_input: String,
    pub device_type: String,
    pub description: Option<String>,
    pub is_custom: bool,
}

/// Summary statistics for the binding list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingStats {
    pub total: usize,
    pub custom: usize,
}

/// Response containing all bindings and their statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingListResponse {
    pub bindings: Vec<CompleteBinding>,
    pub stats: BindingStats,
}

/// Maps a physical device to its Star Citizen identity within a profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMapping {
    pub product_name: String,
    pub device_type: String,
    pub sc_guid: Option<String>,
    pub sc_instance: u32,
    #[serde(default)]
    pub alias: Option<String>,
}
