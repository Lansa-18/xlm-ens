use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use stellar_rpc_client::{Client as StellarRpcClient, Ledger, LedgerStart};
use stellar_xdr::curr::{LedgerCloseMeta, Limits, ReadXdr, WriteXdr};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::errors::SdkError;

#[derive(Debug, Clone)]
pub struct CaptiveCoreConfig {
    pub binary_path: PathBuf,
    pub network_passphrase: String,
    pub history_archive_urls: Vec<String>,
    pub working_dir: PathBuf,
    pub state_archive_dir: PathBuf,
    pub start_ledger: u32,
    pub heartbeat_timeout: Duration,
    pub restart_backoff: Duration,
    pub max_restart_attempts: u32,
    pub output_target: CaptiveCoreOutputTarget,
    /// Command arguments for the stellar-core process. Supports
    /// `{config}` and `{start_ledger}` placeholders.
    pub command_args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptiveCoreOutputTarget {
    Stdout,
    TcpSocket(SocketAddr),
}

impl CaptiveCoreConfig {
    pub fn rendered_toml(&self) -> String {
        let mut toml = format!(
            "NODE_IS_VALIDATOR=false\n\
             RUN_STANDALONE=false\n\
             UNSAFE_QUORUM=true\n\
             FAILURE_SAFETY=0\n\
             HTTP_PORT=0\n\
             PUBLIC_HTTP_PORT=false\n\
             NETWORK_PASSPHRASE=\"{}\"\n\
             BUCKET_DIR_PATH=\"{}\"\n",
            escape_toml(&self.network_passphrase),
            escape_toml(&self.working_dir.join("buckets").display().to_string())
        );

        for (idx, url) in self.history_archive_urls.iter().enumerate() {
            toml.push_str(&format!(
                "\n[HISTORY.archive_{idx}]\nget=\"{}\"\n",
                escape_toml(url)
            ));
        }

        toml
    }

