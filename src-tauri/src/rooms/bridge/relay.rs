//! App-owned, reactive outbound transport. No cloud request polling or idle heartbeat.
use futures_util::{SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{
    fs::File,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, Message},
    WebSocketStream,
};
use tokio_util::sync::CancellationToken;

fn tls_connector() -> Result<tokio_tungstenite::Connector, &'static str> {
    native_tls::TlsConnector::new()
        .map(tokio_tungstenite::Connector::NativeTls)
        .map_err(|_| "System TLS initialization failed")
}

#[derive(Deserialize)]
struct Config {
    relay_url: String,
    mac_token: String,
    native_client: PathBuf,
}
#[derive(Deserialize)]
struct Client {
    endpoint: String,
    bearer: String,
}

fn private_read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "Relay configuration unavailable")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o077 != 0 || meta.uid() != unsafe { libc::geteuid() } {
            return Err("Relay file must be private to its owner".into());
        }
    }
    if !meta.is_file() || meta.len() > 2 * 1024 * 1024 {
        return Err("Invalid relay file".into());
    }
    serde_json::from_slice(&std::fs::read(path).map_err(|_| "Relay file unavailable")?)
        .map_err(|_| "Invalid relay file".into())
}

fn save(path: &Path, value: &Value) -> Result<(), String> {
    use std::io::Write;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let result = (|| {
        let mut file = opts
            .open(&temporary)
            .map_err(|_| "Relay journal unavailable")?;
        file.write_all(value.to_string().as_bytes())
            .map_err(|_| "Relay journal write failed")?;
        file.sync_all().map_err(|_| "Relay journal sync failed")?;
        std::fs::rename(&temporary, path).map_err(|_| "Relay journal replacement failed")?;
        File::open(path.parent().ok_or("Relay journal directory unavailable")?)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| "Relay journal directory sync failed")
    })();
    let _ = std::fs::remove_file(temporary);
    result.map_err(str::to_owned)
}

fn websocket_url(
    config: &Config,
    client: &Client,
    bind: &str,
    port: u16,
) -> Result<url::Url, String> {
    let mut remote = url::Url::parse(&config.relay_url).map_err(|_| "Invalid relay origin")?;
    let local = url::Url::parse(&client.endpoint).map_err(|_| "Invalid native endpoint")?;
    if remote.scheme() != "https"
        || remote.host_str().is_none()
        || !remote.username().is_empty()
        || remote.password().is_some()
        || remote.query().is_some()
        || remote.fragment().is_some()
        || remote.path() != "/"
        || !bind
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
        || local.scheme() != "http"
        || !local
            .host_str()
            .is_some_and(|host| matches!(host, "127.0.0.1" | "[::1]"))
        || local.port_or_known_default() != Some(port)
        || local.path() != "/mcp/ocv"
        || !local.username().is_empty()
        || local.password().is_some()
        || local.query().is_some()
        || local.fragment().is_some()
        || config.mac_token.len() < 40
        || client.bearer.len() < 40
    {
        return Err("Invalid private relay configuration".into());
    }
    remote
        .set_scheme("wss")
        .map_err(|_| "Invalid relay scheme")?;
    remote.set_path("/bridge/connect");
    Ok(remote)
}

/// Retains the Python journal format so uncertain work survives the migration.
fn recover(path: &Path) -> Result<Value, String> {
    let mut record = if path.exists() {
        private_read(path)?
    } else {
        json!({"state":"settled"})
    };
    match record["state"].as_str() {
        Some("executing") => {
            record["response"] = json!({"jsonrpc":"2.0","id":record["rpc_id"],"error":{"code":-32070,"message":"Native execution outcome is uncertain; inspect the message receipt before retrying","data":{"request_key":record["key"],"state":"ambiguous"}}});
            record["state"] = json!("responding");
            save(path, &record)?;
        }
        Some("settled" | "responding") => {}
        _ => return Err("Relay journal needs recovery".into()),
    }
    Ok(record)
}

