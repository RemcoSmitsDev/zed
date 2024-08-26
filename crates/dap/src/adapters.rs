use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub struct DebugAdapterName(pub Arc<str>);

pub struct DebugAdapterBinary {
    pub path: PathBuf,
}

pub trait DebugAdapter {
    fn name(&self) -> DebugAdapterName;

    fn get_debug_adapter_start_command(&self) -> String;

    fn is_installed(&self) -> Option<DebugAdapterBinary>;

    fn download_adapter(&mut self) -> Result<DebugAdapterBinary, ()>;
}

struct _PythonDebugAdapter {}

impl _PythonDebugAdapter {
    const _ADAPTER_NAME: &'static str = "debugpy";
}

impl DebugAdapter for _PythonDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::_ADAPTER_NAME.into())
    }

    fn get_debug_adapter_start_command(&self) -> String {
        "fail".to_string()
    }

    fn is_installed(&self) -> Option<DebugAdapterBinary> {
        None
    }

    fn download_adapter(&mut self) -> Result<DebugAdapterBinary, ()> {
        Err(())
    }
}
