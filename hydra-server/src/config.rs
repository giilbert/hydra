use std::sync::OnceLock;

use serde::Deserialize;

static GLOBAL_CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Deserialize, Debug)]
pub struct Config {
    pub docker: DockerConfig,
}

#[derive(Deserialize, Debug)]
pub struct DockerConfig {
    pub cpu_set: String,
    pub cpu_shares: i64,
    #[serde(with = "serde_humanize_rs")]
    pub memory: usize,
}

impl Config {
    pub fn load_from_file() -> Self {
        // first check test-config/config.toml
        // then check /etc/hydra/config.toml
        let data = if std::fs::metadata("test-config/config.toml").is_ok() {
            std::fs::read_to_string("test-config/config.toml")
        } else {
            std::fs::read_to_string("/etc/hydra/config.toml")
        }
        .unwrap();

        let config = toml::from_str(&data).expect("error parsing config file");

        log::info!("Loaded config: {:?}", config);

        config
    }

    pub fn global() -> &'static Self {
        GLOBAL_CONFIG.get_or_init(|| {
            let config = Config::load_from_file();
            config
        })
    }
}
