//! Process-shared, background-only downloads of original optional media.
mod cache;
mod download;
#[cfg(any(test, feature = "media-download-qa"))]
mod qa;
pub(crate) mod view;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_media_feedback_tests.rs"]
mod wallpaper_feedback_tests;

#[cfg(not(test))]
use crate::app::wallpaper_resolvers::bundle_resources_dir;
use crate::app::MultisoulApp;
use download::{DownloadCancellation, DownloadEvent};
use gpui::{App, Global, WeakEntity};
use media_store::{MediaAvailability, MediaCatalog, MediaStore, MediaTransfers, TransferId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) use cache::resolve as cached_asset;

#[derive(Default)]
struct SharedMedia {
    catalog: Option<Arc<MediaCatalog>>,
    store: Option<MediaStore>,
    transfers: MediaTransfers,
    cancellations: HashMap<String, (TransferId, Arc<DownloadCancellation>)>,
    subscribers: Vec<WeakEntity<MultisoulApp>>,
}
impl Global for SharedMedia {}

impl Drop for SharedMedia {
    fn drop(&mut self) {
        for (_, cancel) in self.cancellations.values() {
            cancel.cancel();
        }
    }
}

#[cfg(not(test))]
pub(crate) fn register_window(app: WeakEntity<MultisoulApp>, cx: &mut App) {
    if !cx.has_global::<SharedMedia>() {
        cx.set_global(SharedMedia::default());
        let work = cx
            .background_executor()
            .spawn(async move { load_catalog_and_cache() });
        cx.spawn(async move |cx| {
            let result = work.await;
            let _ = cx.update(|cx| {
                let library = cx.global_mut::<SharedMedia>();
                match result {
                    Ok((catalog, store, paths)) => {
                        for (rel, path) in paths {
                            cache::publish(&rel, path.clone());
                            library.transfers.seed_verified(&rel, path);
                        }
                        library.catalog = Some(Arc::new(catalog));
                        library.store = Some(store);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "optional media catalog unavailable");
                    }
                }
                notify_subscribers(true, None, cx);
            });
        })
        .detach();
    }
    let library = cx.global_mut::<SharedMedia>();
    library
        .subscribers
        .retain(|subscriber| subscriber.upgrade().is_some());
    library.subscribers.push(app);
}

