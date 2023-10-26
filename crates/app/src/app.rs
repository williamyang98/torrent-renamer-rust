use anyhow;
use reqwest;
use serde;
use serde_json;
use tokio;
use tokio::sync::{RwLock, Mutex};
use tvdb::api::LoginSession;
use tvdb::models::Series;
use crate::file_intent::FilterRules;
use crate::app_folder::AppFolder;
use std::sync::Arc;
use std::fmt;
use thiserror;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Credentials {
    #[serde(rename="credentials")]
    pub login_info: tvdb::api::LoginInfo,     
    pub token: String,
}

#[derive(Debug, thiserror::Error)]
pub struct LoggedOutError; 
impl fmt::Display for LoggedOutError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct App {
    filter_rules: Arc<FilterRules>,
    credentials: Credentials,
    client: Arc<reqwest::Client>,
    login_session: Arc<RwLock<Option<Arc<LoginSession>>>>,

    folders: Arc<RwLock<Vec<Arc<AppFolder>>>>,
    selected_folder_index: Arc<RwLock<Option<usize>>>,
    folders_busy_lock: Arc<Mutex<()>>,

    series: Arc<RwLock<Option<Vec<Series>>>>,
    selected_series_index: Arc<RwLock<Option<usize>>>,
    series_busy_lock: Arc<Mutex<()>>,
}

impl App {
    pub async fn new(config_path: &str) -> Result<App, anyhow::Error> {
        let (filter_rules_str, credentials_str) = tokio::join!(
            tokio::fs::read_to_string(format!("{}/app_config.json", config_path)),
            tokio::fs::read_to_string(format!("{}/credentials.json", config_path)),
        );

        let filter_rules: FilterRules = serde_json::from_str(filter_rules_str?.as_str())?;
        let credentials: Credentials = serde_json::from_str(credentials_str?.as_str())?;
        
        Ok(App {
            filter_rules: Arc::new(filter_rules),
            credentials,
            client: Arc::new(reqwest::Client::new()),
            login_session: Arc::new(RwLock::new(None)),

            folders: Arc::new(RwLock::new(Vec::new())),
            selected_folder_index: Arc::new(RwLock::new(None)),
            folders_busy_lock: Arc::new(Mutex::new(())),

            series: Arc::new(RwLock::new(None)),
            selected_series_index: Arc::new(RwLock::new(None)),
            series_busy_lock: Arc::new(Mutex::new(())),
        })
    }
}

impl App {
    pub async fn login(&self) -> Result<(), anyhow::Error> {
        let token = tvdb::api::login(self.client.as_ref(), &self.credentials.login_info).await?;
        let session = LoginSession::new(self.client.clone(), &token);
        *self.login_session.write().await = Some(Arc::new(session));
        Ok(())
    }

    pub fn get_login_session(&self) -> &Arc<RwLock<Option<Arc<LoginSession>>>> {
        &self.login_session
    }

    pub async fn open_folders(&self, root_path: &str) -> Result<(), anyhow::Error> {
        let _busy_lock = self.folders_busy_lock.lock().await;

        let mut new_folders = Vec::new();
        let mut entries = tokio::fs::read_dir(root_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            if !file_type.is_dir() {
                continue;
            }

            let path = entry.path();
            if let Some(path) = path.to_str() {
                let folder = AppFolder::new(root_path, path, self.filter_rules.clone());
                new_folders.push(Arc::new(folder));
            }
        }
        
        new_folders.sort_by(|a, b| {
            let a = a.as_ref();
            let b = b.as_ref();
            let a_name = a.get_folder_name();
            let b_name = b.get_folder_name();
            a_name.partial_cmp(b_name).unwrap_or(std::cmp::Ordering::Equal)
        });

        let (mut folders, mut selected_folder_index) = tokio::join!(
            self.folders.write(),
            self.selected_folder_index.write(),
        );
        *folders = new_folders;
        *selected_folder_index = None;
        Ok(())
    }

    pub fn get_folders_busy_lock(&self) -> &Arc<Mutex<()>> {
        &self.folders_busy_lock
    }

    pub fn get_folders(&self) -> &Arc<RwLock<Vec<Arc<AppFolder>>>> {
        &self.folders
    }

    pub fn get_selected_folder_index(&self) -> &Arc<RwLock<Option<usize>>> {
        &self.selected_folder_index 
    }

    pub fn get_series(&self) -> &Arc<RwLock<Option<Vec<Series>>>> {
        &self.series
    }

    pub fn get_selected_series_index(&self) -> &Arc<RwLock<Option<usize>>> {
        &self.selected_series_index
    }

    pub fn get_series_busy_lock(&self) -> &Arc<Mutex<()>> {
        &self.series_busy_lock
    }

    pub async fn update_search_series(&self, search: String) -> Result<(), anyhow::Error> {
        let _busy_lock = self.series_busy_lock.lock().await;
        let login_session = self.login_session.read().await;
        let session = login_session.as_ref().ok_or(LoggedOutError)?;
        let search_results = session.search_series(&search).await?;

        let (mut series, mut series_index) = tokio::join!(
            self.series.write(),
            self.selected_series_index.write(),
        );
        *series = Some(search_results);
        *series_index = None;
        Ok(())
    }

    pub async fn set_series_to_current_folder(&self, series_id: u32) -> Option<()> {
        let (folders_guard, selected_index_guard, session_guard) = tokio::join!(
            self.folders.read(),
            self.selected_folder_index.read(),
            self.login_session.read(),
        );

        let session = match session_guard.as_ref() {
            Some(session) => session.clone(),
            None => return Some(()),
        };
        
        let selected_index = match *selected_index_guard {
            Some(index) => index,
            None => return Some(()),
        };

        let folder = &folders_guard[selected_index];
        let folder = folder.clone();
        drop(folders_guard);
        drop(selected_index_guard);

        folder.load_cache_from_api(session, series_id).await?; 
        drop(session_guard);

        tokio::join!(
            folder.update_file_intents(),
            folder.save_cache_to_file(),
        );
        Some(())
    }
}
