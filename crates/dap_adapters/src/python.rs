use dap::{
    transport::{TcpTransport, Transport},
    DebugRequestType,
};
use gpui::AsyncAppContext;
use regex::Regex;
use std::{collections::HashMap, net::Ipv4Addr, path::PathBuf, sync::Arc};
use sysinfo::{Pid, Process};

use crate::*;

pub(crate) struct PythonDebugAdapter {
    port: u16,
    host: Ipv4Addr,
    timeout: Option<u64>,
}

impl PythonDebugAdapter {
    const ADAPTER_NAME: &'static str = "debugpy";
    const ADAPTER_PATH: &'static str = "src/debugpy/adapter";
    const LANGUAGE_NAME: &'static str = "Python";

    pub(crate) async fn new(host: &TCPHost) -> Result<Self> {
        Ok(PythonDebugAdapter {
            port: TcpTransport::port(host).await?,
            host: host.host(),
            timeout: host.timeout,
        })
    }
}

#[async_trait(?Send)]
impl DebugAdapter for PythonDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::ADAPTER_NAME.into())
    }

    fn transport(&self) -> Arc<dyn Transport> {
        Arc::new(TcpTransport::new(self.host, self.port, self.timeout))
    }

    async fn fetch_latest_adapter_version(
        &self,
        delegate: &dyn DapDelegate,
    ) -> Result<AdapterVersion> {
        let github_repo = GithubRepo {
            repo_name: Self::ADAPTER_NAME.into(),
            repo_owner: "microsoft".into(),
        };

        adapters::fetch_latest_adapter_version_from_github(github_repo, delegate).await
    }

    async fn install_binary(
        &self,
        version: AdapterVersion,
        delegate: &dyn DapDelegate,
    ) -> Result<()> {
        let version_path = adapters::download_adapter_from_github(
            self.name(),
            version,
            adapters::DownloadedFileType::Zip,
            delegate,
        )
        .await?;

        // only needed when you install the latest version for the first time
        if let Some(debugpy_dir) =
            util::fs::find_file_name_in_dir(version_path.as_path(), |file_name| {
                file_name.starts_with("microsoft-debugpy-")
            })
            .await
        {
            // TODO Debugger: Rename folder instead of moving all files to another folder
            // We're doing uncessary IO work right now
            util::fs::move_folder_files_to_folder(debugpy_dir.as_path(), version_path.as_path())
                .await?;
        }

        Ok(())
    }

    async fn get_installed_binary(
        &self,
        delegate: &dyn DapDelegate,
        config: &DebugAdapterConfig,
        user_installed_path: Option<PathBuf>,
        cx: &mut AsyncAppContext,
    ) -> Result<DebugAdapterBinary> {
        let debugpy_dir = if let Some(user_installed_path) = user_installed_path {
            user_installed_path
        } else {
            let adapter_path = paths::debug_adapters_dir().join(self.name());
            let file_name_prefix = format!("{}_", self.name());

            util::fs::find_file_name_in_dir(adapter_path.as_path(), |file_name| {
                file_name.starts_with(&file_name_prefix)
            })
            .await
            .ok_or_else(|| anyhow!("Debugpy directory not found"))?
        };

        let toolchain = delegate
            .toolchain_store()
            .active_toolchain(
                delegate.worktree_id(),
                language::LanguageName::new(Self::LANGUAGE_NAME),
                cx,
            )
            .await
            .ok_or(anyhow!("failed to find active toolchain for Python"))?;

        Ok(DebugAdapterBinary {
            command: toolchain.path.to_string(),
            arguments: Some(vec![
                debugpy_dir.join(Self::ADAPTER_PATH).into(),
                format!("--port={}", self.port).into(),
                format!("--host={}", self.host).into(),
            ]),
            cwd: config.cwd.clone(),
            envs: None,
        })
    }

    fn request_args(&self, config: &DebugAdapterConfig) -> Value {
        let pid = if let DebugRequestType::Attach(attach_config) = &config.request {
            attach_config.process_id
        } else {
            None
        };

        json!({
            "request": match config.request {
                DebugRequestType::Launch => "launch",
                DebugRequestType::Attach(_) => "attach",
            },
            "processId": pid,
            "program": config.program,
            "subProcess": true,
            "cwd": config.cwd,
        })
    }

    fn supports_attach(&self) -> bool {
        true
    }

    fn attach_processes<'a>(
        &self,
        processes: &'a HashMap<Pid, Process>,
    ) -> Option<Vec<(&'a Pid, &'a Process)>> {
        let regex = Regex::new(r"(?i)^(?:python3|python|py)(?:$|\b)").unwrap();

        Some(
            processes
                .iter()
                .filter(|(_, process)| regex.is_match(&process.name().to_string_lossy()))
                .collect::<Vec<_>>(),
        )
    }
}