    pub fn command_line(&self, config_path: &Path, start_ledger: u32) -> Vec<String> {
        let metadata_stream_target = match self.output_target {
            CaptiveCoreOutputTarget::Stdout => "stdout".to_string(),
            CaptiveCoreOutputTarget::TcpSocket(address) => address.to_string(),
        };
        let base = if self.command_args.is_empty() {
            vec![
                "run".to_string(),
                "--conf".to_string(),
                "{config}".to_string(),
                "--start-at-ledger".to_string(),
                "{start_ledger}".to_string(),
                "--metadata-output-stream".to_string(),
                metadata_stream_target,
            ]
        } else {
            self.command_args.clone()
        };

        base.into_iter()
            .map(|arg| {
                arg.replace("{config}", &config_path.display().to_string())
                    .replace("{start_ledger}", &start_ledger.to_string())
            })
            .collect()
    }
}

impl Default for CaptiveCoreConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("stellar-core"),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            history_archive_urls: Vec::new(),
            working_dir: std::env::temp_dir().join("xlm-ns-captive-core"),
            state_archive_dir: std::env::temp_dir().join("xlm-ns-captive-core-archive"),
            start_ledger: 1,
            heartbeat_timeout: Duration::from_secs(15),
            restart_backoff: Duration::from_secs(5),
            max_restart_attempts: 3,
            output_target: CaptiveCoreOutputTarget::Stdout,
            command_args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestionSource {
    CaptiveCore,
    Remote,
}

#[derive(Debug, Clone)]
pub struct IngestedLedger<T> {
    pub payload: T,
    pub raw_xdr: Vec<u8>,
    pub ledger_sequence: Option<u32>,
    pub source: IngestionSource,
}

#[derive(Debug, Clone)]
pub struct SupervisorStatus {
    pub active_source: IngestionSource,
    pub last_processed_ledger: Option<u32>,
    pub consecutive_restart_failures: u32,
    pub fallback_reason: Option<String>,
    pub last_archive_path: Option<PathBuf>,
    pub last_heartbeat_at: Option<SystemTime>,
}

type Decoder<T> = Arc<dyn Fn(&[u8]) -> Result<T, SdkError> + Send + Sync>;
type SequenceExtractor<T> = Arc<dyn Fn(&T) -> Option<u32> + Send + Sync>;

#[async_trait]
pub trait CaptiveCoreBackend: Send {
    async fn start(&mut self, start_ledger: u32, rendered_toml: &str) -> Result<(), SdkError>;
    async fn next_frame(&mut self, timeout: Duration) -> Result<Vec<u8>, SdkError>;
    async fn archive_state(&mut self, archive_root: &Path) -> Result<Option<PathBuf>, SdkError>;
    async fn stop(&mut self) -> Result<(), SdkError>;
    async fn is_running(&mut self) -> bool;
}

#[async_trait]
pub trait RemoteLedgerSource<T>: Send {
    async fn next_from(&mut self, start_ledger: u32) -> Result<IngestedLedger<T>, SdkError>;
}

pub struct CaptiveCoreIngestor<T, B, R>
where
    B: CaptiveCoreBackend,
    R: RemoteLedgerSource<T>,
{
    config: CaptiveCoreConfig,
    primary: B,
    fallback: R,
    decoder: Decoder<T>,
    sequence_extractor: SequenceExtractor<T>,
    last_processed_ledger: Option<u32>,
    consecutive_restart_failures: u32,
    active_source: IngestionSource,
    fallback_reason: Option<String>,
    next_primary_retry_at: Instant,
    last_archive_path: Option<PathBuf>,
    last_heartbeat_at: Option<SystemTime>,
}

impl<T, B, R> CaptiveCoreIngestor<T, B, R>
where
    B: CaptiveCoreBackend,
    R: RemoteLedgerSource<T>,
{
    pub fn new(
        config: CaptiveCoreConfig,
        primary: B,
        fallback: R,
        decoder: impl Fn(&[u8]) -> Result<T, SdkError> + Send + Sync + 'static,
        sequence_extractor: impl Fn(&T) -> Option<u32> + Send + Sync + 'static,
    ) -> Self {
        Self {
            last_processed_ledger: config.start_ledger.checked_sub(1),
            next_primary_retry_at: Instant::now(),
            config,
            primary,
            fallback,
            decoder: Arc::new(decoder),
            sequence_extractor: Arc::new(sequence_extractor),
            consecutive_restart_failures: 0,
            active_source: IngestionSource::CaptiveCore,
            fallback_reason: None,
            last_archive_path: None,
            last_heartbeat_at: None,
        }
    }

    pub async fn start(&mut self) -> Result<(), SdkError> {
        let rendered = self.config.rendered_toml();
        self.primary
            .start(self.next_start_ledger(), &rendered)
            .await?;
        info!(
            "captive core started at ledger {}",
            self.next_start_ledger()
        );
        Ok(())
    }

    pub fn status(&self) -> SupervisorStatus {
        SupervisorStatus {
            active_source: self.active_source,
            last_processed_ledger: self.last_processed_ledger,
            consecutive_restart_failures: self.consecutive_restart_failures,
            fallback_reason: self.fallback_reason.clone(),
            last_archive_path: self.last_archive_path.clone(),
            last_heartbeat_at: self.last_heartbeat_at,
        }
    }

    pub async fn next_ledger(&mut self) -> Result<IngestedLedger<T>, SdkError> {
        loop {
            if self.active_source == IngestionSource::CaptiveCore {
                if !self.primary.is_running().await {
                    self.handle_primary_failure("captive core process not running".to_string())
                        .await?;
                    continue;
                }

                match self.primary.next_frame(self.config.heartbeat_timeout).await {
                    Ok(frame) => {
                        let payload = (self.decoder)(&frame)?;
                        let ledger_sequence = (self.sequence_extractor)(&payload);
                        if let Some(sequence) = ledger_sequence {
                            self.last_processed_ledger = Some(sequence);
                        }
                        self.active_source = IngestionSource::CaptiveCore;
                        self.fallback_reason = None;
                        self.consecutive_restart_failures = 0;
                        self.last_heartbeat_at = Some(SystemTime::now());
                        info!(
                            "received ledger from captive core: {:?}",
                            self.last_processed_ledger
                        );
                        return Ok(IngestedLedger {
                            payload,
                            raw_xdr: frame,
                            ledger_sequence,
                            source: IngestionSource::CaptiveCore,
                        });
                    }
                    Err(error) => {
                        self.handle_primary_failure(error.to_string()).await?;
                        continue;
                    }
                }
            }

            if Instant::now() >= self.next_primary_retry_at {
                let rendered = self.config.rendered_toml();
                match self
                    .primary
                    .start(self.next_start_ledger(), &rendered)
                    .await
                {
                    Ok(()) => {
                        info!(
                            "captive core recovered, resuming primary ingest from ledger {}",
                            self.next_start_ledger()
                        );
                        self.active_source = IngestionSource::CaptiveCore;
                        self.fallback_reason = None;
                        continue;
                    }
                    Err(error) => {
                        warn!("captive core restart failed, keeping fallback active: {error}");
                        self.fallback_reason = Some(error.to_string());
                        self.next_primary_retry_at = Instant::now() + self.config.restart_backoff;
                    }
                }
            }

            let next_ledger = self.next_start_ledger();
            let mut ledger = self.fallback.next_from(next_ledger).await?;
            if let Some(sequence) = ledger.ledger_sequence {
                self.last_processed_ledger = Some(sequence);
            }
            ledger.source = IngestionSource::Remote;
            self.active_source = IngestionSource::Remote;
            self.last_heartbeat_at = Some(SystemTime::now());
            info!(
                "using remote ingestion fallback from ledger {:?}",
                ledger.ledger_sequence.or(self.last_processed_ledger)
            );
            return Ok(ledger);
        }
    }

    async fn handle_primary_failure(&mut self, reason: String) -> Result<(), SdkError> {
        warn!("captive core failure detected: {reason}");
        self.fallback_reason = Some(reason.clone());
        self.active_source = IngestionSource::Remote;

        let _ = self.primary.stop().await;
        self.last_archive_path = self
            .primary
            .archive_state(&self.config.state_archive_dir)
            .await?;

        let rendered = self.config.rendered_toml();
        while self.consecutive_restart_failures < self.config.max_restart_attempts {
            self.consecutive_restart_failures += 1;
            let backoff = self.restart_delay(self.consecutive_restart_failures);
            info!(
                "attempting captive core restart {} from ledger {} after {:?}",
                self.consecutive_restart_failures,
                self.next_start_ledger(),
                backoff
            );
            tokio::time::sleep(backoff).await;
            match self
                .primary
                .start(self.next_start_ledger(), &rendered)
                .await
            {
                Ok(()) => {
                    info!(
                        "captive core restart succeeded on attempt {}",
                        self.consecutive_restart_failures
                    );
                    self.active_source = IngestionSource::CaptiveCore;
                    self.fallback_reason = None;
                    return Ok(());
                }
                Err(error) => {
                    error!(
                        "captive core restart attempt {} failed: {}",
                        self.consecutive_restart_failures, error
                    );
                    self.fallback_reason = Some(error.to_string());
                }
            }
        }

        self.next_primary_retry_at = Instant::now() + self.config.restart_backoff;
        Ok(())
    }

    fn next_start_ledger(&self) -> u32 {
        self.last_processed_ledger
            .map(|ledger| ledger.saturating_add(1))
            .unwrap_or(self.config.start_ledger)
    }

    fn restart_delay(&self, attempt: u32) -> Duration {
        let multiplier = 1u32
            .checked_shl(attempt.saturating_sub(1).min(8))
            .unwrap_or(256);
        self.config
            .restart_backoff
            .checked_mul(multiplier)
            .unwrap_or(self.config.restart_backoff)
    }
}

#[derive(Debug)]
enum CaptiveCoreReader {
    Stdout(BufReader<ChildStdout>),
    Socket(BufReader<TcpStream>),
}

impl CaptiveCoreReader {
    async fn next_frame(&mut self, timeout: Duration) -> Result<Vec<u8>, SdkError> {
        match self {
            Self::Stdout(reader) => {
                tokio::time::timeout(timeout, read_length_prefixed_frame(reader))
                    .await
                    .map_err(|_| {
                        SdkError::Ingestion(format!(
                            "captive core heartbeat timed out after {} ms",
                            timeout.as_millis()
                        ))
                    })?
            }
            Self::Socket(reader) => {
                tokio::time::timeout(timeout, read_length_prefixed_frame(reader))
                    .await
                    .map_err(|_| {
                        SdkError::Ingestion(format!(
                            "captive core heartbeat timed out after {} ms",
                            timeout.as_millis()
                        ))
                    })?
            }
        }
    }
}

#[derive(Debug)]
pub struct TokioCaptiveCoreBackend {
    config: CaptiveCoreConfig,
    child: Option<Child>,
    reader: Option<CaptiveCoreReader>,
    current_config_path: Option<PathBuf>,
    stderr_task: Option<JoinHandle<()>>,
}

impl TokioCaptiveCoreBackend {
    pub fn new(config: CaptiveCoreConfig) -> Self {
        Self {
            config,
            child: None,
            reader: None,
            current_config_path: None,
            stderr_task: None,
        }
    }

