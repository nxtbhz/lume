use std::fmt;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError};
use serde_json::{json, Value};

use crate::lsp::protocol::{path_to_uri, read_message, write_message};
use crate::lsp::{LspError, PositionEncoding, ServerKind};

pub const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(500);
const GRACEFUL_EXIT_POLL_TIMEOUT: Duration = Duration::from_millis(100);

const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
const JSONRPC_REQUEST_CANCELLED: i64 = -32800;
const JSONRPC_CONTENT_MODIFIED: i64 = -32801;

/// Normalized LSP `textDocumentSync` change mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDocumentSyncKind {
    None,
    Full,
    Incremental,
}

impl TextDocumentSyncKind {
    fn from_u64(v: u64) -> Option<Self> {
        match v {
            0 => Some(Self::None),
            1 => Some(Self::Full),
            2 => Some(Self::Incremental),
            _ => None,
        }
    }
}

/// Capabilities extracted from the server's `initialize` response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerCapabilities {
    pub position_encoding: PositionEncoding,
    pub definition_provider: bool,
    pub open_close: bool,
    pub sync_kind: TextDocumentSyncKind,
}

#[derive(Debug)]
pub(crate) enum IncomingMessage {
    Message(Value),
    Eof,
    ProtocolError(String),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WaitError {
    Timeout,
    Disconnected,
    Cancelled,
}

pub(crate) trait SessionClock: Send {
    fn now(&self) -> Duration;
    fn recv_until(
        &self,
        rx: &Receiver<IncomingMessage>,
        deadline: Duration,
        cancellable: bool,
    ) -> Result<IncomingMessage, WaitError>;
}

pub(crate) struct RealClock {
    origin: Instant,
    cancel: Option<Receiver<()>>,
}

impl RealClock {
    pub(crate) fn with_cancel(cancel: Receiver<()>) -> Self {
        Self {
            origin: Instant::now(),
            cancel: Some(cancel),
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|c| !matches!(c.try_recv(), Err(crossbeam_channel::TryRecvError::Empty)))
    }
}

impl SessionClock for RealClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }

    fn recv_until(
        &self,
        rx: &Receiver<IncomingMessage>,
        deadline: Duration,
        cancellable: bool,
    ) -> Result<IncomingMessage, WaitError> {
        if cancellable && self.is_cancelled() {
            return Err(WaitError::Cancelled);
        }

        let now = self.now();
        if now >= deadline {
            return match rx.try_recv() {
                Ok(msg) => Ok(msg),
                Err(crossbeam_channel::TryRecvError::Empty) => Err(WaitError::Timeout),
                Err(crossbeam_channel::TryRecvError::Disconnected) => Err(WaitError::Disconnected),
            };
        }

        let remaining = deadline - now;
        if cancellable {
            if let Some(ref cancel_rx) = self.cancel {
                let timer = crossbeam_channel::after(remaining);
                crossbeam_channel::select! {
                    recv(cancel_rx) -> _ => return Err(WaitError::Cancelled),
                    recv(rx) -> res => {
                        return res.map_err(|_| WaitError::Disconnected);
                    }
                    recv(timer) -> _ => return Err(WaitError::Timeout),
                }
            }
        }

        match rx.recv_timeout(remaining) {
            Ok(msg) => Ok(msg),
            Err(RecvTimeoutError::Timeout) => Err(WaitError::Timeout),
            Err(RecvTimeoutError::Disconnected) => Err(WaitError::Disconnected),
        }
    }
}

pub(crate) trait ProcessHandle: Send {
    fn terminate_and_reap(&mut self);

    fn graceful_reap(&mut self, _timeout: Duration) {
        self.terminate_and_reap();
    }
}

struct ChildProcessHandle {
    child: Option<Child>,
}

impl ChildProcessHandle {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }
}

impl ProcessHandle for ChildProcessHandle {
    fn terminate_and_reap(&mut self) {
        if let Some(mut child) = self.child.take() {
            match child.try_wait() {
                Ok(Some(_)) => {}
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }

    fn graceful_reap(&mut self, timeout: Duration) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
            }
        }
    }
}

/// Active stdio JSON-RPC session with a single language server process.
pub struct LspSession {
    writer: Option<Box<dyn Write + Send>>,
    incoming: Receiver<IncomingMessage>,
    process: Box<dyn ProcessHandle>,
    clock: Box<dyn SessionClock>,
    capabilities: ServerCapabilities,
    next_id: i64,
    alive: bool,
}

