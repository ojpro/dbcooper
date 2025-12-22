use async_ssh2_lite::{AsyncSession, TokioTcpStream};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

pub struct SshTunnel {
    pub local_port: u16,
    _shutdown_tx: oneshot::Sender<()>,
}

impl SshTunnel {
    pub async fn new(
        ssh_host: &str,
        ssh_port: u16,
        ssh_user: &str,
        ssh_password: Option<&str>,
        ssh_key_path: Option<&str>,
        remote_host: &str,
        remote_port: u16,
    ) -> Result<Self, String> {
        // Connect to SSH server
        let ssh_addr: SocketAddr = format!("{}:{}", ssh_host, ssh_port)
            .parse()
            .map_err(|e| format!("Invalid SSH address: {}", e))?;

        let stream = TokioTcpStream::connect(ssh_addr)
            .await
            .map_err(|e| format!("Failed to connect to SSH server: {}", e))?;

        let mut session = AsyncSession::new(stream, None)
            .map_err(|e| format!("Failed to create SSH session: {}", e))?;

        session
            .handshake()
            .await
            .map_err(|e| format!("SSH handshake failed: {}", e))?;

        // Authenticate with key first if provided
        if let Some(key_path) = ssh_key_path {
            if !key_path.is_empty() {
                // Expand ~ to home directory
                let expanded_path = if key_path.starts_with("~") {
                    if let Some(home) = dirs::home_dir() {
                        key_path.replacen("~", home.to_str().unwrap_or(""), 1)
                    } else {
                        key_path.to_string()
                    }
                } else {
                    key_path.to_string()
                };

                let _ = session
                    .userauth_pubkey_file(
                        ssh_user,
                        None,
                        std::path::Path::new(&expanded_path),
                        None,
                    )
                    .await;
            }
        }

        // If not authenticated, try password
        if !session.authenticated() {
            if let Some(password) = ssh_password {
                if !password.is_empty() {
                    session
                        .userauth_password(ssh_user, password)
                        .await
                        .map_err(|e| format!("SSH password authentication failed: {}", e))?;
                }
            }
        }

        if !session.authenticated() {
            return Err("SSH authentication failed - check credentials".to_string());
        }

        // Create local listener on random port
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind local port: {}", e))?;

        let local_port = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?
            .port();

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let remote_host = remote_host.to_string();
        let session = Arc::new(session);

        // Spawn tunnel forwarding task
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    accept_result = listener.accept() => {
                        if let Ok((mut local_stream, _)) = accept_result {
                            let session = Arc::clone(&session);
                            let remote_host = remote_host.clone();

                            tokio::spawn(async move {
                                if let Ok(channel) = session
                                    .channel_direct_tcpip(&remote_host, remote_port, None)
                                    .await
                                {
                                    let mut channel = channel;
                                    let _ = tokio::io::copy_bidirectional(&mut local_stream, &mut channel).await;
                                }
                            });
                        }
                    }
                }
            }
        });

        Ok(Self {
            local_port,
            _shutdown_tx: shutdown_tx,
        })
    }
}
