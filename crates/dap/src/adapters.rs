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

    fn download_adapter(&mut self) -> DebugAdapterBinary;
}

struct PythonDebugAdapter {}

impl PythonDebugAdapter {
    const ADAPTER_NAME: &'static str = "debugpy";
}

impl DebugAdapter for PythonDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::ADAPTER_NAME.into())
    }

    fn get_debug_adapter_start_command(&self) -> String {
        todo!()
    }

    fn is_installed(&self) -> Option<DebugAdapterBinary> {
        todo!()
    }

    fn download_adapter(&mut self) -> DebugAdapterBinary {
        todo!()
    }
}
