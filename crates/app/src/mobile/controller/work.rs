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
            let access = MobileAccess::open(store.clone())
                .context("Unable to read this Mac’s identity from SQLite")?;
            ensure!(
                !cancelled.load(Ordering::Acquire),
                "Window closed. Connection cancelled."
            );
            let server = Arc::new(
                MobileServer::start_with_mobile_sources(access, addresses, sources)
                    .context("Unable to enable mobile connection")?,
            );
            if server.start_collaboration_relay().is_err() {
                tracing::warn!("Mobile connection is on, but the collaboration relay could not start. Local project access remains available.");
            }
            match push {
                Ok(Some(config)) => {
                    if server.start_push_notifications(config).is_err() {
                        tracing::warn!("Mobile connection is on, but the notification service could not start. Reconfigure task notifications.");
                    }
                }
                Err(_) => tracing::warn!("Mobile connection is on, but notification settings could not be read. Reconfigure task notifications."),
                Ok(None) => {}
            }
            let (image, pairing_code, expires) = code(&server)?;
            let projects = store.workspace_identity_store().list()?;
            let devices = server.devices()?;
            if cancelled.load(Ordering::Acquire) {
                server.shutdown();
                anyhow::bail!("Window closed. Connection cancelled.");
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
    ensure!(!cancelled.load(Ordering::Acquire), "Connection is closed");
    let server = server.context("Enable mobile connection first")?;
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
                .context("Pairing was not approved. Check whether the code has expired.")?;
            Ok(Completion::Approved(server.devices()?))
        }
        Command::ApproveAll { request_id, access } => {
            server
                .approve_all_projects(&request_id, access)
                .context("Pairing was not approved. Check whether the code has expired.")?;
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
                .context("Unable to save permissions. Refresh the device list and try again.")?;
            Ok(Completion::Devices(server.devices()?))
        }
        Command::PermissionsAll {
            device_id,
            revision,
            access,
        } => {
            server
                .replace_with_all_projects(&device_id, revision, access)
                .context("Unable to save permissions. Refresh the device list and try again.")?;
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
        "Connection is closed. Reopen notification settings."
    );
    super::super::push::configuration::save(store, config.as_ref())?;
    let summary = super::super::push::configuration::Summary::from_config(config.as_ref());
    if let Some(server) = server {
        server.configure_push_notifications(config).context(
            "Settings were saved but could not be applied. Save again or disable the service.",
        )?;
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
