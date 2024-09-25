use crate::client::TransportParams;
use ::fs::Fs;
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use futures::AsyncReadExt;
use gpui::AsyncAppContext;
use http_client::{github::latest_github_release, HttpClient};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use smol::{
    self,
    fs::{self, File},
    io::BufReader,
    net::{TcpListener, TcpStream},
    process,
};
use std::{
    collections::HashMap,
    ffi::OsString,
    fmt::Debug,
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};
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

/// Get an open port to use with the tcp client when not supplied by debug config
async fn get_port(host: Ipv4Addr) -> Option<u16> {
    Some(
        TcpListener::bind(SocketAddrV4::new(host, 0))
            .await
            .ok()?
            .local_addr()
            .ok()?
            .port(),
    )
}

/// Creates a debug client that connects to an adapter through tcp
///
/// TCP clients don't have an error communication stream with an adapter
///
/// # Parameters
/// - `host`: The ip/port that that the client will connect too
/// - `adapter_binary`: The debug adapter binary to start
/// - `cx`: The context that the new client belongs too
async fn create_tcp_client(
    host: TCPHost,
    adapter_binary: DebugAdapterBinary,
    cx: &mut AsyncAppContext,
) -> Result<TransportParams> {
    let host_address = host.host.unwrap_or_else(|| Ipv4Addr::new(127, 0, 0, 1));

    let mut port = host.port;
    if port.is_none() {
        port = get_port(host_address).await;
    }

    let mut command = process::Command::new(adapter_binary.path);
    command
        .args(adapter_binary.arguments)
        .envs(adapter_binary.env.clone().unwrap_or_default())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let process = command
        .spawn()
        .with_context(|| "failed to start debug adapter.")?;

    if let Some(delay) = host.delay {
        // some debug adapters need some time to start the TCP server
        // so we have to wait few milliseconds before we can connect to it
        cx.background_executor()
            .timer(Duration::from_millis(delay))
            .await;
    }

    let address = SocketAddrV4::new(
        host_address,
        port.ok_or(anyhow!("Port is required to connect to TCP server"))?,
    );

    let (rx, tx) = TcpStream::connect(address).await?.split();

    Ok(TransportParams::new(
        Box::new(BufReader::new(rx)),
        Box::new(tx),
        None,
        Some(process),
    ))
}

/// Creates a debug client that connects to an adapter through std input/output
///
/// # Parameters
/// - `adapter_binary`: The debug adapter binary to start
fn create_stdio_client(adapter_binary: DebugAdapterBinary) -> Result<TransportParams> {
    let mut command = if let Some(start_command) = &adapter_binary.start_command {
        let mut command = process::Command::new(start_command);
        command.arg(adapter_binary.path);
        command
    } else {
        let mut command = process::Command::new(adapter_binary.path);
        command.args(adapter_binary.arguments);
        command
    };

    command
        .envs(adapter_binary.env.clone().unwrap_or_default())
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

#[derive(Debug, Clone)]
pub struct DebugAdapterBinary {
    pub start_command: Option<String>,
    pub path: PathBuf,
    pub arguments: Vec<OsString>,
    pub env: Option<HashMap<String, String>>,
}

#[async_trait(?Send)]
pub trait DebugAdapter: Debug + Send + Sync + 'static {
    fn id(&self) -> String {
        "".to_string()
    }

    fn name(&self) -> DebugAdapterName;

    async fn connect(
        &self,
        adapter_binary: DebugAdapterBinary,
        cx: &mut AsyncAppContext,
    ) -> anyhow::Result<TransportParams>;

    async fn install_or_fetch_binary(
        &self,
        delegate: Box<dyn DapDelegate>,
    ) -> Result<DebugAdapterBinary>;

    fn request_args(&self) -> Value;
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

pub trait DapDelegate {
    fn http_client(&self) -> Option<Arc<dyn HttpClient>>;
    fn fs(&self) -> Arc<dyn Fs>;
}
