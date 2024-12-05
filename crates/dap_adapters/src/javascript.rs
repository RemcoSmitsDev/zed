use adapters::latest_github_release;
use dap::transport::{TcpTransport, Transport};
use regex::Regex;
use std::{collections::HashMap, net::Ipv4Addr};
use sysinfo::{Pid, Process};
use task::DebugRequestType;

use crate::*;

pub(crate) struct JsDebugAdapter {
    port: u16,
    host: Ipv4Addr,
    timeout: Option<u64>,
}

impl JsDebugAdapter {
    const ADAPTER_NAME: &'static str = "vscode-js-debug";
    const ADAPTER_PATH: &'static str = "js-debug/src/dapDebugServer.js";

    pub(crate) async fn new(host: TCPHost) -> Result<Self> {
        Ok(JsDebugAdapter {
            host: host.host(),
            timeout: host.timeout,
            port: TcpTransport::port(&host).await?,
        })
    }
}

#[async_trait(?Send)]
impl DebugAdapter for JsDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::ADAPTER_NAME.into())
    }

    fn transport(&self) -> Box<dyn Transport> {
        Box::new(TcpTransport::new(self.host, self.port, self.timeout))
    }

    async fn fetch_latest_adapter_version(
        &self,
        delegate: &dyn DapDelegate,
    ) -> Result<AdapterVersion> {
        let http_client = delegate
            .http_client()
            .ok_or_else(|| anyhow!("Failed to download adapter: couldn't connect to GitHub"))?;
        let release = latest_github_release(
            &format!("{}/{}", "microsoft", Self::ADAPTER_NAME),
            true,
            false,
            http_client,
        )
        .await?;

        let asset_name = format!("js-debug-dap-{}.tar.gz", release.tag_name);

        Ok(AdapterVersion {
            tag_name: release.tag_name,
            url: release
                .assets
                .iter()
                .find(|asset| asset.name == asset_name)
                .ok_or_else(|| anyhow!("no asset found matching {:?}", asset_name))?
                .browser_download_url
                .clone(),
        })
    }

    async fn get_installed_binary(
        &self,
        delegate: &dyn DapDelegate,
        config: &DebugAdapterConfig,
    ) -> Result<DebugAdapterBinary> {
        let node_runtime = delegate
            .node_runtime()
            .ok_or(anyhow!("Couldn't get npm runtime"))?;

        let adapter_path = paths::debug_adapters_dir().join(self.name());

        let file_name_prefix = format!("{}_", self.name());

        let adapter_path = util::fs::find_file_name_in_dir(adapter_path.as_path(), |file_name| {
            file_name.starts_with(&file_name_prefix)
        })
        .await
        .ok_or_else(|| anyhow!("Couldn't find JavaScript dap directory"))?;

        Ok(DebugAdapterBinary {
            command: node_runtime
                .binary_path()
                .await?
                .to_string_lossy()
                .into_owned(),
            arguments: Some(vec![
                adapter_path.join(Self::ADAPTER_PATH).into(),
                self.port.to_string().into(),
                self.host.to_string().into(),
            ]),
            cwd: config.cwd.clone(),
            envs: None,
        })
    }

    async fn install_binary(
        &self,
        version: AdapterVersion,
        delegate: &dyn DapDelegate,
    ) -> Result<()> {
        adapters::download_adapter_from_github(
            self.name(),
            version,
            adapters::DownloadedFileType::GzipTar,
            delegate,
        )
        .await?;

        return Ok(());
    }

    fn request_args(&self, config: &DebugAdapterConfig) -> Value {
        let pid = if let DebugRequestType::Attach(attach_config) = &config.request {
            attach_config.process_id
        } else {
            None
        };

        json!({
            "program": config.program,
            "type": "pwa-node",
            "request": match config.request {
                DebugRequestType::Launch => "launch",
                DebugRequestType::Attach(_) => "attach",
            },
            "processId": pid,
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
        let regex = Regex::new(r"(?i)^(?:node|bun|iojs)(?:$|\b)").unwrap();

        Some(
            processes
                .iter()
                .filter(|(_, process)| regex.is_match(&process.name().to_string_lossy()))
                .collect::<Vec<_>>(),
        )
    }
}