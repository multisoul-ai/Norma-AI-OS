use norma::workspace_dispatch::*;

pub fn card(area: &str, instance: &str) -> WorkspaceCardRef {
    WorkspaceCardRef {
        workspace_id: "space".into(),
        area_id: area.into(),
        node_id: 1,
        card_instance_id: instance.into(),
    }
}

pub fn directory() -> WorkspaceDirectory {
    let project = WorkspaceProjectRef {
        workspace_id: "space".into(),
        project_id: "project".into(),
    };
    let role = WorkspaceDirectoryRole {
        card: card("first", "alpha"),
        project: project.clone(),
        name: "Alan".into(),
        speech_alias: Some("艾伦".into()),
        enabled: true,
        submission: WorkspaceSubmissionCapability::Supported,
        execution: None,
    };
    WorkspaceDirectory {
        workspace_id: "space".into(),
        revision: 3,
        catalog_fingerprint: "catalog".into(),
        authorization_revision: "desktop".into(),
        authorized_area_ids: vec!["first".into(), "second".into()],
        covered_area_ids: vec!["first".into(), "second".into()],
        missing_area_ids: vec![],
        registration_complete: true,
        projects: vec![WorkspaceDirectoryProject {
            reference: project.clone(),
            name: "Web".into(),
            area_ids: vec!["first".into(), "second".into()],
        }],
        areas: vec![
            WorkspaceDirectoryArea {
                area_id: "first".into(),
                project: project.clone(),
                name: "Web".into(),
            },
            WorkspaceDirectoryArea {
                area_id: "second".into(),
                project,
                name: "Web background".into(),
            },
        ],
        call_keys: vec![
            WorkspaceDirectoryCallKey {
                key: "alan".into(),
                card: role.card.clone(),
                state: WorkspaceCallKeyStatus::Active,
            },
            WorkspaceDirectoryCallKey {
                key: "艾伦".into(),
                card: role.card.clone(),
                state: WorkspaceCallKeyStatus::Active,
            },
            WorkspaceDirectoryCallKey {
                key: "simon".into(),
                card: role.card.clone(),
                state: WorkspaceCallKeyStatus::Retired,
            },
        ],
        roles: vec![role],
    }
}

pub fn request(target: WorkspaceTargetIntent) -> WorkspaceResolutionRequest {
    WorkspaceResolutionRequest {
        mode: WorkspaceRoutingMode::Execute,
        body: "修复登录".into(),
        targets: vec![target],
    }
}

pub fn named(name: &str) -> WorkspaceTargetIntent {
    WorkspaceTargetIntent::CallName {
        name: name.into(),
        project: None,
    }
}
