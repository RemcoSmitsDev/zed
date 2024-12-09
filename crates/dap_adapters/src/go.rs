use dap::transport::{TcpTransport, Transport};
use std::{ffi::OsStr, net::Ipv4Addr, path::PathBuf, sync::Arc};

use crate::*;

pub(crate) struct GoDebugAdapter {
    port: u16,
    host: Ipv4Addr,
    timeout: Option<u64>,
}

impl GoDebugAdapter {
    const _ADAPTER_NAME: &'static str = "delve";
    // const ADAPTER_PATH: &'static str = "src/debugpy/adapter";

    pub(crate) async fn new(host: &TCPHost) -> Result<Self> {
        Ok(GoDebugAdapter {
            port: TcpTransport::port(host).await?,
            host: host.host(),
            timeout: host.timeout,
        })
    }
}

#[async_trait(?Send)]
impl DebugAdapter for GoDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::_ADAPTER_NAME.into())
    }

    fn transport(&self) -> Arc<dyn Transport> {
        Arc::new(TcpTransport::new(self.host, self.port, self.timeout))
    }

    async fn get_binary(
        &self,
        delegate: &dyn DapDelegate,
        config: &DebugAdapterConfig,
        user_installed_path: Option<PathBuf>,
    ) -> Result<DebugAdapterBinary> {
        self.get_installed_binary(delegate, config, user_installed_path)
            .await
    }

    async fn fetch_latest_adapter_version(
        &self,
        _delegate: &dyn DapDelegate,
    ) -> Result<AdapterVersion> {
        unimplemented!("This adapter is used from path for now");
    }

    async fn install_binary(
        &self,
        version: AdapterVersion,
        delegate: &dyn DapDelegate,
    ) -> Result<()> {
        adapters::download_adapter_from_github(
            self.name(),
            version,
            adapters::DownloadedFileType::Zip,
            delegate,
        )
        .await?;
        Ok(())
    }

    async fn get_installed_binary(
        &self,
        delegate: &dyn DapDelegate,
        config: &DebugAdapterConfig,
        _: Option<PathBuf>,
    ) -> Result<DebugAdapterBinary> {
        let delve_path = delegate
            .which(OsStr::new("dlv"))
            .and_then(|p| p.to_str().map(|p| p.to_string()))
            .ok_or(anyhow!("Dlv not found in path"))?;

        Ok(DebugAdapterBinary {
            command: delve_path,
            arguments: Some(vec![
                "dap".into(),
                "--listen".into(),
                format!("{}:{}", self.host, self.port).into(),
            ]),
            cwd: config.cwd.clone(),
            envs: None,
        })
    }

    fn request_args(&self, config: &DebugAdapterConfig) -> Value {
        json!({
            "program": config.program,
            "cwd": config.cwd,
            "subProcess": true,
        })
    }
}
