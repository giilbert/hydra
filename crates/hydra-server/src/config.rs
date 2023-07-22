use serde::Deserialize;
use std::sync::OnceLock;

static GLOBAL_CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default)]
    pub use_https: bool,
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
        // then use default config
        let data = if std::fs::metadata("test-config/config.toml").is_ok() {
            Some(std::fs::read_to_string("test-config/config.toml").unwrap())
        } else if std::fs::metadata("/etc/hydra/config.toml").is_ok() {
            Some(std::fs::read_to_string("/etc/hydra/config.toml").unwrap())
        } else {
            None
        };

        let config = match data {
            Some(data) => toml::from_str(&data).expect("error parsing config file"),
            None => Config {
                use_https: false,
                docker: DockerConfig {
                    cpu_set: "0".to_string(),
                    cpu_shares: 10_000,
                    memory: 8 * 1000 * 1000,
                },
            },
        };

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

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Environment {
    Development,
    Production,
}

impl Environment {
    pub fn get() -> Self {
        match std::env::var("ENVIRONMENT")
            .unwrap_or("development".to_string())
            .as_str()
        {
            "production" => Environment::Production,
            "development" => Environment::Development,
            _ => panic!("invalid environment"),
        }
    }
}