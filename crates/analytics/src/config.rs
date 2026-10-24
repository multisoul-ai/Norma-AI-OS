use std::path::PathBuf;

pub const DEFAULT_HOST: &str = "https://us.i.posthog.com";

#[derive(Clone, Debug)]
pub struct Config {
    /// 用户开关；构建时没有 project key 时无论如何都不发送。
    pub enabled: bool,
    pub host: String,
    pub project_key: String,
    /// 存放匿名 ID、用户 ID 与离线队列的目录。
    pub data_dir: PathBuf,
    /// 用户标识哈希的盐。
    pub id_salt: String,
    pub app_version: String,
    pub os_version: String,
}

impl Config {
    /// 读取构建期环境变量 `NORMA_POSTHOG_KEY` / `NORMA_POSTHOG_HOST` / `NORMA_ANALYTICS_SALT`；
    /// 用户开关取自 `data_dir` 下的退出标记文件，默认开启。
    pub fn from_build_env(data_dir: PathBuf, os_version: String) -> Self {
        Self {
            enabled: !opt_out_marker(&data_dir).exists(),
            host: option_env!("NORMA_POSTHOG_HOST")
                .unwrap_or(DEFAULT_HOST)
                .to_owned(),
            project_key: option_env!("NORMA_POSTHOG_KEY")
                .unwrap_or_default()
                .to_owned(),
            data_dir,
            id_salt: option_env!("NORMA_ANALYTICS_SALT")
                .unwrap_or("norma-analytics")
                .to_owned(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            os_version,
        }
    }

    pub fn has_project_key(&self) -> bool {
        !self.project_key.trim().is_empty()
    }
}

/// 用户关闭统计时存在的标记文件。
pub fn opt_out_marker(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("analytics_opted_out")
}