#[cfg(test)]
pub(crate) fn seed_verified_fixture(rel: &str, cache_root: PathBuf, cx: &mut App) {
    let json = serde_json::json!({
        "schema_version": 1, "resource_version": "media-test",
        "resources": [{ "path": rel, "url": "https://example.com/test.mp4",
            "bytes": 3, "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "bundled": false }]
    });
    let path = cache_root.join("removed.mp4");
    let mut library = SharedMedia::default();
    library.catalog = Some(Arc::new(
        MediaCatalog::from_json(&json.to_string()).expect("trusted test catalog must parse"),
    ));
    library.store = Some(MediaStore::new(cache_root));
    library.transfers.seed_verified(rel, path);
    cx.set_global(library);
}

pub(crate) fn availability(rel: &str, cx: &App) -> Option<MediaAvailability> {
    let library = cx.try_global::<SharedMedia>()?;
    let catalog = library.catalog.as_ref()?;
    catalog.find(rel)?;
    if catalog.is_bundled(rel) {
        return None;
    }
    Some(library.transfers.state(rel))
}

pub(crate) fn download_bytes(rel: &str, cx: &App) -> Option<u64> {
    cx.try_global::<SharedMedia>()?
        .catalog
        .as_ref()?
        .find(rel)
        .map(|artifact| artifact.bytes())
}

pub(crate) fn request(rel: &str, retry: bool, cx: &mut App) {
    if !cx.has_global::<SharedMedia>() {
        return;
    }
    let library = cx.global_mut::<SharedMedia>();
    let Some(catalog) = library.catalog.as_ref() else {
        return;
    };
    if catalog.is_bundled(rel) {
        return;
    }
    let Some(artifact) = catalog.find(rel).cloned() else {
        return;
    };
    let Some(store) = library.store.clone() else {
        return;
    };
    let Some(id) = library.transfers.begin(rel, artifact.bytes(), retry) else {
        return;
    };
    let cancel = Arc::new(DownloadCancellation::default());
    library
        .cancellations
        .insert(rel.into(), (id, cancel.clone()));
    let rel = rel.to_string();
    let (events, receiver) = async_channel::bounded(8);
    cx.background_executor()
        .spawn(async move {
            let result = match download::acquire_slot(&cancel).await {
                Ok(_permit) => download::download(&artifact, &store, &cancel, &events),
                Err(error) => Err(error),
            };
            let _ = events.send_blocking(DownloadEvent::Finished(result));
        })
        .detach();
    cx.spawn(async move |cx| {
        while let Ok(event) = receiver.recv().await {
            let finished = matches!(event, DownloadEvent::Finished(_));
            if cx.update(|cx| receive(&rel, id, event, cx)).is_err() {
                break;
            }
            if finished {
                break;
            }
        }
    })
    .detach();
    cx.defer(|cx| notify_subscribers(false, None, cx));
}

pub(crate) fn cancel(rel: &str, cx: &mut App) {
    if !cx.has_global::<SharedMedia>() {
        return;
    }
    let library = cx.global_mut::<SharedMedia>();
    if !library.transfers.cancel(rel) {
        return;
    }
    if let Some((_, flag)) = library.cancellations.remove(rel) {
        flag.cancel();
    }
    cx.defer(|cx| notify_subscribers(false, None, cx));
}

pub(crate) fn shutdown(cx: &mut App) {
    if cx.has_global::<SharedMedia>() {
        let library = cx.global_mut::<SharedMedia>();
        for (_, flag) in library.cancellations.values() {
            flag.cancel();
        }
    }
}

fn receive(rel: &str, id: TransferId, event: DownloadEvent, cx: &mut App) {
    let library = cx.global_mut::<SharedMedia>();
    let (changed, refresh, image_ready) = match event {
        DownloadEvent::CacheInvalidated => {
            let current = library.transfers.progress(rel, id, 0);
            if current {
                cache::revoke(rel);
            }
            (current, current, false)
        }
        DownloadEvent::Progress(received) => {
            (library.transfers.progress(rel, id, received), false, false)
        }
        DownloadEvent::Finished(result) => {
            let was_missing = cache::resolve(rel).is_none();
            let path = result.as_ref().ok().cloned();
            if let Err(error) = &result {
                tracing::warn!(rel, %error, "optional media transfer failed");
            }
            let changed = library.transfers.finish(rel, id, result);
            if changed {
                if let Some(path) = &path {
                    cache::publish(rel, path.clone());
                }
            }
            if library
                .cancellations
                .get(rel)
                .is_some_and(|(current, _)| *current == id)
            {
                library.cancellations.remove(rel);
            }
            let ready = changed && path.is_some();
            (changed, ready, ready && was_missing)
        }
    };
    if changed {
        notify_subscribers(refresh, image_ready.then_some(rel), cx);
    }
}

fn notify_subscribers(refresh_media: bool, image_ready: Option<&str>, cx: &mut App) {
    let subscribers = cx.global::<SharedMedia>().subscribers.clone();
    for subscriber in subscribers {
        let _ = subscriber.update(cx, |app, cx| {
            if let Some(rel) = image_ready {
                app.wallpaper_settings
                    .image_resource_ready(rel, app.motion.reduce_motion);
            }
            if refresh_media {
                app.refresh_optional_media(cx);
            }
            cx.notify();
        });
    }
}

#[cfg(not(test))]
type LoadedMedia = (MediaCatalog, MediaStore, Vec<(String, PathBuf)>);

#[cfg(not(test))]
fn load_catalog_and_cache() -> Result<LoadedMedia, String> {
    let catalog = if let Some(resources) = bundle_resources_dir() {
        let json = std::fs::read_to_string(resources.join("media-catalog.json"))
            .map_err(|error| error.to_string())?;
        MediaCatalog::from_json(&json).map_err(|error| error.to_string())?
    } else {
        development_catalog()?
    };
    let database = crate::database::default_database_paths().map_err(|error| error.to_string())?;
    let root = database
        .app_db_path()
        .parent()
        .ok_or("missing app data directory")?
        .join("media")
        .join(catalog.resource_version());
    let store = MediaStore::new(root);
    let paths = catalog
        .entries()
        .iter()
        .filter_map(|artifact| {
            store
                .find_verified(artifact)
                .ok()
                .flatten()
                .map(|path| (artifact.relative_path().to_string(), path))
        })
        .collect();
    Ok((catalog, store, paths))
}

#[cfg(not(test))]
fn development_catalog() -> Result<MediaCatalog, String> {
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("../../../../packaging/media/resources.json"))
            .map_err(|error| error.to_string())?;
    let base = json["download_base_url"]
        .as_str()
        .ok_or("missing media URL")?
        .to_string();
    for resource in json["resources"]
        .as_array_mut()
        .ok_or("missing media list")?
    {
        let rel = resource["path"]
            .as_str()
            .ok_or("missing media ID")?
            .to_string();
        let suffix = std::path::Path::new(&rel)
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or("missing media extension")?;
        resource["url"] = format!(
            "{base}/{}.{}",
            resource["sha256"].as_str().ok_or("missing media hash")?,
            suffix
        )
        .into();
        resource["bundled"] = true.into();
    }
    MediaCatalog::from_json(&json.to_string()).map_err(|error| error.to_string())
}
