use super::*;

/// The process owns the service so closing the last window does not disconnect paired phones.
pub(crate) fn attach(store: AppStore, cx: &mut App) -> Entity<MobileController> {
    if !cx.has_global::<Controllers>() {
        cx.set_global(Controllers::default());
    }
    let registry = cx.global_mut::<Controllers>();
    if let Some(existing) = registry.0.get(store.path()) {
        return existing.clone();
    }
    let path = store.path().to_path_buf();
    let entity = cx.new(|cx| {
        let mut host = MobileController::new(store);
        let (sender, receiver) = async_channel::bounded::<crate::mobile::LiveSnapshotRequest>(8);
        host.snapshots = Some(sender);
        let database = host.store.path().to_path_buf();
        host.snapshot_task = Some(cx.spawn(async move |this, cx| {
            // Same eight-request ceiling as the gateway. One slow disk cannot block all other projects.
            use futures::StreamExt;
            receiver
                .for_each_concurrent(8, |request| {
                    let mut cx = cx.clone();
                    let this = this.clone();
                    let database = database.clone();
                    async move {
                        let Ok(cancelled) = this
                            .update(&mut cx, |host: &mut MobileController, _| {
                                host.cancelled.clone()
                            })
                        else {
                            return;
                        };
                        if Instant::now() >= request.deadline || cancelled.load(Ordering::Acquire) {
                            return;
                        }
                        if request.include_cards {
                            for project in &request.projects {
                                if Instant::now() >= request.deadline
                                    || cancelled.load(Ordering::Acquire)
                                {
                                    return;
                                }
                                if let Ok(load) = cx.update(|cx| {
                                    crate::workspace::sessions::ensure(
                                        &database,
                                        &project.workspace_id,
                                        request.deadline,
                                        cx,
                                    )
                                }) {
                                    let timeout = cx.background_executor().timer(
                                        request.deadline.saturating_duration_since(Instant::now()),
                                    );
                                    futures::future::select(load, timeout).await;
                                }
                            }
                        }
                        if Instant::now() >= request.deadline || cancelled.load(Ordering::Acquire) {
                            return;
                        }
                        if let Ok(projects) = cx.update(|cx| {
                            crate::norma::endpoint::mobile_project_snapshots(
                                &database,
                                request.projects,
                                request.include_cards,
                                cx,
                            )
                        }) {
                            let _ = request.reply.try_send(projects);
                        }
                    }
                })
                .await;
        }));
        let (sender, receiver) = async_channel::bounded::<crate::mobile::LiveCommandRequest>(8);
        host.commands = Some(sender);
        let database = host.store.path().to_path_buf();
        host.command_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(call) = receiver.recv().await {
                let _ = cx.update(|cx| {
                    crate::norma::endpoint::mobile_command::execute(database.clone(), call, cx)
                });
            }
        }));
        let (sender, receiver) = async_channel::bounded::<crate::mobile::LiveTerminalRequest>(8);
        host.terminals = Some(sender);
        let database = host.store.path().to_path_buf();
        host.terminal_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(call) = receiver.recv().await {
                let _ = cx.update(|cx| {
                    crate::norma::endpoint::mobile_terminal::read(&database, call, cx)
                });
            }
        }));
        let (sender, receiver) =
            async_channel::bounded::<crate::mobile::LiveConversationRequest>(8);
        host.conversations = Some(sender);
        let database = host.store.path().to_path_buf();
        host.conversation_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(call) = receiver.recv().await {
                let _ = cx.update(|cx| {
                    crate::norma::endpoint::mobile_conversation::read(database.clone(), call, cx)
                });
            }
        }));
        let (sender, receiver) = async_channel::bounded::<crate::mobile::LiveApprovalRequest>(8);
        host.approvals = Some(sender);
        let database = host.store.path().to_path_buf();
        host.approval_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(call) = receiver.recv().await {
                let _ = cx.update(|cx| {
                    crate::norma::endpoint::mobile_approval::read(&database, call, cx)
                });
            }
        }));
        let (sender, receiver) = async_channel::bounded::<crate::mobile::LiveReviewRequest>(8);
        host.reviews = Some(sender);
        let database = host.store.path().to_path_buf();
        host.review_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(call) = receiver.recv().await {
                let _ = cx.update(|cx| {
                    crate::norma::endpoint::mobile_review::read(database.clone(), call, cx)
                });
            }
        }));
        host
    });
    cx.global_mut::<Controllers>()
        .0
        .insert(path, entity.clone());
    entity
}
