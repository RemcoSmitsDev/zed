use crate::client::TransportParams;
use ::fs::Fs;
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use futures::AsyncReadExt;
use gpui::AsyncAppContext;
use http_client::{github::latest_github_release, HttpClient};
use node_runtime::NodeRuntime;
use serde_json::Value;
use smol::{
    self,
    fs::File,
    io::BufReader,
    net::{TcpListener, TcpStream},
    process,
};
use std::{
    collections::HashMap,
    ffi::OsString,
    fmt::Debug,
    net::{Ipv4Addr, SocketAddrV4},
    path::Path,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use task::{DebugAdapterConfig, TCPHost};

/// Get an open port to use with the tcp client when not supplied by debug config
async fn get_open_port(host: Ipv4Addr) -> Option<u16> {
    Some(
        TcpListener::bind(SocketAddrV4::new(host, 0))
            .await
            .ok()?
            .local_addr()
            .ok()?
            .port(),
    )
}

pub trait DapDelegate {
    fn http_client(&self) -> Option<Arc<dyn HttpClient>>;
    fn node_runtime(&self) -> Option<NodeRuntime>;
    fn fs(&self) -> Arc<dyn Fs>;
}

/// TCP clients don't have an error communication stream with an adapter
/// # Parameters
/// - `host`: The ip/port that that the client will connect too
/// - `adapter_binary`: The debug adapter binary to start
/// - `cx`: The context that the new client belongs too
pub async fn create_tcp_client(
    host: TCPHost,
    adapter_binary: &DebugAdapterBinary,
    cx: &mut AsyncAppContext,
) -> Result<TransportParams> {
    let host_address = host.host.unwrap_or_else(|| Ipv4Addr::new(127, 0, 0, 1));

    let mut port = host.port;
    if port.is_none() {
        port = get_open_port(host_address).await;
    }

    let mut command = process::Command::new(&adapter_binary.command);

    if let Some(args) = &adapter_binary.arguments {
        command.args(args);
    }

    if let Some(envs) = &adapter_binary.envs {
        command.envs(envs);
    }

    command
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
    log::info!("Debug adapter has connected to tcp server");

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
pub fn create_stdio_client(adapter_binary: &DebugAdapterBinary) -> Result<TransportParams> {
    let mut command = process::Command::new(&adapter_binary.command);

    if let Some(args) = &adapter_binary.arguments {
        command.args(args);
    }

    if let Some(envs) = &adapter_binary.envs {
        command.envs(envs);
    }

    command
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

    log::info!("Debug adapter has connected to stdio adapter");

    Ok(TransportParams::new(
        Box::new(BufReader::new(stdout)),
        Box::new(stdin),
        Some(Box::new(BufReader::new(stderr))),
        Some(process),
    ))
}

pub struct DebugAdapterName(pub Arc<str>);

impl AsRef<Path> for DebugAdapterName {
    fn as_ref(&self) -> &Path {
        Path::new(&*self.0)
    }
}

impl std::fmt::Display for DebugAdapterName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Debug, Clone)]
pub struct DebugAdapterBinary {
    pub command: String,
    pub arguments: Option<Vec<OsString>>,
    pub envs: Option<HashMap<String, String>>,
}

pub enum DebugAdapterDownloadKind {
    Github(GithubRepo),
    Custom,
}

async fn download_adapter_from_github(
    adapter_name: DebugAdapterName,
    github_repo: GithubRepo,
    delegate: &dyn DapDelegate,
) -> Result<()> {
    let adapter_path = paths::debug_adapters_dir().join(&adapter_name);
    let fs = delegate.fs();

    if fs.is_dir(adapter_path.as_path()).await {
        return Ok(());
    }

    if let Some(http_client) = delegate.http_client() {
        if !adapter_path.exists() {
            fs.create_dir(&adapter_path.as_path()).await?;
        }

        let repo_name_with_owner = format!("{}/{}", github_repo.repo_owner, github_repo.repo_name);
        let release =
            latest_github_release(&repo_name_with_owner, false, false, http_client.clone()).await?;

        let asset_name = format!("{}_{}.zip", &adapter_name, release.tag_name);
        let zip_path = adapter_path.join(&asset_name);

        if smol::fs::metadata(&zip_path).await.is_err() {
            let mut response = http_client
                .get(&release.zipball_url, Default::default(), true)
                .await
                .context("Error downloading release")?;

            let mut file = File::create(&zip_path).await?;
            futures::io::copy(response.body_mut(), &mut file).await?;

            let _unzip_status = process::Command::new("unzip")
                .current_dir(&adapter_path)
                .arg(&zip_path)
                .output()
                .await?
                .status;

            fs.remove_file(&zip_path.as_path(), Default::default())
                .await?;

            let file_name = util::fs::find_file_name_in_dir(&adapter_path.as_path(), |file_name| {
                file_name.contains(&adapter_name.to_string())
            })
            .await
            .ok_or_else(|| anyhow!("Unzipped directory not found"));

            let file_name = file_name?;

            fs.rename(
                file_name.as_path(),
                adapter_path
                    .join(format!("{}_{}", adapter_name, release.tag_name))
                    .as_path(),
                Default::default(),
            )
            .await?;

            // if !unzip_status.success() {
            //     dbg!(unzip_status);
            //     Err(anyhow!("failed to unzip downloaded dap archive"))?;
            // }

            return Ok(());
        }
    }

    bail!("Install failed to download & counldn't preinstalled dap")
}

pub struct GithubRepo {
    pub repo_name: String,
    pub repo_owner: String,
}

#[async_trait(?Send)]
pub trait DebugAdapter: 'static + Send + Sync {
    fn id(&self) -> String {
        "".to_string()
    }

    fn download_kind(&self) -> DebugAdapterDownloadKind;

    fn name(&self) -> DebugAdapterName;

    async fn connect(
        &self,
        adapter_binary: &DebugAdapterBinary,
        cx: &mut AsyncAppContext,
    ) -> anyhow::Result<TransportParams>;

    /// Installs the binary for the debug adapter.
    /// This method is called when the adapter binary is not found or needs to be updated.
    /// It should download and install the necessary files for the debug adapter to function.
    async fn install_binary(&self, delegate: &dyn DapDelegate) -> Result<()> {
        let adapter_name = self.name();
        let download_kind = self.download_kind();

        match download_kind {
            DebugAdapterDownloadKind::Github(github_repo) => {
                download_adapter_from_github(adapter_name, github_repo, delegate).await
            }
            DebugAdapterDownloadKind::Custom => Ok(()),
        }
    }

    async fn fetch_binary(
        &self,
        delegate: &dyn DapDelegate,
        config: &DebugAdapterConfig,
    ) -> Result<DebugAdapterBinary>;

    /// Should return base configuration to make the debug adapter work
    fn request_args(&self, config: &DebugAdapterConfig) -> Value;
}