    async fn write_runtime_config(
        &mut self,
        start_ledger: u32,
        rendered_toml: &str,
    ) -> Result<PathBuf, SdkError> {
        fs::create_dir_all(&self.config.working_dir)
            .await
            .map_err(|e| SdkError::Ingestion(format!("failed to create working dir: {e}")))?;
        let config_path = self
            .config
            .working_dir
            .join(format!("stellar-core-{start_ledger}.cfg.toml"));
        fs::write(&config_path, rendered_toml).await.map_err(|e| {
            SdkError::Ingestion(format!("failed to write captive core config: {e}"))
        })?;
        Ok(config_path)
    }

    async fn attach_reader(
        &self,
        child_stdout: Option<ChildStdout>,
    ) -> Result<CaptiveCoreReader, SdkError> {
        match self.config.output_target {
            CaptiveCoreOutputTarget::Stdout => {
                let stdout = child_stdout.ok_or_else(|| {
                    SdkError::Ingestion("captive core stdout pipe unavailable".into())
                })?;
                Ok(CaptiveCoreReader::Stdout(BufReader::new(stdout)))
            }
            CaptiveCoreOutputTarget::TcpSocket(address) => {
                let connect = async {
                    loop {
                        match TcpStream::connect(address).await {
                            Ok(stream) => return Ok(stream),
                            Err(error) => {
                                tokio::time::sleep(Duration::from_millis(200)).await;
                                if !matches!(error.kind(), std::io::ErrorKind::ConnectionRefused) {
                                    return Err(error);
                                }
                            }
                        }
                    }
                };
                let stream = tokio::time::timeout(self.config.heartbeat_timeout, connect)
                    .await
                    .map_err(|_| {
                        SdkError::Ingestion(format!(
                            "timed out connecting to captive core metadata socket {address}"
                        ))
                    })?
                    .map_err(|e| {
                        SdkError::Ingestion(format!(
                            "failed to connect to captive core metadata socket {address}: {e}"
                        ))
                    })?;
                Ok(CaptiveCoreReader::Socket(BufReader::new(stream)))
            }
        }
    }

