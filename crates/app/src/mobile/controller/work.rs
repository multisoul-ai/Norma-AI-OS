use super::{Command, MobileAccess, MobileServer};
use anyhow::{ensure, Context as _, Result};
use gpui::RenderImage;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use storage::{AppStore, MobileDeviceGrant, WorkspaceIdentity};

#[path = "../interfaces.rs"]
mod interfaces;
#[path = "../qr.rs"]
mod qr;

pub(super) enum Completion {
    Push(super::super::push::configuration::Summary),
    Info(Vec<WorkspaceIdentity>, Vec<MobileDeviceGrant>),
    Started(
        Arc<MobileServer>,
        Arc<RenderImage>,
        String,
        Instant,
        Vec<WorkspaceIdentity>,
        Vec<MobileDeviceGrant>,
    ),
    Code(Arc<RenderImage>, String, Instant),
    Devices(Vec<MobileDeviceGrant>),
    Approved(Vec<MobileDeviceGrant>),
    Cancelled,
}

pub(super) fn execute(
    command: Command,
    store: AppStore,
    server: Option<Arc<MobileServer>>,
    cancelled: Arc<AtomicBool>,
    sources: super::super::MobileSources,
) -> Result<Completion> {
    match command {
        Command::LoadPush => {
            let config = super::super::push::configuration::effective(&store)?;
            return Ok(Completion::Push(
                super::super::push::configuration::Summary::from_config(config.as_ref()),
            ));
        }
        Command::SavePush {
            endpoint,
            key,
            environment,
        } => {
            let config = super::super::PushRelayConfig::from_fields(&endpoint, &key, environment)?;
            return save_push(&store, server.as_deref(), &cancelled, Some(config));
        }
        Command::ClearPush => return save_push(&store, server.as_deref(), &cancelled, None),
        Command::Reload => {
            return Ok(Completion::Info(
                store.workspace_identity_store().list()?,
                store.mobile_access_store().list()?,
            ))
        }
        Command::Enable => {
            let push = super::super::push::configuration::effective(&store);
            let addresses = interfaces::addresses()?;
            let access = MobileAccess::open(store.clone()).context("无法读取电脑 Keychain 身份")?;
            ensure!(!cancelled.load(Ordering::Acquire), "窗口已关闭，连接已取消");
            let server = Arc::new(
                MobileServer::start_with_mobile_sources(access, addresses, sources)
                    .context("无法启动手机连接")?,
            );
            match push {
                Ok(Some(config)) => {
                    if server.start_push_notifications(config).is_err() {
                        tracing::warn!("手机连接已开启，但通知服务无法启动；请重新配置任务提醒");
                    }
                }
                Err(_) => tracing::warn!("手机连接已开启，但通知配置不可读取；请重新配置任务提醒"),
                Ok(None) => {}
            }
            let (image, pairing_code, expires) = code(&server)?;
            let projects = store.workspace_identity_store().list()?;
            let devices = server.devices()?;
            if cancelled.load(Ordering::Acquire) {
                server.shutdown();
                anyhow::bail!("窗口已关闭，连接已取消");
            }
            return Ok(Completion::Started(
                server,
                image,
                pairing_code,
                expires,
                projects,
                devices,
            ));
        }
        Command::Revoke(ref device) if server.is_none() => {
            store.mobile_access_store().revoke(device)?;
            return Ok(Completion::Devices(store.mobile_access_store().list()?));
        }
        _ => {}
    }
    ensure!(!cancelled.load(Ordering::Acquire), "连接已关闭");
    let server = server.context("请先启用手机连接")?;
    match command {
        Command::NewCode => {
            let (image, pairing_code, expires) = code(&server)?;
            Ok(Completion::Code(image, pairing_code, expires))
        }
        Command::CancelCode => {
            server.cancel_pairing()?;
            Ok(Completion::Cancelled)
        }
        Command::Approve {
            request_id,
            projects,
        } => {
            server
                .approve(&request_id, &projects)
                .context("配对未获批准，请检查核对码是否已过期")?;
            Ok(Completion::Approved(server.devices()?))
        }
        Command::Revoke(device) => {
            server.revoke(&device)?;
            Ok(Completion::Devices(server.devices()?))
        }
        Command::Permissions {
            device_id,
            revision,
            projects,
        } => {
            server
                .replace_permissions(&device_id, revision, &projects)
                .context("权限保存失败，请刷新设备后重试")?;
            Ok(Completion::Devices(server.devices()?))
        }
        Command::Reload
        | Command::Enable
        | Command::LoadPush
        | Command::SavePush { .. }
        | Command::ClearPush => unreachable!("handled before requiring server"),
    }
}

fn save_push(
    store: &AppStore,
    server: Option<&MobileServer>,
    cancelled: &AtomicBool,
    config: Option<super::super::PushRelayConfig>,
) -> Result<Completion> {
    // Saving while phone connections are off is valid. A closed server must never be restarted here.
    ensure!(
        server.is_none() || !cancelled.load(Ordering::Acquire),
        "连接已关闭，请重新打开通知设置"
    );
    super::super::push::configuration::save(store, config.as_ref())?;
    let summary = super::super::push::configuration::Summary::from_config(config.as_ref());
    if let Some(server) = server {
        server
            .configure_push_notifications(config)
            .context("配置已保存，但未能应用；请重新保存或停用")?;
    }
    Ok(Completion::Push(summary))
}

pub(super) fn code(server: &MobileServer) -> Result<(Arc<RenderImage>, String, Instant)> {
    let issued = Instant::now();
    let name = objc2_foundation::NSProcessInfo::processInfo()
        .hostName()
        .to_string();
    let qr = server.begin_pairing(&name)?;
    let json = qr.to_json()?;
    let image = qr::pixels(&json)?;
    let pairing_code = qr.pairing_code().to_owned();
    // QR is monochrome, so GPUI's BGRA and image's RGBA byte layouts coincide.
    Ok((
        Arc::new(RenderImage::new(vec![image::Frame::new(image)])),
        pairing_code,
        issued + Duration::from_secs(120),
    ))
}
