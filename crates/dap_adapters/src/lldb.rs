use anyhow::Result;
use async_trait::async_trait;
use dap::transport::{StdioTransport, Transport};
use task::DebugAdapterConfig;

use crate::*;

#[derive(Debug, Eq, PartialEq, Clone)]
pub(crate) struct LldbDebugAdapter {}

impl LldbDebugAdapter {
    const ADAPTER_NAME: &'static str = "lldb";

    pub(crate) fn new() -> Self {
        LldbDebugAdapter {}
    }
}

#[async_trait(?Send)]
impl DebugAdapter for LldbDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::ADAPTER_NAME.into())
    }

    fn download_kind(&self) -> DebugAdapterDownloadKind {
        DebugAdapterDownloadKind::Github(GithubRepo {
            repo_name: "llvm-project".to_string(),
            repo_owner: "llvm".to_string(),
        })
    }

    fn transport(&self) -> Box<dyn Transport> {
        Box::new(StdioTransport::new())
    }

    async fn fetch_binary(
        &self,
        _: &dyn DapDelegate,
        _: &DebugAdapterConfig,
    ) -> Result<DebugAdapterBinary> {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("xcrun")
                .args(&["-f", "lldb-dap"])
                .output()?;
            let lldb_dap_path = String::from_utf8(output.stdout)?.trim().to_string();

            Ok(DebugAdapterBinary {
                command: lldb_dap_path,
                arguments: None,
                envs: None,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(anyhow::anyhow!(
                "LLDB-DAP is only supported on macOS (Right now)"
            ))
        }
    }

    fn request_args(&self, config: &DebugAdapterConfig) -> Value {
        json!({"program": config.program})
    }
}
