use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: i32,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub session: Session,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub order: i64,
    #[serde(default)]
    pub targets: Vec<TargetRule>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRule {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub dir: String,
    #[serde(default = "default_match_type")]
    pub match_type: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    #[serde(default)]
    pub active_category_id: Option<String>,
    #[serde(default)]
    pub search_text: String,
}

fn default_version() -> i32 {
    1
}

fn default_match_type() -> String {
    "glob".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: 1,
            categories: vec![],
            session: Session::default(),
        }
    }
}

pub fn app_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("FileDock")
}

pub fn config_path() -> PathBuf {
    app_config_dir().join("config.json")
}

pub fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_config_file() -> Config {
    let path = config_path();
    if !path.exists() {
        return Config::default();
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Config::default(),
    };
    match serde_json::from_str::<Config>(&content) {
        Ok(cfg) => cfg,
        Err(_) => Config::default(),
    }
}

pub fn save_config_file(config: &Config) -> Result<(), String> {
    atomic_write(
        &config_path(),
        &serde_json::to_string_pretty(config).map_err(|e| e.to_string())?,
    )
}

pub fn init_config_file() -> Result<(), String> {
    let dir = app_config_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = config_path();
    if !path.exists() {
        let default = Config::default();
        atomic_write(
            &path,
            &serde_json::to_string_pretty(&default).map_err(|e| e.to_string())?,
        )?;
    } else {
        let cfg = load_config_file();
        atomic_write(
            &path,
            &serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?,
        )?;
    }

    Ok(())
}
