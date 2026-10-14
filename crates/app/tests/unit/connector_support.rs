use super::*;
use crate::app::{background_project::PreparedProject, MultisoulApp};
use gpui::{AppContext, Entity, TestAppContext};
use norma::connector::{Auth, Authenticated, ConnectorTools, SystemClock};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

pub(super) struct Fixture {
    pub test: TestAppContext,
    pub tools: ConnectorTools,
    pub auth: Arc<Auth>,
    pub identity: Authenticated,
    pub repo: Arc<repository::Repository>,
    pub requests: async_channel::Receiver<repository::LiveRequest>,
    pub host: Entity<MultisoulApp>,
    pub sessions: Vec<Entity<MultisoulApp>>,
    pub on_next_capture: Option<Box<dyn FnOnce()>>,
    pub workspace: String,
    pub areas: Vec<String>,
    pub _guard: tempfile::TempDir,
}
impl Fixture {
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }
    pub fn with_clock(clock: Arc<dyn norma::connector::Clock>) -> Self {
        let guard = tempfile::tempdir().expect("临时连接器项目");
        let root = guard.path().join("project");
        std::fs::create_dir(&root).expect("项目目录");
        let paths = storage::DatabasePaths::isolated_test(&guard.path().join("data"));
        let store = storage::AppStore::initialize(&paths).expect("临时App库");
        let workspace = store
            .desktop_workspace_store()
            .create("Connector")
            .expect("工作空间");
        let areas: Vec<_> = (0..2)
            .map(|_| {
                store
                    .desktop_workspace_store()
                    .add_project(&workspace.id, &root)
                    .expect("区域")
            })
            .collect();
        let prepared: Vec<_> = areas
            .iter()
            .map(|a| PreparedProject::load_for_desktop(paths.clone(), a.clone()).expect("准备项目"))
            .collect();
        let database = paths.app_db_path().to_owned();
        let mut test = TestAppContext::single();
        let (host, cx) =
            test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
        let sessions: Vec<_> = prepared
            .into_iter()
            .map(|p| {
                cx.new(|cx| {
                    let mut app = MultisoulApp::from_background_project(p, cx);
                    app.runtime.set_pty_factory(Box::new(|_, _, _, tx| {
                        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
                    }));
                    app
                })
            })
            .collect();
        host.update(cx, |app, _| {
            app.desktop_workspace.workspaces = vec![workspace.clone()];
            app.desktop_workspace.active_id = Some(workspace.id.clone());
            app.desktop_workspace.areas = areas.clone();
            for (area, session) in areas.iter().zip(&sessions) {
                app.desktop_workspace
                    .sessions
                    .insert((workspace.id.clone(), area.id.clone()), session.clone());
            }
        });
        cx.update(|window, cx| {
            crate::workspace::desktop::sessions::register(
                database.clone(),
                window.window_handle(),
                host.clone(),
                cx,
            )
        });
        let (live, requests) = async_channel::bounded(8);
        let auth = Arc::new(
            Auth::load(
                Arc::new(repository::Credentials(
                    storage::connector_store::ConnectorStore::new(database.clone()),
                )),
                clock.clone(),
            )
            .expect("随机凭据"),
        );
        let token = auth.generate().expect("随机Token");
        let identity = auth.authenticate(Some(&token)).expect("鉴权");
        let signer = CardSigner::new(&auth.signer_key().expect("签名密钥"));
        let repo = Arc::new(repository::Repository {
            database,
            clock,
            signer: signer.clone(),
            live,
        });
        let tools = ConnectorTools::new(auth.clone(), signer, repo.clone());
        Self {
            test,
            tools,
            auth,
            identity,
            repo,
            requests,
            host,
            sessions,
            on_next_capture: None,
            workspace: workspace.id,
            areas: areas.into_iter().map(|a| a.id).collect(),
            _guard: guard,
        }
    }
    pub fn call(&mut self, name: &str, args: Value) -> Value {
        let tools = self.tools.clone();
        let identity = self.identity.clone();
        let name = name.to_owned();
        let thread = std::thread::spawn(move || {
            tools
                .call(&identity, &name, args)
                .structured_content
                .expect("MCP结构化输出")
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(40);
        while !thread.is_finished() {
            assert!(std::time::Instant::now() < deadline, "后台调用不能死锁");
            if let Ok(request) = self.requests.try_recv() {
                if request.execute.is_none() {
                    if let Some(advance) = self.on_next_capture.take() {
                        advance();
                    }
                }
                self.test.update(|cx| {
                    live::capture(&self.repo.database, request, &self.repo.signer, cx)
                });
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        thread.join().expect("工具线程")
    }
    pub fn create(&mut self, operation: &str, area: usize, task: Option<&str>) -> Value {
        self.call("create_card",serde_json::json!({"operation_id":operation,"workspace_id":self.workspace,"area_id":self.areas[area],"profile":"codex","task":task}))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.test.update(|cx| {
            for session in &self.sessions {
                session.update(cx, |app, _| app.runtime.kill_all());
            }
            self.host.update(cx, |app, _| app.runtime.kill_all());
        });
    }
}
