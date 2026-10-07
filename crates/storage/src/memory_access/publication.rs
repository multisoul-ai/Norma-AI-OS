use super::*;
use agent::MemoryMutation;
use sha2::{Digest, Sha256};

pub(super) struct PublicationSource<'a> {
    pub reference: MemorySourceReference,
    pub text: &'a str,
    pub operation: &'a str,
    pub context: Option<&'a ToolContext>,
}

impl MemoryAccess {
    pub(super) fn publish(
        &mut self,
        alias: MemoryScopeAlias,
        expected: MemoryRevision,
        entries: Vec<(String, String)>,
        supersedes: &[String],
        provenance: PublicationSource<'_>,
    ) -> Result<MemoryToolOutput, AdapterError> {
        let PublicationSource {
            reference: source,
            text,
            operation,
            context,
        } = provenance;
        let scope = self.scope(alias)?;
        if !source.is_well_formed() || entries.is_empty() {
            return Err(AdapterError::InvalidArguments("缺少有效来源或主题".into()));
        }
        let mut changes: Vec<_> = entries
            .iter()
            .map(|(id, markdown)| {
                Ok(MemoryMutation::Upsert {
                    id: MemoryEntryId::parse(id).map_err(failure)?,
                    markdown: markdown.clone(),
                    sources: vec![source.clone()],
                })
            })
            .collect::<Result<_, AdapterError>>()?;
        for id in supersedes {
            changes.push(MemoryMutation::Delete {
                id: MemoryEntryId::parse(id).map_err(failure)?,
            });
        }
        let mut identity =
            serde_json::to_vec(&(scope.clone(), &entries, &source, text)).map_err(failure)?;
        // Keep fingerprints of pre-merge operations compatible with their saved receipts.
        if !supersedes.is_empty() {
            identity.extend(serde_json::to_vec(supersedes).map_err(failure)?);
        }
        let fingerprint = format!("{:x}", Sha256::digest(identity));
        journal::source(self.paths.app_db_path(), &source, text)?;
        let store = self.store()?;
        let candidates: Vec<_> = entries
            .iter()
            .map(|(id, markdown)| (id, format!("{:x}", Sha256::digest(markdown))))
            .collect();
        let record =
            serde_json::to_string(&(scope.clone(), expected, candidates, &source, supersedes))
                .map_err(failure)?;
        let reservation = journal::reserve(
            self.paths.app_db_path(),
            operation,
            &fingerprint,
            &record,
            || {
                // Existing operations must reconcile even after superseded topics are gone.
                if context.is_some() {
                    let baseline = self.baselines.get(&alias).ok_or_else(|| {
                        AdapterError::Rejected("请先 list 或 read 获取写入基线".into())
                    })?;
                    let mut targets: std::collections::HashSet<_> =
                        entries.iter().map(|(id, _)| id).collect();
                    for target in supersedes {
                        if !targets.insert(target)
                            || !baseline.entries().any(|entry| entry.id.as_str() == target)
                        {
                            return Err(AdapterError::InvalidArguments(
                                "合并删除目标必须存在、不能重复或包含保留主题".into(),
                            ));
                        }
                    }
                    for target in entries.iter().map(|(id, _)| id).chain(supersedes) {
                        if baseline.entries().any(|entry| entry.id.as_str() == target)
                            && self.read_entries.get(&(alias, target.clone())) != Some(&expected)
                        {
                            return Err(AdapterError::Rejected(
                                "更新或合并现有主题前必须完整 read".into(),
                            ));
                        }
                    }
                }
                Ok(())
            },
        )?;
        if let Some(receipt) = reservation {
            let expected = match receipt {
                Ok(receipt) => return Ok(receipt),
                Err(expected) => expected,
            };
            let current = store.snapshot(&scope).map_err(failure)?;
            // Reconcile only authenticated CURRENT ancestry, never orphan revision directories.
            for number in expected.number().saturating_add(1)..=current.revision().number() {
                let published = match store.published_snapshot(&scope, MemoryRevision::new(number))
                {
                    Ok(snapshot) => snapshot,
                    Err(crate::MemoryError::RevisionNotFound) => continue,
                    Err(error) => return Err(failure(error)),
                };
                if entries.iter().all(|(id, markdown)| {
                    MemoryEntryId::parse(id)
                        .ok()
                        .and_then(|id| store.read(&published, &id).ok().flatten())
                        .is_some_and(|entry| {
                            entry.markdown == *markdown && entry.sources == [source.clone()]
                        })
                }) && supersedes
                    .iter()
                    .all(|id| !published.entries().any(|entry| entry.id.as_str() == id))
                {
                    let output = MemoryToolOutput {
                        scope: alias,
                        revision: published.revision(),
                        saved: true,
                        ..Default::default()
                    };
                    journal::finish(
                        self.paths.app_db_path(),
                        operation,
                        &output,
                        !supersedes.is_empty(),
                    )
                    .map_err(|error| {
                        AdapterError::Unknown(format!("无法确认保存回执，记忆可能已保存：{error}"))
                    })?;
                    return Ok(output);
                }
            }
            return Err(AdapterError::Unknown(
                "无法确认此前保存结果，请重新核验；不会自动重放".into(),
            ));
        }
        if context.is_some_and(|context| context.cancellation.is_cancelled()) {
            return Err(AdapterError::Cancelled);
        }
        let revision = match store.commit(&scope, expected, &changes) {
            Ok(receipt) => receipt.revision,
            Err(crate::MemoryError::PublishedButUnsynced { revision, .. }) => {
                store
                    .published_snapshot(&scope, revision)
                    .map_err(|error| AdapterError::Unknown(format!("无法确认发布结果：{error}")))?;
                revision
            }
            Err(error) => return Err(failure(error)),
        };
        let output = MemoryToolOutput {
            scope: alias,
            revision,
            saved: true,
            ..Default::default()
        };
        journal::finish(
            self.paths.app_db_path(),
            operation,
            &output,
            !supersedes.is_empty(),
        )
        .map_err(|error| {
            AdapterError::Unknown(format!("无法确认保存回执，记忆可能已保存：{error}"))
        })?;
        self.baselines.insert(
            alias,
            store.snapshot(&scope).map_err(|error| {
                AdapterError::Unknown(format!("记忆已发布，无法确认后续快照：{error}"))
            })?,
        );
        Ok(output)
    }
}
