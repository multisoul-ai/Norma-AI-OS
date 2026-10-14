//! Dedicated App thread owns tokio and all disk/Keychain work. Shutdown never creates a process.
use super::repository::{Credentials, LiveRequest, Repository};
use norma::connector::{
    Auth, CardSigner, ConnectorConfig, ConnectorServer, ConnectorTools, Keychain, SystemClock,
};
use std::{path::PathBuf, sync::Arc};
use storage::connector_store::{ConnectorAudit, ConnectorStore};

#[derive(Clone)]
pub(super) enum Command {
    Enable,
    Disable,
    Generate,
    Revoke,
    Port(u16),
    TunnelAuthority(Option<workspace::HostAuthority>),
    Reload,
    Shutdown,
}
pub(super) struct Update {
    pub config: storage::connector_store::ConnectorConfig,
    pub listening: bool,
    pub expiries: Vec<Option<i64>>,
    pub locked_until: i64,
    pub audit: Vec<ConnectorAudit>,
    pub token: Option<String>,
    pub error: Option<String>,
    pub signer: Option<CardSigner>,
}
pub(super) fn start(
    database: PathBuf,
    commands: async_channel::Receiver<Command>,
    updates: async_channel::Sender<Update>,
    live: async_channel::Sender<LiveRequest>,
) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build();
        let result = runtime
            .map_err(anyhow::Error::from)
            .and_then(|runtime| runtime.block_on(run(database, commands, updates.clone(), live)));
        if result.is_err() {
            let _ = updates.send_blocking(Update {
                config: Default::default(),
                listening: false,
                expiries: Vec::new(),
                locked_until: 0,
                audit: Vec::new(),
                token: None,
                error: Some("连接器初始化失败，请检查存储与钥匙串权限。".into()),
                signer: None,
            });
        }
    });
}
fn keychain() -> anyhow::Result<Arc<dyn Keychain>> {
    #[cfg(target_os = "macos")]
    {
        Ok(Arc::new(norma::connector::MacKeychain))
    }
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("connector credentials require macOS Keychain")
    }
}
async fn run(
    database: PathBuf,
    commands: async_channel::Receiver<Command>,
    updates: async_channel::Sender<Update>,
    live: async_channel::Sender<LiveRequest>,
) -> anyhow::Result<()> {
    let store = ConnectorStore::new(database.clone());
    let mut config = store.config()?;
    let auth = Arc::new(Auth::load(
        Arc::new(Credentials(store.clone())),
        keychain()?,
        Arc::new(SystemClock),
    )?);
    if !config.enabled {
        auth.revoke()?;
    }
    let signer = CardSigner::new(&auth.signer_key()?);
    let source = Arc::new(Repository {
        database,
        signer: signer.clone(),
        live,
    });
    let tools = ConnectorTools::new(auth.clone(), signer.clone(), source);
    let mut server = None;
    let mut error = None;
    if config.enabled {
        match ConnectorServer::start(
            ConnectorConfig {
                enabled: true,
                port: config.port,
                tunnel_authority: config.tunnel_authority.clone(),
            },
            auth.clone(),
            tools.clone(),
        )
        .await
        {
            Ok(started) => server = started,
            Err(_) => error = Some("端口无法监听，请检查是否已被占用。".into()),
        }
    }
    publish(
        &updates,
        &store,
        &config,
        &auth,
        Details {
            listening: server.is_some(),
            token: None,
            error: error.clone(),
            signer: Some(signer),
        },
    )
    .await;
    loop {
        let command = tokio::select! {
            command=commands.recv()=>match command { Ok(command)=>command,Err(_)=>break },
            _=tokio::time::sleep(std::time::Duration::from_secs(2)), if server.is_some()=>Command::Reload,
        };
        let reloading = matches!(command, Command::Reload);
        let mut token = None;
        let result = match command {
            Command::Shutdown => break,
            Command::Enable => {
                if server.is_none() {
                    match ConnectorServer::start(
                        ConnectorConfig {
                            enabled: true,
                            port: config.port,
                            tunnel_authority: config.tunnel_authority.clone(),
                        },
                        auth.clone(),
                        tools.clone(),
                    )
                    .await
                    {
                        Ok(started) => {
                            server = started;
                            config.enabled = true;
                            store.save_config(&config).map_err(Into::into)
                        }
                        Err(_) => Err(anyhow::anyhow!("端口无法监听，请检查是否已被占用。")),
                    }
                } else {
                    Ok(())
                }
            }
            Command::Disable => {
                // Revoke first, then stop; an IO failure cannot preserve an authorized session.
                let revoke = auth.revoke();
                if let Some(old) = server.take() {
                    old.stop().await;
                }
                config.enabled = false;
                let save = store.save_config(&config).map_err(anyhow::Error::from);
                revoke.and(save)
            }
            Command::Generate => match auth.generate() {
                Ok(value) => {
                    token = Some(value);
                    Ok(())
                }
                Err(e) => Err(e),
            },
            Command::Revoke => auth.revoke(),
            Command::Port(port) => {
                if port == 0 {
                    Err(anyhow::anyhow!("端口须为 1–65535。"))
                } else {
                    let enabled = config.enabled;
                    if let Some(old) = server.take() {
                        old.stop().await;
                    }
                    config.port = port;
                    let saved = store.save_config(&config).map_err(anyhow::Error::from);
                    if saved.is_ok() && enabled {
                        match ConnectorServer::start(
                            ConnectorConfig {
                                enabled: true,
                                port,
                                tunnel_authority: config.tunnel_authority.clone(),
                            },
                            auth.clone(),
                            tools.clone(),
                        )
                        .await
                        {
                            Ok(started) => {
                                server = started;
                                Ok(())
                            }
                            Err(_) => Err(anyhow::anyhow!("新端口无法监听，请更换端口。")),
                        }
                    } else {
                        saved
                    }
                }
            }
            Command::TunnelAuthority(authority) => {
                let mut next = config.clone();
                next.tunnel_authority = authority;
                match store.save_config(&next) {
                    Err(error) => Err(error.into()),
                    Ok(()) => {
                        if let Some(old) = server.take() {
                            old.stop().await;
                        }
                        config = next;
                        if config.enabled {
                            match ConnectorServer::start(
                                ConnectorConfig {
                                    enabled: true,
                                    port: config.port,
                                    tunnel_authority: config.tunnel_authority.clone(),
                                },
                                auth.clone(),
                                tools.clone(),
                            )
                            .await
                            {
                                Ok(started) => {
                                    server = started;
                                    Ok(())
                                }
                                Err(error) => Err(error.into()),
                            }
                        } else {
                            Ok(())
                        }
                    }
                }
            }
            Command::Reload => Ok(()),
        };
        if !reloading {
            error = result
                .err()
                .map(|_| "连接器操作失败，请检查端口、存储与钥匙串权限。".into());
        }
        publish(
            &updates,
            &store,
            &config,
            &auth,
            Details {
                listening: server.is_some(),
                token,
                error: error.clone(),
                signer: None,
            },
        )
        .await;
    }
    if let Some(server) = server {
        server.stop().await;
    }
    Ok(())
}
struct Details {
    listening: bool,
    token: Option<String>,
    error: Option<String>,
    signer: Option<CardSigner>,
}
async fn publish(
    updates: &async_channel::Sender<Update>,
    store: &ConnectorStore,
    config: &storage::connector_store::ConnectorConfig,
    auth: &Auth,
    details: Details,
) {
    let Details {
        listening,
        token,
        error,
        signer,
    } = details;
    let audit = store.audit_page(20);
    let error = error.or_else(|| audit.as_ref().err().map(|_| "审计记录暂不可读。".into()));
    let _ = updates
        .send(Update {
            config: config.clone(),
            listening,
            expiries: auth.token_status(),
            locked_until: auth.locked_until(),
            audit: audit.unwrap_or_default(),
            token,
            error,
            signer,
        })
        .await;
}
