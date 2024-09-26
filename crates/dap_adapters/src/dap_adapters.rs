pub mod javascript;
pub mod php;
pub mod python;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use dap::{
    adapters::{
        create_stdio_client, create_tcp_client, DapDelegate, DebugAdapter, DebugAdapterBinary,
        DebugAdapterName,
    },
    client::TransportParams,
};
use gpui::AsyncAppContext;
use http_client::github::latest_github_release;
use serde_json::{json, Value};
use smol::{
    fs::{self, File},
    process,
};
use std::{fmt::Debug, process::Stdio};
use task::{CustomArgs, DebugAdapterConfig, DebugAdapterKind, DebugConnectionType, TCPHost};

pub fn build_adapter(adapter_config: &DebugAdapterConfig) -> Result<Box<dyn DebugAdapter>> {
    match &adapter_config.kind {
        DebugAdapterKind::Custom(start_args) => Ok(Box::new(CustomDebugAdapter::new(
            adapter_config,
            start_args.clone(),
        ))),
        DebugAdapterKind::Python => Ok(Box::new(PythonDebugAdapter::new(adapter_config))),
        DebugAdapterKind::PHP => Ok(Box::new(PhpDebugAdapter::new(adapter_config))),
        DebugAdapterKind::Lldb => Ok(Box::new(LldbDebugAdapter::new(adapter_config))),
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct CustomDebugAdapter {
    start_command: String,
    initialize_args: Option<Vec<String>>,
    program: String,
    connection: DebugConnectionType,
}

impl CustomDebugAdapter {
    const _ADAPTER_NAME: &'static str = "custom_dap";

    fn new(adapter_config: &DebugAdapterConfig, custom_args: CustomArgs) -> Self {
        CustomDebugAdapter {
            start_command: custom_args.start_command,
            program: adapter_config.program.clone(),
            connection: custom_args.connection,
            initialize_args: adapter_config.initialize_args.clone(),
        }
    }
}

#[async_trait(?Send)]
impl DebugAdapter for CustomDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::_ADAPTER_NAME.into())
    }

    async fn connect(
        &self,
        adapter_binary: DebugAdapterBinary,
        cx: &mut AsyncAppContext,
    ) -> Result<TransportParams> {
        match &self.connection {
            DebugConnectionType::STDIO => create_stdio_client(adapter_binary),
            DebugConnectionType::TCP(tcp_host) => {
                create_tcp_client(tcp_host.clone(), adapter_binary, cx).await
            }
        }
    }

    async fn install_or_fetch_binary(
        &self,
        _delegate: Box<dyn DapDelegate>,
    ) -> Result<DebugAdapterBinary> {
        bail!("Install or fetch not implemented for custom debug adapter (yet)");
    }

    fn request_args(&self) -> Value {
        let base_args = json!({
            "program": format!("{}", &self.program)
        });

        // TODO Debugger: Figure out a way to combine this with base args
        // if let Some(args) = &self.initialize_args {
        //     let args = json!(args.clone()).as_object().into_iter();
        //     base_args.as_object_mut().unwrap().extend(args);
        // }

        base_args
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct PythonDebugAdapter {
    program: String,
    adapter_path: Option<String>,
}

impl PythonDebugAdapter {
    const ADAPTER_NAME: &'static str = "debugpy";

    fn new(adapter_config: &DebugAdapterConfig) -> Self {
        PythonDebugAdapter {
            program: adapter_config.program.clone(),
            adapter_path: adapter_config.adapter_path.clone(),
        }
    }
}

#[async_trait(?Send)]
impl DebugAdapter for PythonDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::ADAPTER_NAME.into())
    }

    async fn connect(
        &self,
        adapter_binary: DebugAdapterBinary,
        _cx: &mut AsyncAppContext,
    ) -> Result<TransportParams> {
        create_stdio_client(adapter_binary)
    }

    async fn install_or_fetch_binary(
        &self,
        delegate: Box<dyn DapDelegate>,
    ) -> Result<DebugAdapterBinary> {
        let adapter_path = paths::debug_adapters_dir().join("debugpy/src/debugpy/adapter");
        let fs = delegate.fs();

        if fs.is_dir(adapter_path.as_path()).await {
            return Ok(DebugAdapterBinary {
                start_command: Some("python3".to_string()),
                path: adapter_path,
                arguments: vec![],
                env: None,
            });
        } else if let Some(http_client) = delegate.http_client() {
            let debugpy_dir = paths::debug_adapters_dir().join("debugpy");

            if !debugpy_dir.exists() {
                fs.create_dir(&debugpy_dir.as_path()).await?;
            }

            let release =
                latest_github_release("microsoft/debugpy", false, false, http_client.clone())
                    .await?;
            let asset_name = format!("{}.zip", release.tag_name);

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

                let mut ls = process::Command::new("ls")
                    .current_dir(&debugpy_dir)
                    .stdout(Stdio::piped())
                    .spawn()?;

                let std = ls
                    .stdout
                    .take()
                    .ok_or(anyhow!("Failed to list directories"))?
                    .into_stdio()
                    .await?;

                let file_name = String::from_utf8(
                    process::Command::new("grep")
                        .arg("microsoft-debugpy")
                        .stdin(std)
                        .output()
                        .await?
                        .stdout,
                )?;

                let file_name = file_name.trim_end();
                process::Command::new("sh")
                    .current_dir(&debugpy_dir)
                    .arg("-c")
                    .arg(format!("mv {file_name}/* ."))
                    .output()
                    .await?;

                process::Command::new("rm")
                    .current_dir(&debugpy_dir)
                    .arg("-rf")
                    .arg(file_name)
                    .arg(zip_path)
                    .output()
                    .await?;

                return Ok(DebugAdapterBinary {
                    start_command: Some("python3".to_string()),
                    path: adapter_path,
                    arguments: vec![],
                    env: None,
                });
            }
            return Err(anyhow!("Failed to download debugpy"));
        } else {
            return Err(anyhow!(
                "Could not find debugpy in paths or connect to http"
            ));
        }
    }

    fn request_args(&self) -> Value {
        json!({"program": format!("{}", &self.program)})
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct PhpDebugAdapter {
    program: String,
    adapter_path: Option<String>,
}

impl PhpDebugAdapter {
    const _ADAPTER_NAME: &'static str = "vscode-php-debug";

    fn new(adapter_config: &DebugAdapterConfig) -> Self {
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

#[derive(Debug, Eq, PartialEq, Clone)]
struct LldbDebugAdapter {
    program: String,
    adapter_path: Option<String>,
}

impl LldbDebugAdapter {
    const _ADAPTER_NAME: &'static str = "lldb";

    fn new(adapter_config: &DebugAdapterConfig) -> Self {
        LldbDebugAdapter {
            program: adapter_config.program.clone(),
            adapter_path: adapter_config.adapter_path.clone(),
        }
    }
}

#[async_trait(?Send)]
impl DebugAdapter for LldbDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        DebugAdapterName(Self::_ADAPTER_NAME.into())
    }

    async fn connect(
        &self,
        adapter_binary: DebugAdapterBinary,
        _: &mut AsyncAppContext,
    ) -> Result<TransportParams> {
        create_stdio_client(adapter_binary)
    }

    async fn install_or_fetch_binary(
        &self,
        _delegate: Box<dyn DapDelegate>,
    ) -> Result<DebugAdapterBinary> {
        bail!("Install or fetch binary not implemented for lldb debug adapter (yet)");
    }

    fn request_args(&self) -> Value {
        json!({"program": format!("{}", &self.program)})
    }
}
