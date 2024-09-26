use crate::*;

#[derive(Debug, Eq, PartialEq, Clone)]
pub(crate) struct PhpDebugAdapter {
    program: String,
    adapter_path: Option<String>,
}

impl PhpDebugAdapter {
    const _ADAPTER_NAME: &'static str = "vscode-php-debug";

    pub(crate) fn new(adapter_config: &DebugAdapterConfig) -> Self {
        PhpDebugAdapter {
            program: adapter_config.program.clone(),
            adapter_path: adapter_config.adapter_path.clone(),
        }
    }
}

#[async_trait(?Send)]
impl DebugAdapter for PhpDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::_ADAPTER_NAME.into())
    }

    async fn connect(
        &self,
        adapter_binary: DebugAdapterBinary,
        cx: &mut AsyncAppContext,
    ) -> Result<TransportParams> {
        let host = TCPHost {
            port: Some(8132),
            host: None,
            delay: Some(1000),
        };

        create_tcp_client(host, adapter_binary, cx).await
    }

    async fn install_or_fetch_binary(
        &self,
        _delegate: Box<dyn DapDelegate>,
    ) -> Result<DebugAdapterBinary> {
        bail!("Install or fetch not implemented for Php debug adapter (yet)");
    }

    fn request_args(&self) -> Value {
        json!({"program": format!("{}", &self.program)})
    }
}
