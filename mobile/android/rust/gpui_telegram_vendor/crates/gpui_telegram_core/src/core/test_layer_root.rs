//! Telegram Test Harness: UI Layer Stack & Modal Root Manager
//! Upstream sources: `Telegram/SourceFiles/test/test_layer_root.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerItem {
    pub id: u32,
    pub title: String,
    pub z_order: i32,
}

/// Test fixture simulating the UI layer stack managing modals, popups, and media overlays.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestLayerRoot {
    pub layers: Vec<LayerItem>,
}

impl TestLayerRoot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_layer(&mut self, id: u32, title: impl Into<String>) {
        let z = self.layers.len() as i32 + 10;
        self.layers.push(LayerItem {
            id,
            title: title.into(),
            z_order: z,
        });
    }

    pub fn pop_layer(&mut self) -> Option<LayerItem> {
        self.layers.pop()
    }

    pub fn top_layer(&self) -> Option<&LayerItem> {
        self.layers.last()
    }

    pub fn count(&self) -> usize {
        self.layers.len()
    }
}