    fn spawn_stderr_logger(stderr: ChildStderr) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!("captive core stderr: {line}");
            }
        })
    }
}

#[async_trait]
impl CaptiveCoreBackend for TokioCaptiveCoreBackend {
    async fn start(&mut self, start_ledger: u32, rendered_toml: &str) -> Result<(), SdkError> {
        if self.is_running().await {
            self.stop().await?;
        }

        let config_path = self
            .write_runtime_config(start_ledger, rendered_toml)
            .await?;
        let args = self.config.command_line(&config_path, start_ledger);
        let mut command = Command::new(&self.config.binary_path);
        command.args(&args);
        command.current_dir(&self.config.working_dir);
        if self.config.output_target == CaptiveCoreOutputTarget::Stdout {
            command.stdout(std::process::Stdio::piped());
        } else {
            command.stdout(std::process::Stdio::null());
        }
        command.stderr(std::process::Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|e| SdkError::Ingestion(format!("failed to spawn captive core: {e}")))?;
        let stderr = child.stderr.take();
        let reader = self.attach_reader(child.stdout.take()).await?;

        self.current_config_path = Some(config_path);
        self.reader = Some(reader);
        self.stderr_task = stderr.map(Self::spawn_stderr_logger);
        self.child = Some(child);
        Ok(())
    }

