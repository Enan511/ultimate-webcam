use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

type Connection = (OwnedWriteHalf, BufReader<OwnedReadHalf>);

/// Use a fresh connection so a pending UI command cannot delay camera shutdown.
pub async fn stop_remote_camera(addr: &str) -> anyhow::Result<()> {
    let client = ControlClient::new();
    client.connect(addr).await?;
    let result = client
        .send_command(serde_json::json!({"cmd":"STOP_STREAM"}))
        .await;
    client.disconnect().await;
    result.map(|_| ())
}
#[derive(Clone)]
pub struct ControlClient {
    stream: Arc<Mutex<Option<Connection>>>,
}
impl ControlClient {
    pub fn new() -> Self {
        Self {
            stream: Arc::new(Mutex::new(None)),
        }
    }
    pub async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        let stream =
            tokio::time::timeout(std::time::Duration::from_secs(3), TcpStream::connect(addr))
                .await??;
        stream.set_nodelay(true)?;
        let (reader, writer) = stream.into_split();
        *self.stream.lock().await = Some((writer, BufReader::new(reader)));
        Ok(())
    }
    pub async fn send_command(&self, cmd: Value) -> anyhow::Result<Value> {
        let mut guard = self.stream.lock().await;
        let connection = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Phone is not connected"))?;
        let request = async {
            connection
                .0
                .write_all(format!("{}\n", serde_json::to_string(&cmd)?).as_bytes())
                .await?;
            let mut line = String::new();
            if connection.1.read_line(&mut line).await? == 0 {
                anyhow::bail!("Phone control connection closed");
            }
            let response: Value = serde_json::from_str(&line)?;
            Ok(response)
        };
        match tokio::time::timeout(std::time::Duration::from_secs(12), request).await {
            Ok(Ok(response)) => {
                if response["status"] == "ERROR" {
                    anyhow::bail!(
                        "{}",
                        response["error"]
                            .as_str()
                            .unwrap_or("Phone rejected command")
                    );
                }
                Ok(response)
            }
            Ok(Err(error)) => {
                *guard = None;
                Err(error)
            }
            Err(_) => {
                *guard = None;
                anyhow::bail!("Phone did not respond. Reconnect the USB cable.")
            }
        }
    }
    pub async fn disconnect(&self) {
        *self.stream.lock().await = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    #[tokio::test]
    async fn shutdown_sends_stop_before_disconnecting() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut reader = BufReader::new(read);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&line).unwrap()["cmd"],
                "STOP_STREAM"
            );
            write.write_all(b"{\"status\":\"OK\"}\n").await.unwrap();
            line.clear();
            assert_eq!(reader.read_line(&mut line).await.unwrap(), 0);
        });
        stop_remote_camera(&addr.to_string()).await.unwrap();
        server.await.unwrap();
    }
    #[tokio::test]
    async fn rejected_command_does_not_destroy_control_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut reader = BufReader::new(read);
            for response in [
                r#"{"status":"ERROR","error":"Unsupported mode"}"#,
                r#"{"status":"OK","telemetry":{"isStreaming":true}}"#,
            ] {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                assert!(!line.trim().is_empty());
                write
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let client = ControlClient::new();
        client.connect(&addr.to_string()).await.unwrap();
        assert!(client
            .send_command(serde_json::json!({"cmd":"CONFIGURE"}))
            .await
            .unwrap_err()
            .to_string()
            .contains("Unsupported mode"));
        assert_eq!(
            client
                .send_command(serde_json::json!({"cmd":"GET_STATE"}))
                .await
                .unwrap()["status"],
            "OK"
        );
        server.await.unwrap();
    }
    #[tokio::test]
    async fn closed_control_socket_produces_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            drop(stream);
        });
        let client = ControlClient::new();
        client.connect(&addr.to_string()).await.unwrap();
        server.await.unwrap();
        assert!(client
            .send_command(serde_json::json!({"cmd":"GET_STATE"}))
            .await
            .is_err());
    }
}
