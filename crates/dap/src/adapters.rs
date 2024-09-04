use crate::client::TransportParams;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::{path::PathBuf, sync::Arc};
use task::{DebugAdapterConfig, DebugAdapterKind};

pub fn build_adapter(adapter_config: &DebugAdapterConfig) -> Result<Box<dyn DebugAdapter>> {
    match adapter_config.kind {
        DebugAdapterKind::Custom => Err(anyhow!("Custom is not implemented")),
        DebugAdapterKind::Python => Ok(Box::new(PythonDebugAdapter::new(adapter_config))),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub struct DebugAdapterName(pub Arc<str>);

pub struct DebugAdapterBinary {
    pub path: PathBuf,
}

pub trait DebugAdapter: Debug + Send + Sync + 'static {
    fn id(&self) -> String {
        "".to_string()
    }

    fn name(self: &Self) -> DebugAdapterName;

    fn connect(&self) -> anyhow::Result<TransportParams>;

    fn get_debug_adapter_start_command(self: &Self) -> String;

    fn is_installed(&self) -> Option<DebugAdapterBinary>;

    fn download_adapter(&self) -> anyhow::Result<DebugAdapterBinary>;
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct PythonDebugAdapter {}

impl PythonDebugAdapter {
    const _ADAPTER_NAME: &'static str = "debugpy";

    fn new(adapter_config: &DebugAdapterConfig) -> Self {
        PythonDebugAdapter {}
    }
}

impl DebugAdapter for PythonDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::_ADAPTER_NAME.into())
    }

    fn connect(&self) -> anyhow::Result<TransportParams> {
        todo!()
    }

    fn get_debug_adapter_start_command(&self) -> String {
        "fail".to_string()
    }

    fn is_installed(&self) -> Option<DebugAdapterBinary> {
        None
    }

    fn download_adapter(&self) -> anyhow::Result<DebugAdapterBinary> {
        Err(anyhow::format_err!("Not implemented"))
    }
}