    async fn next_frame(&mut self, timeout: Duration) -> Result<Vec<u8>, SdkError> {
        if let Some(child) = self.child.as_mut() {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| SdkError::Ingestion(format!("failed to inspect captive core: {e}")))?
            {
                self.reader = None;
                self.child = None;
                return Err(SdkError::Ingestion(format!(
                    "captive core exited before next frame: {status}"
                )));
            }
        }

        let reader = self
            .reader
            .as_mut()
            .ok_or_else(|| SdkError::Ingestion("captive core stream is not attached".into()))?;
        reader.next_frame(timeout).await
    }

    async fn archive_state(&mut self, archive_root: &Path) -> Result<Option<PathBuf>, SdkError> {
        if !self.config.working_dir.exists() {
            return Ok(None);
        }

        fs::create_dir_all(archive_root)
            .await
            .map_err(|e| SdkError::Ingestion(format!("failed to create archive root: {e}")))?;
        let archive_dir = archive_root.join(format!("archive-{}", unix_timestamp_millis()));
        copy_directory(&self.config.working_dir, &archive_dir).await?;
        Ok(Some(archive_dir))
    }

    async fn stop(&mut self) -> Result<(), SdkError> {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        self.child = None;
        self.reader = None;
        if let Some(task) = self.stderr_task.take() {
            task.abort();
        }
        Ok(())
    }

    async fn is_running(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) | Err(_) => false,
            },
            None => false,
        }
    }
}

pub async fn read_length_prefixed_frame<R>(reader: &mut R) -> Result<Vec<u8>, SdkError>
where
    R: AsyncRead + Unpin + Send,
{
    let mut header = [0u8; 4];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|e| SdkError::Ingestion(format!("failed to read XDR frame header: {e}")))?;
    let length = u32::from_be_bytes(header) as usize;
    let mut payload = vec![0u8; length];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|e| SdkError::Ingestion(format!("failed to read XDR frame payload: {e}")))?;
    Ok(payload)
}

pub fn decode_ledger_close_meta_xdr(frame: &[u8]) -> Result<LedgerCloseMeta, SdkError> {
    LedgerCloseMeta::from_xdr(frame, Limits::none())
        .map_err(|e| SdkError::Ingestion(format!("failed to decode LedgerCloseMeta XDR: {e}")))
}

pub struct RpcLedgersRemoteSource<T> {
    client: StellarRpcClient,
    xdr_format: Option<String>,
    mapper: Arc<dyn Fn(Ledger) -> Result<IngestedLedger<T>, SdkError> + Send + Sync>,
}

impl<T> RpcLedgersRemoteSource<T> {
    pub fn new(
        rpc_url: &str,
        xdr_format: Option<String>,
        mapper: impl Fn(Ledger) -> Result<IngestedLedger<T>, SdkError> + Send + Sync + 'static,
    ) -> Result<Self, SdkError> {
        let client = StellarRpcClient::new(rpc_url).map_err(|e| {
            SdkError::Ingestion(format!("failed to create RPC fallback client: {e}"))
        })?;
        Ok(Self {
            client,
            xdr_format,
            mapper: Arc::new(mapper),
        })
    }

    fn map_ledger(&self, ledger: Ledger) -> Result<IngestedLedger<T>, SdkError> {
        (self.mapper)(ledger)
    }
}

