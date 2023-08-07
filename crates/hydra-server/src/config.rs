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
    #[serde(with = "serde_humanize_rs")]
    pub disk_read_rate: usize,
    #[serde(with = "serde_humanize_rs")]
    pub disk_write_rate: usize,
    #[serde(with = "serde_humanize_rs")]
    pub disk_max_size: usize,
}

impl Config {
    pub fn load_from_file() -> Self {
        use std::fs;
        // first check test-config/config.toml
        // then check /etc/hydra/config.toml
        // then use default config
        let data = if fs::metadata("test-config/config.toml").is_ok() {
            Some(fs::read_to_string("test-config/config.toml").expect("file should be present"))
        } else if fs::metadata("/etc/hydra/config.toml").is_ok() {
            Some(fs::read_to_string("/etc/hydra/config.toml").expect("file should be present"))
        } else {
            None
        };

        let config = match data {
            Some(data) => toml::from_str(&data).expect("error parsing config file"),
            None => {
                log::info!("Using default config...");
                Config {
                    use_https: false,
                    docker: DockerConfig {
                        cpu_set: "0".to_string(),
                        cpu_shares: 50_000,
                        memory: 192 * 1000 * 1000,
                        disk_read_rate: 10 * 1000 * 1000,
                        disk_write_rate: 10 * 1000 * 1000,
                        disk_max_size: 50 * 1000 * 1000,
                    },
                }
            }
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
