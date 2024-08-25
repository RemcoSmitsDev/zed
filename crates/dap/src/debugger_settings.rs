use gpui::{AppContext, Global};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::{Settings, SettingsSources};

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy)]
#[serde(default)]
pub struct DebuggerSettings {
    pub save_breakpoints: bool,
    pub button: bool,
}

impl Default for DebuggerSettings {
    fn default() -> Self {
        Self {
            button: true,
            save_breakpoints: true,
        }
    }
}

impl Settings for DebuggerSettings {
    const KEY: Option<&'static str> = Some("debugger");

    type FileContent = Self;

    fn load(
        sources: SettingsSources<Self::FileContent>,
        _: &mut AppContext,
    ) -> anyhow::Result<Self> {
        sources.json_merge()
    }
}

impl Global for DebuggerSettings {}