#[async_trait]
impl<T: Send> RemoteLedgerSource<T> for RpcLedgersRemoteSource<T> {
    async fn next_from(&mut self, start_ledger: u32) -> Result<IngestedLedger<T>, SdkError> {
        let response = self
            .client
            .get_ledgers(
                LedgerStart::Ledger(start_ledger),
                Some(1),
                self.xdr_format.clone(),
            )
            .await
            .map_err(|e| {
                SdkError::Ingestion(format!(
                    "failed to fetch ledger {start_ledger} from RPC fallback: {e}"
                ))
            })?;
        let ledger = response.ledgers.into_iter().next().ok_or_else(|| {
            SdkError::Ingestion(format!(
                "RPC fallback returned no ledgers starting at {start_ledger}"
            ))
        })?;
        (self.mapper)(ledger)
    }
}

pub struct RpcLedgerCloseMetaRemoteSource {
    inner: RpcLedgersRemoteSource<LedgerCloseMeta>,
}

impl RpcLedgerCloseMetaRemoteSource {
    pub fn new(rpc_url: &str) -> Result<Self, SdkError> {
        let inner =
            RpcLedgersRemoteSource::new(rpc_url, Some("json".to_string()), |ledger: Ledger| {
                if let Some(metadata) = ledger.metadata_json {
                    let raw_xdr = metadata.to_xdr(Limits::none()).map_err(|e| {
                        SdkError::Ingestion(format!(
                            "failed to serialize RPC metadata for ledger {}: {e}",
                            ledger.sequence
                        ))
                    })?;
                    return Ok(IngestedLedger {
                        payload: metadata,
                        raw_xdr,
                        ledger_sequence: Some(ledger.sequence),
                        source: IngestionSource::Remote,
                    });
                }

                let metadata =
                    LedgerCloseMeta::from_xdr_base64(&ledger.metadata_xdr, Limits::none())
                        .map_err(|e| {
                            SdkError::Ingestion(format!(
                                "failed to decode RPC fallback metadata XDR for ledger {}: {e}",
                                ledger.sequence
                            ))
                        })?;
                let raw_xdr = metadata.to_xdr(Limits::none()).map_err(|e| {
                    SdkError::Ingestion(format!(
                        "failed to re-encode RPC fallback metadata for ledger {}: {e}",
                        ledger.sequence
                    ))
                })?;
                Ok(IngestedLedger {
                    payload: metadata,
                    raw_xdr,
                    ledger_sequence: Some(ledger.sequence),
                    source: IngestionSource::Remote,
                })
            })?;
        Ok(Self { inner })
    }
}

#[async_trait]
impl RemoteLedgerSource<LedgerCloseMeta> for RpcLedgerCloseMetaRemoteSource {
    async fn next_from(
        &mut self,
        start_ledger: u32,
    ) -> Result<IngestedLedger<LedgerCloseMeta>, SdkError> {
        self.inner.next_from(start_ledger).await
    }
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\"', "\\\"")
}

fn unix_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