impl fmt::Debug for LspSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LspSession")
            .field("capabilities", &self.capabilities)
            .field("next_id", &self.next_id)
            .field("alive", &self.alive)
            .finish()
    }
}

impl Drop for LspSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl LspSession {
    /// Spawns `kind` in `root` with piped stdio, cooperative cancellation, and `initialize`.
    pub(crate) fn spawn_with_cancel(
        kind: ServerKind,
        root: &Path,
        cancel: Receiver<()>,
    ) -> Result<Self, LspError> {
        let mut child = Command::new(kind.executable_name())
            .args(kind.args())
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    LspError::MissingExecutable(kind)
                } else {
                    LspError::Protocol(format!("failed to spawn {}: {e}", kind.display_name()))
                }
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            LspError::Protocol("failed to capture server stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            LspError::Protocol("failed to capture server stdout".to_string())
        })?;
        if let Some(mut stderr) = child.stderr.take() {
            // Drain stderr away from the TUI so the server never blocks on a full pipe.
            let _ = std::thread::Builder::new()
                .name(format!("lume-lsp-{}-stderr", kind.executable_name()))
                .spawn(move || {
                    let _ = io::copy(&mut stderr, &mut io::sink());
                });
        }

        let (tx, rx) = crossbeam_channel::unbounded();
        let _ = std::thread::Builder::new()
            .name(format!("lume-lsp-{}-stdout", kind.executable_name()))
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    match read_message(&mut reader) {
                        Ok(Some(msg)) => {
                            if tx.send(IncomingMessage::Message(msg)).is_err() {
                                break;
                            }
                        }
                        Ok(None) => {
                            let _ = tx.send(IncomingMessage::Eof);
                            break;
                        }
                        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                            let _ = tx.send(IncomingMessage::ProtocolError(e.to_string()));
                            break;
                        }
                        Err(_) => {
                            let _ = tx.send(IncomingMessage::Eof);
                            break;
                        }
                    }
                }
            });

        Self::from_parts(
            Box::new(BufWriter::new(stdin)),
            rx,
            Box::new(ChildProcessHandle::new(child)),
            Box::new(RealClock::with_cancel(cancel)),
            root,
        )
    }

    pub(crate) fn from_parts(
        writer: Box<dyn Write + Send>,
        incoming: Receiver<IncomingMessage>,
        process: Box<dyn ProcessHandle>,
        clock: Box<dyn SessionClock>,
        root: &Path,
    ) -> Result<Self, LspError> {
        let mut session = Self {
            writer: Some(writer),
            incoming,
            process,
            clock,
            capabilities: ServerCapabilities {
                position_encoding: PositionEncoding::Utf16,
                definition_provider: false,
                open_close: false,
                sync_kind: TextDocumentSyncKind::None,
            },
            next_id: 1,
            alive: true,
        };

        let root_uri = match path_to_uri(root) {
            Ok(uri) => uri,
            Err(err) => {
                session.terminate_fatal();
                return Err(err);
            }
        };
        let folder_name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");

        let init_params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "workspaceFolders": [{
                "uri": root_uri,
                "name": folder_name,
            }],
            "capabilities": {
                "general": {
                    "positionEncodings": ["utf-8", "utf-16", "utf-32"]
                },
                "textDocument": {
                    "definition": {
                        "linkSupport": true
                    },
                    "synchronization": {
                        "dynamicRegistration": false,
                        "willSave": false,
                        "willSaveWaitUntil": false,
                        "didSave": false
                    }
                }
            }
        });

        let init_result =
            match session.request_with_timeout("initialize", init_params, INITIALIZE_TIMEOUT, true)
            {
                Ok(res) => res,
                Err(err) => {
                    session.terminate_fatal();
                    return Err(err);
                }
            };

        session.capabilities = match parse_server_capabilities(&init_result) {
            Ok(caps) => caps,
            Err(err) => {
                session.terminate_fatal();
                return Err(err);
            }
        };

        if let Err(err) = session.notify("initialized", json!({})) {
            session.terminate_fatal();
            return Err(err);
        }

        Ok(session)
    }

    pub fn capabilities(&self) -> ServerCapabilities {
        self.capabilities
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Sends a JSON-RPC notification without waiting for a response.
    pub fn notify(&mut self, method: &str, params: Value) -> Result<(), LspError> {
        if !self.alive {
            return Err(LspError::Exited);
        }
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        if self.send_raw(&msg).is_err() {
            self.terminate_fatal();
            return Err(LspError::Exited);
        }
        Ok(())
    }

    /// Sends a JSON-RPC request and waits up to [`REQUEST_TIMEOUT`] for its response.
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, LspError> {
        self.request_with_timeout(method, params, REQUEST_TIMEOUT, true)
    }

    fn request_with_timeout(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        cancellable: bool,
    ) -> Result<Value, LspError> {
        if !self.alive {
            return Err(LspError::Exited);
        }
        let id = self.next_id;
        self.next_id += 1;

        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if self.send_raw(&msg).is_err() {
            self.terminate_fatal();
            return Err(LspError::Exited);
        }

        let deadline = self.clock.now().saturating_add(timeout);
        loop {
            let incoming = match self.clock.recv_until(&self.incoming, deadline, cancellable) {
                Ok(m) => m,
                Err(WaitError::Timeout) => {
                    self.terminate_fatal();
                    return Err(LspError::Timeout);
                }
                Err(WaitError::Disconnected | WaitError::Cancelled) => {
                    self.terminate_fatal();
                    return Err(LspError::Exited);
                }
            };

            let msg = match incoming {
                IncomingMessage::Message(v) => v,
                IncomingMessage::Eof => {
                    self.terminate_fatal();
                    return Err(LspError::Exited);
                }
                IncomingMessage::ProtocolError(err) => {
                    self.terminate_fatal();
                    return Err(LspError::Protocol(err));
                }
            };

            let Some(obj) = msg.as_object() else {
                continue;
            };

            let has_method = obj.get("method").and_then(Value::as_str).is_some();
            let msg_id = obj.get("id");

            if has_method && msg_id.is_some() {
                // Server-to-client request: reply -32601 (Method not found) and continue waiting.
                let reply = json!({
                    "jsonrpc": "2.0",
                    "id": msg_id.cloned().unwrap_or(Value::Null),
                    "error": {
                        "code": JSONRPC_METHOD_NOT_FOUND,
                        "message": "Method not supported",
                    }
                });
                if self.send_raw(&reply).is_err() {
                    self.terminate_fatal();
                    return Err(LspError::Exited);
                }
                continue;
            }

            if has_method {
                // Server notification: ignore.
                continue;
            }

            if let Some(resp_id) = msg_id {
                if resp_id.as_i64() == Some(id) {
                    if let Some(err) = obj.get("error") {
                        let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
                        if code == JSONRPC_CONTENT_MODIFIED || code == JSONRPC_REQUEST_CANCELLED {
                            return Err(LspError::ContentModified);
                        }
                        let err_msg = err
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown LSP error");
                        return Err(LspError::Protocol(format!("LSP error {code}: {err_msg}")));
                    }
                    return Ok(obj.get("result").cloned().unwrap_or(Value::Null));
                }
                // Response to an unknown or already-abandoned request ID: ignore.
            }
        }
    }

    /// Gracefully shuts down the server (`shutdown` within 500 ms + `exit`) and reaps the process.
    pub fn shutdown(&mut self) {
        if !self.alive {
            self.writer.take();
            self.process.terminate_and_reap();
            return;
        }
        let shutdown_ok = self
            .request_with_timeout("shutdown", Value::Null, SHUTDOWN_TIMEOUT, false)
            .is_ok();
        if shutdown_ok {
            let _ = self.notify("exit", Value::Null);
        }
        self.alive = false;
        self.writer.take();
        if shutdown_ok {
            self.process.graceful_reap(GRACEFUL_EXIT_POLL_TIMEOUT);
        } else {
            self.process.terminate_and_reap();
        }
    }

    fn send_raw(&mut self, msg: &Value) -> io::Result<()> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "LSP stdin closed"))?;
        write_message(writer, msg)
    }

    fn terminate_fatal(&mut self) {
        self.alive = false;
        self.writer.take();
        self.process.terminate_and_reap();
    }
}

