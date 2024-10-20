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

    async fn install_binary(&self, delegate: &dyn DapDelegate) -> Result<()> {
        let adapter_path = paths::debug_adapters_dir().join(self.name());
        let fs = delegate.fs();

        if fs.is_dir(adapter_path.as_path()).await {
            return Ok(());
        }

        if let Some(http_client) = delegate.http_client() {
            let debugpy_dir = paths::debug_adapters_dir().join("debugpy");

            if !debugpy_dir.exists() {
                fs.create_dir(&debugpy_dir.as_path()).await?;
            }

            let release =
                latest_github_release("microsoft/debugpy", false, false, http_client.clone())
                    .await?;
            let asset_name = format!("{}_{}.zip", self.name(), release.tag_name);

            let zip_path = debugpy_dir.join(asset_name);

            if fs::metadata(&zip_path).await.is_err() {
                let mut response = http_client
                    .get(&release.zipball_url, Default::default(), true)
                    .await
                    .context("Error downloading release")?;

                let mut file = File::create(&zip_path).await?;
                futures::io::copy(response.body_mut(), &mut file).await?;

                let _unzip_status = process::Command::new("unzip")
                    .current_dir(&debugpy_dir)
                    .arg(&zip_path)
                    .output()
                    .await?
                    .status;

                let file_name =
                    util::fs::find_file_name_in_dir(&debugpy_dir.as_path(), |file_name| {
                        file_name.starts_with("microsoft-debugpy-")
                    })
                    .await
                    .ok_or_else(|| anyhow!("Debugpy unzipped directory not found"))?;

                fs.rename(
                    file_name.as_path(),
                    debugpy_dir
                        .join(format!("{}_{}", self.name(), release.tag_name))
                        .as_path(),
                    Default::default(),
                )
                .await?;

                fs.remove_file(&zip_path.as_path(), Default::default())
                    .await?;

                // if !unzip_status.success() {
                //     dbg!(unzip_status);
                //     Err(anyhow!("failed to unzip debugpy archive"))?;
                // }

                return Ok(());
            }
        }

        bail!("Install or fetch not implemented for Python debug adapter (yet)");
    }

    fn request_args(&self, config: &DebugAdapterConfig) -> Value {
        json!({"program": config.program, "subProcess": true})
    }
}