async fn session<S, F, Fut>(
    mut socket: WebSocketStream<S>,
    journal: &Path,
    mut execute: F,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnMut(Value) -> Fut,
    Fut: Future<Output = Value>,
{
    let mut record = recover(journal)?;
    loop {
        let responding = record["state"] == "responding";
        let frame = if responding {
            json!({"type":"response","key":record["key"],"response":record["response"]})
        } else {
            json!({"type":"ready"})
        };
        socket
            .send(Message::Text(frame.to_string()))
            .await
            .map_err(|_| "Relay connection lost")?;
        loop {
            // Only acknowledgement waits have a deadline; idle receive waits indefinitely.
            let incoming = if responding {
                tokio::time::timeout(Duration::from_secs(30), socket.next())
                    .await
                    .map_err(|_| "Relay acknowledgement uncertain")?
            } else {
                socket.next().await
            };
            match incoming {
                Some(Ok(Message::Text(text))) => {
                    if text.len() > 64 * 1024 {
                        return Err("Relay frame exceeds limit".into());
                    }
                    let frame: Value =
                        serde_json::from_str(&text).map_err(|_| "Invalid relay frame")?;
                    if responding {
                        if frame["type"] != "ack" || frame["key"] != record["key"] {
                            return Err("Invalid relay acknowledgement".into());
                        }
                        record = json!({"state":"settled"});
                        save(journal, &record)?;
                    } else {
                        let key = frame["key"]
                            .as_str()
                            .filter(|key| {
                                key.len() == 66
                                    && key.starts_with("q_")
                                    && key[2..].bytes().all(|c| c.is_ascii_hexdigit())
                            })
                            .ok_or("Invalid lease key")?;
                        if frame["type"] != "work"
                            || !frame["rpc"].is_object()
                            || frame["rpc"].get("id").is_none()
                        {
                            return Err("Invalid relay work".into());
                        }
                        log::debug!("[room_relay] accepted pushed request");
                        record = json!({"state":"executing","key":key,"rpc_id":frame["rpc"]["id"]});
                        save(journal, &record)?;
                        let response = execute(frame["rpc"].clone()).await;
                        if response["id"] != record["rpc_id"] || response["jsonrpc"] != "2.0" {
                            return Err("Native response uncertain".into());
                        }
                        record["state"] = json!("responding");
                        record["response"] = if response.to_string().len() > 63 * 1024 {
                            json!({"jsonrpc":"2.0","id":record["rpc_id"],"error":{"code":-32070,"message":"Native response exceeds relay limit; inspect existing receipt before retrying"}})
                        } else {
                            response
                        };
                        save(journal, &record)?;
                    }
                    break;
                }
                Some(Ok(Message::Ping(payload))) => {
                    socket
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|_| "Relay connection lost")?;
                }
                Some(Ok(Message::Pong(_))) => {}
                _ => return Err("Relay connection closed".into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::{TcpListener, TcpStream};
    const KEY: &str = "q_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn secure_tls_backend_initializes_without_a_missing_crypto_provider() {
        assert!(tls_connector().is_ok());
    }

    async fn pair() -> (
        WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
        WebSocketStream<TcpStream>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (client, server) = tokio::join!(
            tokio_tungstenite::connect_async(format!("ws://{addr}")),
            async { tokio_tungstenite::accept_async(listener.accept().await.unwrap().0).await }
        );
        (client.unwrap().0, server.unwrap())
    }
    async fn frame(socket: &mut WebSocketStream<TcpStream>) -> Value {
        let msg = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        serde_json::from_str(msg.to_text().unwrap()).unwrap()
    }
    async fn send(socket: &mut WebSocketStream<TcpStream>, frame: Value) {
        socket.send(Message::Text(frame.to_string())).await.unwrap();
    }

    #[tokio::test]
    async fn idle_is_silent_and_lost_ack_replays_only_response() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("journal.json");
        let calls = Arc::new(AtomicUsize::new(0));
        let (client, mut server) = pair().await;
        let task = {
            let calls = calls.clone();
            let journal = journal.clone();
            tokio::spawn(async move {
                session(client, &journal, |rpc| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async move { json!({"jsonrpc":"2.0","id":rpc["id"],"result":{}}) }
                })
                .await
            })
        };
        assert_eq!(frame(&mut server).await, json!({"type":"ready"}));
        assert!(
            tokio::time::timeout(Duration::from_millis(120), server.next())
                .await
                .is_err()
        );
        send(
            &mut server,
            json!({"type":"work","key":KEY,"rpc":{"jsonrpc":"2.0","id":1,"method":"ping"}}),
        )
        .await;
        let response = frame(&mut server).await;
        assert_eq!(response["type"], "response");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        send(&mut server, json!({"type":"ack","key":"wrong"})).await;
        assert!(task.await.unwrap().is_err());
        assert_eq!(
            private_read::<Value>(&journal).unwrap()["state"],
            "responding"
        );
        let (client, mut server) = pair().await;
        let task = {
            let calls = calls.clone();
            let journal = journal.clone();
            tokio::spawn(async move {
                session(client, &journal, |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { panic!("must never reexecute") }
                })
                .await
            })
        };
        assert_eq!(frame(&mut server).await, response);
        send(&mut server, json!({"type":"ack","key":KEY})).await;
        assert_eq!(frame(&mut server).await, json!({"type":"ready"}));
        assert_eq!(private_read::<Value>(&journal).unwrap()["state"], "settled");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(
            tokio::time::timeout(Duration::from_millis(120), server.next())
                .await
                .is_err()
        );
        task.abort();
        let _ = task.await;
        assert!(tokio::time::timeout(Duration::from_secs(2), server.next())
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn interrupted_python_journal_is_recovered_without_execution() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("journal.json");
        save(
            &journal,
            &json!({"state":"executing","key":KEY,"rpc_id":"old"}),
        )
        .unwrap();
        let (client, mut server) = pair().await;
        let task = {
            let journal = journal.clone();
            tokio::spawn(async move {
                session(client, &journal, |_| async {
                    panic!("uncertain work cannot replay")
                })
                .await
            })
        };
        let response = frame(&mut server).await;
        assert_eq!(response["response"]["id"], "old");
        assert_eq!(response["response"]["error"]["data"]["state"], "ambiguous");
        send(&mut server, json!({"type":"ack","key":KEY})).await;
        assert_eq!(frame(&mut server).await["type"], "ready");
        assert_eq!(private_read::<Value>(&journal).unwrap()["state"], "settled");
        task.abort();
    }

    #[test]
    fn configuration_requires_private_files_tls_and_the_owning_listener() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        save(&path, &json!({"state":"settled"})).unwrap();
        assert!(private_read::<Value>(&path).is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(private_read::<Value>(&path).is_err());
        }
        let mut config = Config {
            relay_url: "https://relay.example".into(),
            mac_token: "m".repeat(48),
            native_client: path,
        };
        let client = Client {
            endpoint: "http://127.0.0.1:9476/mcp/ocv".into(),
            bearer: "n".repeat(48),
        };
        assert_eq!(
            websocket_url(&config, &client, "127.0.0.1", 9476)
                .unwrap()
                .as_str(),
            "wss://relay.example/bridge/connect"
        );
        assert!(websocket_url(&config, &client, "0.0.0.0", 9476).is_err());
        assert!(websocket_url(&config, &client, "127.0.0.1", 9477).is_err());
        config.relay_url = "http://relay.example".into();
        assert!(websocket_url(&config, &client, "127.0.0.1", 9476).is_err());
    }
}