fn parse_server_capabilities(init_result: &Value) -> Result<ServerCapabilities, LspError> {
    let caps = init_result
        .get("capabilities")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            LspError::Protocol("initialize response missing capabilities object".to_string())
        })?;

    let position_encoding = match caps.get("positionEncoding") {
        None | Some(Value::Null) => PositionEncoding::Utf16,
        Some(Value::String(s)) => match s.as_str() {
            "utf-8" => PositionEncoding::Utf8,
            "utf-16" => PositionEncoding::Utf16,
            "utf-32" => PositionEncoding::Utf32,
            other => {
                return Err(LspError::Protocol(format!(
                    "unsupported server positionEncoding '{other}'"
                )));
            }
        },
        Some(_) => {
            return Err(LspError::Protocol(
                "invalid positionEncoding type in server capabilities".to_string(),
            ));
        }
    };

    let definition_provider = match caps.get("definitionProvider") {
        Some(Value::Bool(b)) => *b,
        Some(Value::Object(_)) => true,
        None | Some(Value::Null) => false,
        Some(_) => {
            return Err(LspError::Protocol(
                "invalid definitionProvider type in server capabilities".to_string(),
            ));
        }
    };

    let (open_close, sync_kind) = match caps.get("textDocumentSync") {
        None | Some(Value::Null) => (false, TextDocumentSyncKind::None),
        Some(Value::Number(num)) => {
            let kind = num
                .as_u64()
                .and_then(TextDocumentSyncKind::from_u64)
                .ok_or_else(|| {
                    LspError::Protocol("invalid numeric textDocumentSync value".to_string())
                })?;
            (kind != TextDocumentSyncKind::None, kind)
        }
        Some(Value::Object(obj)) => {
            let change = match obj.get("change") {
                None | Some(Value::Null) => TextDocumentSyncKind::None,
                Some(Value::Number(num)) => num
                    .as_u64()
                    .and_then(TextDocumentSyncKind::from_u64)
                    .ok_or_else(|| {
                    LspError::Protocol("invalid textDocumentSync.change value".to_string())
                })?,
                Some(_) => {
                    return Err(LspError::Protocol(
                        "invalid textDocumentSync.change type".to_string(),
                    ));
                }
            };
            let open_close = match obj.get("openClose") {
                Some(Value::Bool(b)) => *b,
                None | Some(Value::Null) => change != TextDocumentSyncKind::None,
                Some(_) => {
                    return Err(LspError::Protocol(
                        "invalid textDocumentSync.openClose type".to_string(),
                    ));
                }
            };
            (open_close, change)
        }
        Some(_) => {
            return Err(LspError::Protocol(
                "invalid textDocumentSync type in server capabilities".to_string(),
            ));
        }
    };

    Ok(ServerCapabilities {
        position_encoding,
        definition_provider,
        open_close,
        sync_kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::protocol::read_message;
    use serde_json::json;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FakeProcess(Arc<AtomicBool>);

    impl ProcessHandle for FakeProcess {
        fn terminate_and_reap(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    struct FakeClock {
        now_ms: Arc<AtomicU64>,
    }

    impl FakeClock {
        fn new() -> Self {
            Self {
                now_ms: Arc::new(AtomicU64::new(0)),
            }
        }
    }

    impl SessionClock for FakeClock {
        fn now(&self) -> Duration {
            Duration::from_millis(self.now_ms.load(Ordering::SeqCst))
        }

        fn recv_until(
            &self,
            rx: &Receiver<IncomingMessage>,
            deadline: Duration,
            _cancellable: bool,
        ) -> Result<IncomingMessage, WaitError> {
            match rx.try_recv() {
                Ok(msg) => Ok(msg),
                Err(crossbeam_channel::TryRecvError::Disconnected) => Err(WaitError::Disconnected),
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    self.now_ms
                        .store(deadline.as_millis() as u64, Ordering::SeqCst);
                    Err(WaitError::Timeout)
                }
            }
        }
    }

    fn drain_sent_messages(buf: &Arc<Mutex<Vec<u8>>>) -> Vec<Value> {
        let bytes = buf.lock().unwrap().clone();
        let mut cursor = Cursor::new(bytes);
        let mut out = Vec::new();
        while let Some(msg) = read_message(&mut cursor).unwrap() {
            out.push(msg);
        }
        out
    }

    #[test]
    fn initialization_and_failure() {
        let root = Path::new("/tmp/project");

        // 1. Successful init with interleaved notification, server request, unknown ID response,
        //    omitted positionEncoding (defaults to UTF-16), and numeric textDocumentSync = 2.
        let sent = Arc::new(Mutex::new(Vec::new()));
        let reaped = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();

        // Interleaved messages before initialize response (id 1)
        tx.send(IncomingMessage::Message(json!({
            "jsonrpc": "2.0",
            "method": "window/logMessage",
            "params": { "type": 3, "message": "starting" }
        })))
        .unwrap();
        tx.send(IncomingMessage::Message(json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "workspace/configuration",
            "params": { "items": [] }
        })))
        .unwrap();
        tx.send(IncomingMessage::Message(json!({
            "jsonrpc": "2.0",
            "id": 777,
            "result": null
        })))
        .unwrap();
        tx.send(IncomingMessage::Message(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "capabilities": {
                    "definitionProvider": true,
                    "textDocumentSync": 2
                }
            }
        })))
        .unwrap();

        let mut session = LspSession::from_parts(
            Box::new(SharedWriter(Arc::clone(&sent))),
            rx,
            Box::new(FakeProcess(Arc::clone(&reaped))),
            Box::new(FakeClock::new()),
            root,
        )
        .unwrap();

        assert_eq!(
            session.capabilities(),
            ServerCapabilities {
                position_encoding: PositionEncoding::Utf16,
                definition_provider: true,
                open_close: true,
                sync_kind: TextDocumentSyncKind::Incremental,
            }
        );

        let msgs = drain_sent_messages(&sent);
        assert_eq!(msgs[0]["method"], "initialize");
        assert_eq!(msgs[0]["id"], 1);
        // Server request id 99 gets -32601 error reply
        assert_eq!(msgs[1]["id"], 99);
        assert_eq!(msgs[1]["error"]["code"], -32601);
        // Client sends initialized notification after initialize response
        assert_eq!(msgs[2]["method"], "initialized");
        assert!(msgs[2].get("id").is_none());

        // Shutdown sequence: sends shutdown, receives response, sends exit
        tx.send(IncomingMessage::Message(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": null
        })))
        .unwrap();
        session.shutdown();
        let msgs = drain_sent_messages(&sent);
        assert_eq!(msgs[3]["method"], "shutdown");
        assert_eq!(msgs[4]["method"], "exit");
        assert!(reaped.load(Ordering::SeqCst));

        // 2. EOF during initialization reaps process and returns LspError::Exited
        let sent_eof = Arc::new(Mutex::new(Vec::new()));
        let reaped_eof = Arc::new(AtomicBool::new(false));
        let (tx_eof, rx_eof) = crossbeam_channel::unbounded();
        tx_eof.send(IncomingMessage::Eof).unwrap();
        let err = LspSession::from_parts(
            Box::new(SharedWriter(sent_eof)),
            rx_eof,
            Box::new(FakeProcess(Arc::clone(&reaped_eof))),
            Box::new(FakeClock::new()),
            root,
        )
        .unwrap_err();
        assert_eq!(err, LspError::Exited);
        assert!(reaped_eof.load(Ordering::SeqCst));

        // 3. Timeout during request advances controlled clock by 10s, reaps process, returns Timeout
        let sent_to = Arc::new(Mutex::new(Vec::new()));
        let reaped_to = Arc::new(AtomicBool::new(false));
        let (tx_to, rx_to) = crossbeam_channel::unbounded();
        let clock = FakeClock::new();
        let now_ms = Arc::clone(&clock.now_ms);
        tx_to
            .send(IncomingMessage::Message(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "capabilities": {
                        "positionEncoding": "utf-8",
                        "definitionProvider": { "workDoneProgress": false },
                        "textDocumentSync": { "openClose": true, "change": 1 }
                    }
                }
            })))
            .unwrap();
        let mut session_to = LspSession::from_parts(
            Box::new(SharedWriter(sent_to)),
            rx_to,
            Box::new(FakeProcess(Arc::clone(&reaped_to))),
            Box::new(clock),
            root,
        )
        .unwrap();
        assert_eq!(
            session_to.capabilities(),
            ServerCapabilities {
                position_encoding: PositionEncoding::Utf8,
                definition_provider: true,
                open_close: true,
                sync_kind: TextDocumentSyncKind::Full,
            }
        );

        let err = session_to
            .request("textDocument/definition", json!({}))
            .unwrap_err();
        assert_eq!(err, LspError::Timeout);
        assert_eq!(now_ms.load(Ordering::SeqCst), 10_000);
        assert!(reaped_to.load(Ordering::SeqCst));
    }
}
