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
use thiserror;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Credentials {
    #[serde(rename="credentials")]
    pub login_info: tvdb::api::LoginInfo,     
    pub token: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppInitError {
    #[error("failed to load filter rules from file: {}", .0)]
    IOFilterRulesLoad(std::io::Error),
    #[error("failed to load credentials from file: {}", .0)]
    IOCredentialsLoad(std::io::Error),
    #[error("json decode on filter rules: {}", .0)]
    JsonDecodeFilterRules(serde_json::Error),
    #[error("json decode on credentials: {}", .0)]
    JsonDecodeCredentials(serde_json::Error),
}

pub struct App {
    filter_rules: Arc<FilterRules>,
    credentials: Credentials,
    client: Arc<reqwest::Client>,
    login_session: Arc<RwLock<Option<Arc<LoginSession>>>>,
    
    root_path: Arc<RwLock<String>>,
    folders: Arc<RwLock<Vec<Arc<AppFolder>>>>,
    selected_folder_index: Arc<RwLock<Option<usize>>>,
    folders_busy_lock: Arc<Mutex<()>>,

    series: Arc<RwLock<Option<Vec<Series>>>>,
    selected_series_index: Arc<RwLock<Option<usize>>>,
    series_busy_lock: Arc<Mutex<()>>,

    errors: Arc<RwLock<Vec<String>>>,
}

impl App {
    pub async fn new(config_path: &str) -> Result<App, AppInitError> {
        let (filter_rules_str, credentials_str) = tokio::join!(
            tokio::fs::read_to_string(format!("{}/app_config.json", config_path)),
            tokio::fs::read_to_string(format!("{}/credentials.json", config_path)),
        );

        let filter_rules_str = filter_rules_str.map_err(AppInitError::IOFilterRulesLoad)?;
        let credentials_str = credentials_str.map_err(AppInitError::IOCredentialsLoad)?;

        let filter_rules: FilterRules = serde_json::from_str(filter_rules_str.as_str())
            .map_err(AppInitError::JsonDecodeFilterRules)?;
        let credentials: Credentials = serde_json::from_str(credentials_str.as_str())
            .map_err(AppInitError::JsonDecodeCredentials)?;
        
        Ok(App {
            filter_rules: Arc::new(filter_rules),
            credentials,
            client: Arc::new(reqwest::Client::new()),
            login_session: Arc::new(RwLock::new(None)),
            
            root_path: Arc::new(RwLock::new(".".to_string())),
            folders: Arc::new(RwLock::new(Vec::new())),
            selected_folder_index: Arc::new(RwLock::new(None)),
            folders_busy_lock: Arc::new(Mutex::new(())),

            series: Arc::new(RwLock::new(None)),
            selected_series_index: Arc::new(RwLock::new(None)),
            series_busy_lock: Arc::new(Mutex::new(())),

            errors: Arc::new(RwLock::new(Vec::new())),
        })
    }
}

impl App {
    pub async fn login(&self) -> Option<()> {
        let token = tvdb::api::login(self.client.as_ref(), &self.credentials.login_info).await;
        let token = match token {
            Ok(token) => token,
            Err(err) => {
                let message = format!("Error on tvdb api login: {}", err);
                self.errors.write().await.push(message);
                return None;
            },
        };

        let session = LoginSession::new(self.client.clone(), &token);
        *self.login_session.write().await = Some(Arc::new(session));
        Some(())
    }

    pub fn get_login_session(&self) -> &Arc<RwLock<Option<Arc<LoginSession>>>> {
        &self.login_session
    }

    pub async fn load_folders_from_existing_root_path(&self) -> Option<()> {
        let path = self.root_path.read().await.clone();
        self.load_folders(path).await
    }

    pub async fn load_folders(&self, root_path: String) -> Option<()> {
        let _busy_lock = self.folders_busy_lock.lock().await;
        // NOTE: If for some reason the folder load failed we can still reattempt 
        *self.root_path.write().await = root_path.clone();

        let mut new_folders = Vec::new();
        let entries = tokio::fs::read_dir(root_path.as_str()).await; 
        let mut entries = match entries {
            Ok(entries) => entries,
            Err(err) => {
                let message = format!("Error on loading folders from '{}': {}", root_path.as_str(), err);
                self.errors.write().await.push(message);
                return None;
            },
        };

        loop {
            let entry_opt = match entries.next_entry().await {
                Ok(entry_opt) => entry_opt,
                Err(err) => {
                    let message = format!("Error during iteraton when getting next entry from folder '{}': {}", root_path.as_str(), err);
                    self.errors.write().await.push(message);
                    return None;
                },
            };

            let entry = match entry_opt {
                Some(entry) => entry,
                None => break,
            };

            let path = entry.path();
            let file_type = match entry.file_type().await {
                Ok(file_type) => file_type,
                Err(err) => {
                    let path_str = path.to_str().unwrap_or(root_path.as_str());
                    let message = format!("Error during iteration when getting file type from folder '{}': {}", path_str, err);
                    self.errors.write().await.push(message);
                    return None;
                },
            };

            if !file_type.is_dir() {
                continue;
            }

            if let Some(path) = path.to_str() {
                let folder = AppFolder::new(root_path.as_str(), path, self.filter_rules.clone());
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
        Some(())
    }

    pub async fn update_search_series(&self, search: String) -> Option<()> {
        let _busy_lock = self.series_busy_lock.lock().await;
        let login_session = self.login_session.read().await;
        let session = match login_session.as_ref() {
            Some(session) => session,
            None => {
                let message = "Login session is required to update the series search results";
                self.errors.write().await.push(message.to_string());
                return None;
            },
        };
        let search_results = match session.search_series(&search).await {
            Ok(results) => results,
            Err(err) => {
                let message = format!("Failed to get series search results due to api error: {}", err);
                self.errors.write().await.push(message);
                return None;
            },
        };

        let (mut series, mut series_index) = tokio::join!(
            self.series.write(),
            self.selected_series_index.write(),
        );
        *series = Some(search_results);
        *series_index = None;
        Some(())
    }

    pub async fn set_series_to_current_folder(&self, series_id: u32) -> Option<()> {
        let (folders_guard, selected_index_guard, session_guard) = tokio::join!(
            self.folders.read(),
            self.selected_folder_index.read(),
            self.login_session.read(),
        );

        let session = match session_guard.as_ref() {
            Some(session) => session.clone(),
            None => {
                let message = "Could not set update folder series from api since no login session exists";
                self.errors.write().await.push(message.to_string());
                return None;
            },
        };
        
        let selected_index = match *selected_index_guard {
            Some(index) => index,
            None => {
                let message = "Could not set update folder series from api since no folder is selected currently";
                self.errors.write().await.push(message.to_string());
                return None;
            },
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

    pub async fn update_file_intents_for_all_folders(&self) -> Option<()> {
        // Allow the folder to be read while it is busy
        // Disallow load_folders(...) while we are performing an update on all folders
        let _busy_lock = self.folders_busy_lock.lock().await;
        let mut tasks = Vec::new();
        {
            let folders = self.folders.as_ref().read().await;
            for folder in folders.iter() {
                let folder = folder.clone();
                let task = async move {
                    if folder.perform_initial_load().await == None {
                        folder.update_file_intents().await;
                    }
                };
                tasks.push(task);
            }
        }
        let _ = futures::future::join_all(tasks).await;
        Some(())
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

    pub fn get_errors(&self) -> &Arc<RwLock<Vec<String>>> {
        &self.errors
    }
}
