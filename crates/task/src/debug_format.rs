use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use util::ResultExt;

use std::net::Ipv4Addr;

use crate::{TaskTemplate, TaskTemplates};

/// Represents the type of the debugger adapter connection
#[derive(Deserialize, Serialize, PartialEq, Eq, JsonSchema, Clone, Debug)]
#[serde(rename_all = "lowercase", tag = "connection")]
pub enum DebugConnectionType {
    /// Connect to the debug adapter via TCP
    TCP(TCPHost),
    /// Connect to the debug adapter via STDIO
    STDIO,
}

impl Default for DebugConnectionType {
    fn default() -> Self {
        DebugConnectionType::TCP(TCPHost::default())
    }
}

/// Represents the host information of the debug adapter
#[derive(Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema, Clone, Debug)]
pub struct TCPHost {
    /// The port that the debug adapter is listening on
    pub port: Option<u16>,
    /// The host that the debug adapter is listening too
    pub host: Option<Ipv4Addr>,
    /// The delay in ms between starting and connecting to the debug adapter
    pub delay: Option<u64>,
}

/// Represents the type that will determine which request to call on the debug adapter
#[derive(Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DebugRequestType {
    /// Call the `launch` request on the debug adapter
    #[default]
    Launch,
    /// Call the `attach` request on the debug adapter
    Attach,
}

/// Represents the configuration for the debug adapter
#[derive(Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DebugAdapterConfig {
    /// Unique id of for the debug adapter,
    /// that will be send with the `initialize` request
    pub id: String,
    /// The type of connection the adapter should use
    #[serde(default, flatten)]
    pub connection: DebugConnectionType,
    /// The type of request that should be called on the debug adapter
    #[serde(default)]
    pub request: DebugRequestType,
    /// The configuration options that are send with the `launch` or `attach` request
    /// to the debug adapter
    pub request_args: Option<DebugRequestArgs>,
}

/// Represents the configuration for the debug adapter that is send with the launch request
#[derive(Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema, Clone, Debug)]
#[serde(transparent)]
pub struct DebugRequestArgs {
    pub args: serde_json::Value,
}

// "label" : "Name of debug task",
// "command": "Null",
// "task_type": "debug",
// "debug_adapter or adapter or debugger": "name of adapter or custom",
// "adapter_path": "Abs path to adapter (we would eventually remove this)",
// "session_type": "launch|attach",
// "program": "Program to debug (main.out)"
//
#[derive(Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema, Clone, Debug)]
enum DebugAdapter {
    #[default]
    Custom,
}

#[derive(Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema, Clone, Debug)]
#[serde(rename_all = "snake_case")]
struct DebugTaskDefinition {
    label: String,
    program: String,
    adapter_path: String,
    session_type: DebugRequestType,
    adapter: DebugAdapter,
}

impl DebugTaskDefinition {
    fn to_zed_format(self) -> anyhow::Result<TaskTemplate> {
        todo!();
    }
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct DebugTaskFile {
    tasks: Vec<DebugTaskDefinition>,
}

impl TryFrom<DebugTaskFile> for TaskTemplates {
    type Error = anyhow::Error;

    fn try_from(value: DebugTaskFile) -> Result<Self, Self::Error> {
        let templates = value
            .tasks
            .into_iter()
            .filter_map(|debug_definition| debug_definition.to_zed_format().log_err())
            .collect();

        Ok(Self(templates))
    }
}
