use crate::*;

#[derive(Debug, Eq, PartialEq, Clone)]
pub(crate) struct PythonDebugAdapter {}

impl PythonDebugAdapter {
    const ADAPTER_NAME: &'static str = "debugpy";
    const ADAPTER_PATH: &'static str = "src/debugpy/adapter";

    pub(crate) fn new() -> Self {
        PythonDebugAdapter {}
    }
}

#[async_trait(?Send)]
impl DebugAdapter for PythonDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::ADAPTER_NAME.into())
    }

    fn download_kind(&self) -> DebugAdapterDownloadKind {
        DebugAdapterDownloadKind::Github(GithubRepo {
            repo_name: "debugpy".to_string(),
            repo_owner: "microsoft".to_string(),
        })
    }

    async fn connect(
        &self,
        adapter_binary: &DebugAdapterBinary,
        _: &mut AsyncAppContext,
    ) -> Result<TransportParams> {
        create_stdio_client(adapter_binary)
    }

    async fn fetch_binary(
        &self,
        _: &dyn DapDelegate,
        _: &DebugAdapterConfig,
    ) -> Result<DebugAdapterBinary> {
        let adapter_path = paths::debug_adapters_dir().join(self.name());

        let debugpy_dir = util::fs::find_file_name_in_dir(adapter_path.as_path(), |file_name| {
            file_name.starts_with("debugpy_")
        })
        .await
        .ok_or_else(|| anyhow!("Debugpy directory not found"))?;

        Ok(DebugAdapterBinary {
            command: "python3".to_string(),
            arguments: Some(vec![debugpy_dir.join(Self::ADAPTER_PATH).into()]),
            envs: None,
        })
    }

    fn request_args(&self, config: &DebugAdapterConfig) -> Value {
        json!({"program": config.program, "subProcess": true})
    }
}
