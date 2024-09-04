use crate::client::TransportParams;
use anyhow::{anyhow, Context, Result};
use gpui::Task;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use smol::{self, io::BufReader, process};
use std::fmt::{format, Debug};
use std::future::Future;
use std::{path::PathBuf, process::Stdio, sync::Arc};
use task::{DebugAdapterConfig, DebugAdapterKind};

pub fn build_adapter(adapter_config: &DebugAdapterConfig) -> Result<Box<dyn DebugAdapter>> {
    match adapter_config.kind {
        DebugAdapterKind::Custom => Err(anyhow!("Custom is not implemented")),
        DebugAdapterKind::Python => Ok(Box::new(PythonDebugAdapter::new(adapter_config))),
    }
}

/// Creates a debug client that connects to an adapter through std input/output
///
/// # Parameters
/// - `command`: The command that starts the debugger
/// - `args`: Arguments of the command that starts the debugger
/// - `cwd`: The absolute path of the project that is being debugged
fn create_stdio_client(
    command: &String,
    args: &Vec<String>,
    cwd: &PathBuf,
) -> Result<TransportParams> {
    let mut command = process::Command::new(command);
    command
        // .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut process = command
        .spawn()
        .with_context(|| "failed to spawn command.")?;

    let stdin = process
        .stdin
        .take()
        .ok_or_else(|| anyhow!("Failed to open stdin"))?;
    let stdout = process
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to open stdout"))?;
    let stderr = process
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to open stderr"))?;

    Ok(TransportParams::new(
        Box::new(BufReader::new(stdout)),
        Box::new(stdin),
        Some(Box::new(BufReader::new(stderr))),
        Some(process),
    ))
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

    fn request_args(&self) -> Value;
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct PythonDebugAdapter {
    program: String,
    adapter_path: Option<String>,
}

impl PythonDebugAdapter {
    const _ADAPTER_NAME: &'static str = "debugpy";

    fn new(adapter_config: &DebugAdapterConfig) -> Self {
        PythonDebugAdapter {
            program: adapter_config.program.clone(),
            adapter_path: adapter_config.adapter_path.clone(),
        }
    }
}

impl DebugAdapter for PythonDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::_ADAPTER_NAME.into())
    }

    fn connect(&self) -> Result<TransportParams> {
        let command = "python3".to_string();
        let args = vec![self
            .adapter_path
            .clone()
            .unwrap_or("/Users/eid/Developer/zed_debugger/".to_string())];

        create_stdio_client(&command, &args, &PathBuf::new())
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

    fn request_args(&self) -> Value {
        json!({"program": format!("{}", &self.program)})
    }
}