pub(crate) fn start(
    store: Arc<super::RoomStore>,
    bind: &str,
    port: u16,
    server_cancel: CancellationToken,
    app_cancel: CancellationToken,
) {
    let folder = crate::storage::data_dir();
    let status_path = folder.join("bridge-relay-status.json");
    let _ = save(&status_path, &json!({"state":"Starting"}));
    let path = folder.join("bridge-relay-client.json");
    if !path.exists() {
        return;
    }
    let configuration = (|| {
        let config: Config = private_read(&path)?;
        let client: Client = private_read(&config.native_client)?;
        let remote = websocket_url(&config, &client, bind, port)?;
        super::authenticate(&client.bearer)?;
        Ok::<_, String>((config, client, remote))
    })();
    let (config, client, remote) = match configuration {
        Ok(value) => value,
        Err(reason) => {
            let _ = save(
                &status_path,
                &json!({"state":"Not connected","reason":reason}),
            );
            log::warn!("[room_relay] private configuration unavailable; not connecting");
            return;
        }
    };
    tokio::spawn(async move {
        let mut opts = std::fs::OpenOptions::new();
        opts.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let Ok(lock) = opts.open(folder.join("bridge-relay.lock")) else {
            let _ = save(
                &status_path,
                &json!({"state":"Not connected","reason":"Journal lock unavailable"}),
            );
            return;
        };
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                log::warn!("[room_relay] another transport owns journal; not connecting");
                let _ = save(
                    &status_path,
                    &json!({"state":"Not connected","reason":"Another transport owns journal"}),
                );
                return;
            }
        }
        let journal = folder.join("bridge-relay-journal.json");
        let connection = async {
            let mut failures = 0u32;
            loop {
                let attempt = async {
                    let mut request = remote
                        .as_str()
                        .into_client_request()
                        .map_err(|_| "Invalid relay request")?;
                    request.headers_mut().insert(
                        "Authorization",
                        format!("Bearer {}", config.mac_token)
                            .parse()
                            .map_err(|_| "Invalid relay credential")?,
                    );
                    request.headers_mut().insert(
                        "User-Agent",
                        concat!("OpenCovibe-Reactive-Relay/", env!("CARGO_PKG_VERSION"))
                            .parse()
                            .map_err(|_| "Invalid relay client header")?,
                    );
                    let (socket, _) = tokio::time::timeout(
                        Duration::from_secs(20),
                        tokio_tungstenite::connect_async_tls_with_config(
                            request,
                            None,
                            false,
                            Some(tls_connector()?),
                        ),
                    )
                    .await
                    .map_err(|_| "Relay connection timeout")?
                    .map_err(|_| "Relay connection unavailable")?;
                    log::info!("[room_relay] reactive connection open; idle traffic disabled");
                    let _ = save(&status_path, &json!({"state":"Connected"}));
                    session(socket, &journal, |rpc| {
                        let store = &store;
                        let bearer = &client.bearer;
                        async move { match super::authenticate(bearer) {
                            Ok(principal) => super::protocol::dispatch(store,&principal,&rpc).await,
                            Err(_) => json!({"jsonrpc":"2.0","id":rpc["id"],"error":{"code":-32001,"message":"Native access revoked"}}),
                        } }
                    }).await
                };
                let result = attempt.await;
                if let Err(reason) = result {
                    let _ = save(
                        &status_path,
                        &json!({"state":"Not connected","reason":reason}),
                    );
                }
                // Failure recovery only. A healthy idle socket sends no timer traffic.
                let delay = (10u64.saturating_mul(1u64 << failures.min(5))).min(300);
                failures = failures.saturating_add(1);
                log::warn!(
                    "[room_relay] connection interrupted; lease retained; retry in {delay}s"
                );
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
        };
        tokio::select! { _=connection=>{}, _=server_cancel.cancelled()=>{}, _=app_cancel.cancelled()=>{} }
        // Dropping the socket immediately closes the app-owned outbound connection.
        drop(lock);
        let _ = save(&status_path, &json!({"state":"Closed"}));
        log::info!("[room_relay] app/server stopped; reactive connection closed");
    });
}