async fn copy_directory(source: &Path, destination: &Path) -> Result<(), SdkError> {
    fs::create_dir_all(destination)
        .await
        .map_err(|e| SdkError::Ingestion(format!("failed to create archive directory: {e}")))?;

    let mut stack = VecDeque::from([(source.to_path_buf(), destination.to_path_buf())]);
    while let Some((src_dir, dst_dir)) = stack.pop_front() {
        fs::create_dir_all(&dst_dir)
            .await
            .map_err(|e| SdkError::Ingestion(format!("failed to create directory: {e}")))?;
        let mut entries = fs::read_dir(&src_dir)
            .await
            .map_err(|e| SdkError::Ingestion(format!("failed to read directory: {e}")))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| SdkError::Ingestion(format!("failed to read directory entry: {e}")))?
        {
            let source_path = entry.path();
            let destination_path = dst_dir.join(entry.file_name());
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| SdkError::Ingestion(format!("failed to inspect entry: {e}")))?;
            if metadata.is_dir() {
                stack.push_back((source_path, destination_path));
            } else if metadata.is_file() {
                fs::copy(&source_path, &destination_path)
                    .await
                    .map_err(|e| {
                        SdkError::Ingestion(format!(
                            "failed to copy '{}' into archive: {e}",
                            source_path.display()
                        ))
                    })?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct FakeLedger {
        sequence: u32,
    }

    struct FakeRemoteSource {
        ledgers: VecDeque<IngestedLedger<FakeLedger>>,
    }

    #[async_trait]
    impl RemoteLedgerSource<FakeLedger> for FakeRemoteSource {
        async fn next_from(
            &mut self,
            _start_ledger: u32,
        ) -> Result<IngestedLedger<FakeLedger>, SdkError> {
            self.ledgers
                .pop_front()
                .ok_or_else(|| SdkError::Ingestion("no remote ledgers queued".into()))
        }
    }

    struct FakeCaptiveCoreBackend {
        epochs: VecDeque<VecDeque<Result<Vec<u8>, SdkError>>>,
        active: VecDeque<Result<Vec<u8>, SdkError>>,
        running: bool,
        archive_count: usize,
        start_ledgers: Vec<u32>,
    }

    impl FakeCaptiveCoreBackend {
        fn new(epochs: Vec<Vec<Result<Vec<u8>, SdkError>>>) -> Self {
            Self {
                epochs: epochs.into_iter().map(VecDeque::from).collect(),
                active: VecDeque::new(),
                running: false,
                archive_count: 0,
                start_ledgers: Vec::new(),
            }
        }
    }

    #[async_trait]
    impl CaptiveCoreBackend for FakeCaptiveCoreBackend {
        async fn start(&mut self, start_ledger: u32, _rendered_toml: &str) -> Result<(), SdkError> {
            self.start_ledgers.push(start_ledger);
            self.active = self.epochs.pop_front().unwrap_or_default();
            self.running = true;
            Ok(())
        }

        async fn next_frame(&mut self, _timeout: Duration) -> Result<Vec<u8>, SdkError> {
            match self.active.pop_front() {
                Some(Ok(frame)) => Ok(frame),
                Some(Err(error)) => {
                    self.running = false;
                    Err(error)
                }
                None => {
                    self.running = false;
                    Err(SdkError::Ingestion("manual termination".into()))
                }
            }
        }

        async fn archive_state(
            &mut self,
            archive_root: &Path,
        ) -> Result<Option<PathBuf>, SdkError> {
            self.archive_count += 1;
            Ok(Some(
                archive_root.join(format!("archive-{}", self.archive_count)),
            ))
        }

        async fn stop(&mut self) -> Result<(), SdkError> {
            self.running = false;
            Ok(())
        }

        async fn is_running(&mut self) -> bool {
            self.running
        }
    }

    fn fake_frame(sequence: u32) -> Vec<u8> {
        sequence.to_be_bytes().to_vec()
    }

    #[tokio::test]
    async fn framed_reader_honours_four_byte_length_prefix() {
        let (mut writer, mut reader) = tokio::io::duplex(64);
        tokio::spawn(async move {
            let payload = b"ledger-xdr";
            writer
                .write_all(&(payload.len() as u32).to_be_bytes())
                .await
                .unwrap();
            writer.write_all(payload).await.unwrap();
        });

        let frame = read_length_prefixed_frame(&mut reader).await.unwrap();
        assert_eq!(frame, b"ledger-xdr");
    }

    #[test]
    fn rendered_toml_includes_passphrase_and_history_archives() {
        let config = CaptiveCoreConfig {
            network_passphrase: "Standalone Network ; February 2017".into(),
            history_archive_urls: vec![
                "https://history-1.example.com".into(),
                "https://history-2.example.com".into(),
            ],
            ..CaptiveCoreConfig::default()
        };

        let toml = config.rendered_toml();
        assert!(toml.contains("NETWORK_PASSPHRASE=\"Standalone Network ; February 2017\""));
        assert!(toml.contains("[HISTORY.archive_0]"));
        assert!(toml.contains("https://history-2.example.com"));
    }

    #[tokio::test]
    async fn supervisor_restarts_from_last_processed_ledger() {
        let backend = FakeCaptiveCoreBackend::new(vec![
            vec![
                Ok(fake_frame(41)),
                Err(SdkError::Ingestion("process exited".into())),
            ],
            vec![Ok(fake_frame(42))],
        ]);
        let remote = FakeRemoteSource {
            ledgers: VecDeque::new(),
        };
        let mut ingestor = CaptiveCoreIngestor::new(
            CaptiveCoreConfig {
                start_ledger: 41,
                ..CaptiveCoreConfig::default()
            },
            backend,
            remote,
            |frame| {
                let sequence = u32::from_be_bytes(frame.try_into().unwrap());
                Ok(FakeLedger { sequence })
            },
            |ledger| Some(ledger.sequence),
        );

        ingestor.start().await.unwrap();
        let first = ingestor.next_ledger().await.unwrap();
        assert_eq!(first.ledger_sequence, Some(41));

        let second = ingestor.next_ledger().await.unwrap();
        assert_eq!(second.ledger_sequence, Some(42));

        let status = ingestor.status();
        assert_eq!(status.active_source, IngestionSource::CaptiveCore);
        assert_eq!(status.last_processed_ledger, Some(42));
        assert!(status.last_archive_path.is_some());
    }

    #[tokio::test]
    async fn supervisor_falls_back_to_remote_after_manual_termination() {
        let backend = FakeCaptiveCoreBackend::new(vec![vec![Err(SdkError::Ingestion(
            "manual termination".into(),
        ))]]);
        let remote = FakeRemoteSource {
            ledgers: VecDeque::from([IngestedLedger {
                payload: FakeLedger { sequence: 99 },
                raw_xdr: fake_frame(99),
                ledger_sequence: Some(99),
                source: IngestionSource::Remote,
            }]),
        };
        let mut ingestor = CaptiveCoreIngestor::new(
            CaptiveCoreConfig {
                start_ledger: 88,
                max_restart_attempts: 1,
                ..CaptiveCoreConfig::default()
            },
            backend,
            remote,
            |frame| {
                let sequence = u32::from_be_bytes(frame.try_into().unwrap());
                Ok(FakeLedger { sequence })
            },
            |ledger| Some(ledger.sequence),
        );

        ingestor.start().await.unwrap();
        let ledger = ingestor.next_ledger().await.unwrap();

        assert_eq!(ledger.source, IngestionSource::Remote);
        assert_eq!(ledger.ledger_sequence, Some(99));

        let status = ingestor.status();
        assert_eq!(status.active_source, IngestionSource::Remote);
        assert!(status.fallback_reason.is_some());
    }

    #[test]
    fn command_line_renders_socket_metadata_stream_target() {
        let config = CaptiveCoreConfig {
            output_target: CaptiveCoreOutputTarget::TcpSocket(
                "127.0.0.1:11626".parse().expect("valid socket address"),
            ),
            ..CaptiveCoreConfig::default()
        };

        let args = config.command_line(Path::new("stellar-core.cfg"), 44);
        assert!(args.iter().any(|arg| arg == "127.0.0.1:11626"));
    }

    #[test]
    fn rpc_remote_source_maps_returned_ledger() {
        let source = RpcLedgersRemoteSource::new("http://127.0.0.1:8000", None, |ledger| {
            Ok(IngestedLedger {
                payload: ledger.sequence,
                raw_xdr: Vec::new(),
                ledger_sequence: Some(ledger.sequence),
                source: IngestionSource::Remote,
            })
        })
        .unwrap();

        let ledger = source
            .map_ledger(Ledger {
                hash: "abc".to_string(),
                sequence: 55,
                ledger_close_time: "1710000000".to_string(),
                header_xdr: String::new(),
                header_json: None,
                metadata_xdr: String::new(),
                metadata_json: None,
            })
            .unwrap();
        assert_eq!(ledger.payload, 55);
        assert_eq!(ledger.ledger_sequence, Some(55));
        assert_eq!(ledger.source, IngestionSource::Remote);
    }
}
