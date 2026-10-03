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
        let changes: Vec<_> = entries
            .iter()
            .map(|(id, markdown)| {
                Ok(MemoryMutation::Upsert {
                    id: MemoryEntryId::parse(id).map_err(failure)?,
                    markdown: markdown.clone(),
                    sources: vec![source.clone()],
                })
            })
            .collect::<Result<_, AdapterError>>()?;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&(scope.clone(), &entries, &source, text)).map_err(failure)?
            )
        );
        journal::source(self.paths.app_db_path(), &source, text)?;
        let store = self.store()?;
        let candidates: Vec<_> = entries
            .iter()
            .map(|(id, markdown)| (id, format!("{:x}", Sha256::digest(markdown))))
            .collect();
        let record = serde_json::to_string(&(scope.clone(), expected, candidates, &source))
            .map_err(failure)?;
        if let Some(receipt) =
            journal::reserve(self.paths.app_db_path(), operation, &fingerprint, &record)?
        {
            let expected = match receipt {
                Ok(receipt) => return Ok(receipt),
                Err(expected) => expected,
            };
            let current = store.snapshot(&scope).map_err(failure)?;
            // Reconcile only authenticated CURRENT ancestry, never orphan revision directories.
            for number in (expected.number().saturating_add(1)..=current.revision().number()).rev()
            {
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
                }) {
                    let output = MemoryToolOutput {
                        scope: alias,
                        revision: published.revision(),
                        saved: true,
                        ..Default::default()
                    };
                    journal::finish(self.paths.app_db_path(), operation, &output).map_err(
                        |error| {
                            AdapterError::Unknown(format!(
                                "无法确认保存回执，记忆可能已保存：{error}"
                            ))
                        },
                    )?;
                    return Ok(output);
                }
            }
            return Err(AdapterError::Unknown(
                "无法确认此前保存结果，请重新核验；不会自动重放".into(),
            ));
        }
        self.store()?
            .preview_import(&self.settings_snapshot(alias)?, &entries)
            .map_err(failure)?;
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
        journal::finish(self.paths.app_db_path(), operation, &output).map_err(|error| {
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
