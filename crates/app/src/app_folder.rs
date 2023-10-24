use async_recursion;
use anyhow;
use futures;
use serde_json;
use std::collections::{HashMap,HashSet};
use std::cell::RefCell;
use std::fmt;
use thiserror;
use tvdb::models::{Episode, Series};
use tvdb::api::{LoginSession, ApiError};
use walkdir;
use std::path;
use tokio;
use enum_map;
use crate::app_folder_cache::{EpisodeKey, AppFolderCache};
use crate::file_intent::{FilterRules, Action, get_file_intent};

const PATH_STR_BOOKMARKS: &str = "bookmarks.json";
const PATH_STR_EPISODES_DATA: &str = "episodes.json";
const PATH_STR_SERIES_DATA: &str = "series.json";

#[derive(Debug, thiserror::Error)]
pub struct CacheSaveError; 

impl fmt::Display for CacheSaveError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

// Changes are queued by calling methods in AppFileContext
// We an update our conflict table from these queued changes by calling AppFolder.flush_file_changes()
pub struct ConflictTable {
    pending_writes: HashMap<String, HashSet<usize>>,
    existing_sources: HashMap<String, usize>,
    action_count: enum_map::EnumMap<Action, usize>,
}

pub struct AppFile {
    src: String,
    src_descriptor: Option<EpisodeKey>,
    action: Action,
    dest: String,
    is_enabled: bool,
}

#[derive(Debug, Clone)]
enum AppFileChange {
    SetAction(usize, Action),
    SetIsEnabled(usize, bool),
    SetDest(usize, String),
}

pub struct AppFileContext<'a> {
    index: usize,
    file: &'a AppFile,
    folder: &'a AppFolder<'a>,
}

pub struct AppFolder<'a> {
    root_path: String,
    filter_rules: &'a FilterRules,
    cache: Option<AppFolderCache>,

    file_table: Vec<AppFile>,
    conflict_table: ConflictTable,
    // Only one AppFileContext can perform a modification on the change queue
    change_queue: RefCell<Vec<AppFileChange>>,
}

impl ConflictTable {
    fn new() -> Self {
        Self {
            pending_writes: HashMap::new(),
            existing_sources: HashMap::new(),
            action_count: enum_map::enum_map!{ _ => 0 },
        }
    }

    fn clear(&mut self) {
        self.pending_writes.clear();
        self.existing_sources.clear();
    }

    fn insert_existing_source(&mut self, src: &str, index: usize) {
        self.existing_sources.insert(src.to_string(), index);
    }

    fn add_pending_write(&mut self, dest: &str, index: usize) {
        let entries = match self.pending_writes.get_mut(dest) {
            Some(entries) => entries,
            None => self.pending_writes.entry(dest.to_string()).or_insert(HashSet::new()),
        };
        entries.insert(index);
    }

    fn remove_pending_write(&mut self, dest: &str, index: usize) {
        let entries = match self.pending_writes.get_mut(dest) {
            Some(entries) => entries,
            None => self.pending_writes.entry(dest.to_string()).or_insert(HashSet::new()),
        };
        entries.remove(&index);
    }

    fn check_if_write_conflicts(&self, dest: &str) -> bool {
        if let Some(_) = self.existing_sources.get(dest) {
            return true;
        }

        if let Some(entries) = self.pending_writes.get(dest) {
            return entries.len() > 1usize;
        } 

        false
    }

    pub fn get_pending_writes(&self) -> &HashMap<String, HashSet<usize>> {
        &self.pending_writes
    }

    pub fn get_source_index(&self, src: &str) -> Option<&usize> {
        self.existing_sources.get(src)
    }

    pub fn get_action_count(&self) -> &enum_map::EnumMap<Action, usize> {
        &self.action_count
    }
}

impl<'a> AppFolder<'a> {
    pub fn new<'b>(root_path: &str, filter_rules: &'b FilterRules) -> Self 
    where 'b: 'a 
    {
        Self {
            root_path: root_path.to_string(),
            filter_rules,
            cache: None,

            file_table: Vec::new(),
            conflict_table: ConflictTable::new(),
            change_queue: RefCell::new(Vec::new()),
        }
    }
}

#[async_recursion::async_recursion]
async fn recursive_search_file_intents(root_path: &str, curr_folder: &str, cache: &AppFolderCache, intents: &mut Vec<AppFile>, rules: &FilterRules) -> Result<(), anyhow::Error> {
    let mut entries = tokio::fs::read_dir(curr_folder).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        if file_type.is_dir() {
            let path = entry.path();
            if let Some(sub_folder) = path.to_str() {
                recursive_search_file_intents(root_path, sub_folder, cache, intents, rules).await?;
            };
            continue;
        }

        if file_type.is_file() {
            let path = entry.path();
            let rel_path = path.strip_prefix(root_path)?;
            if let Some(rel_path) = rel_path.to_str() {
                let intent = get_file_intent(rel_path, rules, cache);
                let app_file = AppFile::new(
                    rel_path,
                    intent.descriptor,
                    intent.action,
                    intent.dest.as_str(),
                );
                intents.push(app_file);
            }
            continue;
        }
    }
    Ok(())
}

impl AppFolder<'_> {
    pub async fn update_file_intents(&mut self) -> Result<(), anyhow::Error>{
        let cache = self.cache.as_ref().ok_or(CacheSaveError)?;
        let mut new_file_table = Vec::<AppFile>::new();
        recursive_search_file_intents(self.root_path.as_str(), self.root_path.as_str(), cache, &mut new_file_table, &self.filter_rules).await?;

        // TODO: LOCK HERE
        self.file_table = new_file_table;
        self.conflict_table.clear();
        self.change_queue.borrow_mut().clear();

        // seed conflict table
        for (index, file) in self.file_table.iter().enumerate() {
            self.conflict_table.insert_existing_source(file.src.as_str(), index);
            self.conflict_table.action_count[file.action] += 1usize;
        }

        // automatically enable renames
        for index in 0..self.get_total_files() {
            if let Some(mut file) = self.get_file(index) {
                if file.get_action() == Action::Rename {
                    file.set_is_enabled(true); 
                }
            }
        }
        Ok(())
    }

    pub async fn load_cache_from_file(&mut self) -> Result<(), anyhow::Error> {
        let (series_data, episodes_data) = tokio::join!(
            tokio::fs::read_to_string(format!("{}/{}", self.root_path, PATH_STR_SERIES_DATA)),
            tokio::fs::read_to_string(format!("{}/{}", self.root_path, PATH_STR_EPISODES_DATA))
        );

        let series: Series = {
            serde_json::from_str(series_data?.as_str())?
        };

        let episodes: Vec<Episode> = {
            serde_json::from_str(episodes_data?.as_str())?
        };

        // TODO: LOCK HERE
        self.cache = Some(AppFolderCache::new(series, episodes));
        Ok(())
    }

    pub async fn load_cache_from_api(&mut self, session: &LoginSession<'_>, series_id: u32) -> Result<(), ApiError> {
        let (series, episodes) = tokio::join!(
            session.get_series(series_id),
            session.get_episodes(series_id),
        );

        let series = series?;
        let episodes = episodes?;

        // TODO: LOCK HERE
        self.cache = Some(AppFolderCache::new(series, episodes));
        Ok(())
    }

    pub async fn save_cache_to_file(&self) -> Result<(), anyhow::Error> {
        // TODO: LOCK HERE
        let cache = self.cache.as_ref().ok_or(CacheSaveError)?;
        let series_str = serde_json::to_string_pretty(&cache.series)?;
        let episodes_str = serde_json::to_string_pretty(&cache.episodes)?;
        // TODO: UNLOCK HERE
        
        tokio::try_join!(
            tokio::fs::write(format!("{}/{}", self.root_path, PATH_STR_SERIES_DATA), series_str),
            tokio::fs::write(format!("{}/{}", self.root_path, PATH_STR_EPISODES_DATA), episodes_str),
        )?;
        
        Ok(())
    }

    // NOTE: This should be done in UI thread after each frame
    pub fn flush_file_changes(&mut self) -> usize {
        let mut total_changes: usize = 0;
        for file_change in self.change_queue.borrow().iter() {
            match file_change {
                AppFileChange::SetAction(index, new_action) => {
                    let index = *index;
                    let new_action = *new_action;
                    let file = match self.file_table.get_mut(index) {
                        Some(file) => file,
                        None => continue,
                    };

                    let old_action = file.action;
                    file.action = new_action;

                    if old_action == new_action {
                        continue;
                    }

                    self.conflict_table.action_count[old_action] -= 1usize;
                    self.conflict_table.action_count[new_action] += 1usize;

                    if !file.is_enabled {
                        continue;
                    };

                    if old_action != Action::Rename && new_action != Action::Rename {
                        continue;
                    }

                    if old_action == Action::Rename {
                        self.conflict_table.remove_pending_write(file.dest.as_str(), index);
                    } else {
                        self.conflict_table.add_pending_write(file.dest.as_str(), index);
                    };
                    total_changes += 1;
                },
                AppFileChange::SetIsEnabled(index, new_is_enabled) => {
                    let index = *index;
                    let new_is_enabled = *new_is_enabled;
                    let file = match self.file_table.get_mut(index) {
                        Some(file) => file,
                        None => continue,
                    };

                    let old_is_enabled = file.is_enabled;
                    file.is_enabled = new_is_enabled;

                    if old_is_enabled == new_is_enabled {
                        continue;
                    }

                    if file.action != Action::Rename {
                        continue;
                    }

                    if new_is_enabled {
                        self.conflict_table.add_pending_write(file.dest.as_str(), index);
                    } else {
                        self.conflict_table.remove_pending_write(file.dest.as_str(), index);
                    };
                    total_changes += 1;
                },
                AppFileChange::SetDest(index, new_dest) => {
                    let index = *index;
                    let file = match self.file_table.get_mut(index) {
                        Some(file) => file,
                        None => continue,
                    };

                    if file.dest.as_str() == new_dest {
                        continue
                    }

                    // We perform a .clear() and .push_str(...) to avoid a short lived clone
                    if !file.is_enabled || file.action != Action::Rename {
                        file.dest.clear();
                        file.dest.push_str(new_dest.as_str());
                        continue
                    }

                    self.conflict_table.remove_pending_write(file.dest.as_str(), index);
                    self.conflict_table.add_pending_write(new_dest.as_str(), index);

                    file.dest.clear();
                    file.dest.push_str(new_dest.as_str());
                    total_changes += 1;
                },
            }
        }

        self.change_queue.borrow_mut().clear();
        total_changes
    }

    pub async fn execute_file_changes(&mut self) {
        use std::pin::Pin;
        use std::future::Future;
        type F = Pin<Box<dyn Future<Output = Result<(), std::io::Error>>>>;

        // TODO: LOCK HERE
        let mut tasks = Vec::<F>::new();
        for file_index in 0..self.get_total_files() {
            if let Some(file) = self.get_file(file_index) {
                if !file.get_is_enabled() {
                    continue;
                }

                if file.get_action() == Action::Delete {
                    let src = path::Path::new(&self.root_path).join(file.get_src());
                    tasks.push(Box::pin({
                        async move {
                            tokio::fs::remove_file(src).await
                        }
                    }));
                    continue;
                }

                if file.get_action() == Action::Rename && !file.get_is_conflict() {
                    tasks.push(Box::pin({
                        let src = path::Path::new(&self.root_path).join(file.get_src());
                        let dest = path::Path::new(&self.root_path).join(file.get_dest());
                        async move {
                            let parent_dir = dest.parent().expect("Invalid filepath");
                            tokio::fs::create_dir_all(parent_dir).await?;
                            tokio::fs::rename(src, dest).await
                        }
                    }));
                    continue;
                }
            }
        }
        // TODO: UNLOCK

        for res in futures::future::join_all(tasks).await.into_iter() {
            if let Err(err) = res {
                // TODO: Error logging
                println!("{:?}", err);
            };
        }
    }

    pub async fn delete_empty_folders(&self) {
        let mut tasks = Vec::new();

        let walker = walkdir::WalkDir::new(self.root_path.as_str())
            .max_depth(1)
            .follow_links(false); // Don't follow symbolic links
                                  //
        for entry_res in walker {
            let entry = match entry_res {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            if !entry.file_type().is_dir() {
                continue;
            }

            let is_empty = self.check_folder_empty(entry.path());
            if !is_empty {
                continue;
            }

            tasks.push({
                async move {
                    tokio::fs::remove_dir_all(entry.path()).await
                }
            });
        }

        for res in futures::future::join_all(tasks).await.into_iter() {
            if let Err(err) = res {
                // TODO: Error logging
                println!("{:?}", err);
            };
        }
    }

    pub fn get_conflict_table(&self) -> &ConflictTable {
        &self.conflict_table
    }

    pub fn get_total_files(&self) -> usize {
        self.file_table.len()
    }
    
    pub fn get_file(&self, index: usize) -> Option<AppFileContext<'_>> {
        let file = self.file_table.get(index)?;
        Some(AppFileContext {
            index,
            file,
            folder: self,
        })
    }

    fn check_folder_empty(&self, path: &path::Path) -> bool {
        for entry_res in walkdir::WalkDir::new(path) {
            if let Ok(entry) = entry_res {
                if entry.file_type().is_file() {
                    return false;
                }
            }
        }
        true
    }
}

impl AppFile {
    pub fn new(src: &str, src_descriptor: Option<EpisodeKey>, action: Action, dest: &str) -> Self {
        Self {
            src: src.to_string(),
            src_descriptor,
            action,
            dest: dest.to_string(),
            is_enabled: false,
        }
    }
}

impl AppFileContext<'_> {
    pub fn get_src(&self) -> &str {
        self.file.src.as_str() 
    }

    pub fn get_src_descriptor(&self) -> &Option<EpisodeKey> {
        &self.file.src_descriptor
    }

    pub fn get_action(&self) -> Action {
        self.file.action
    }

    pub fn get_dest(&self) -> &str {
        self.file.dest.as_str()
    }

    pub fn get_is_enabled(&self) -> bool {
        self.file.is_enabled
    }

    pub fn get_is_conflict(&self) -> bool {
        if !self.file.is_enabled || self.file.action != Action::Rename {
            return false;
        }

        self.folder.conflict_table.check_if_write_conflicts(self.file.dest.as_str())
    }

    pub fn get_episode(&self) -> Option<&Episode> {
        let descriptor = self.file.src_descriptor.as_ref()?;
        let cache = self.folder.cache.as_ref()?;
        let episode_index = *cache.episode_cache.get(descriptor)?;
        cache.episodes.get(episode_index)
    }

    pub fn set_action(&mut self, new_action: Action) {
        let mut queue = self.folder.change_queue.borrow_mut();
        queue.push(AppFileChange::SetAction(self.index, new_action));
        // Automatically set destination to src is not set
        if self.file.action != Action::Rename && new_action == Action::Rename && self.file.dest.len() == 0 {
            queue.push(AppFileChange::SetDest(self.index, self.file.src.to_owned())); 
        }
    }

    pub fn set_is_enabled(&mut self, new_is_enabled: bool) {
        let change = AppFileChange::SetIsEnabled(self.index, new_is_enabled);
        self.folder.change_queue.borrow_mut().push(change);
    }

    pub fn set_dest(&mut self, new_dest: &str) {
        let change = AppFileChange::SetDest(self.index, new_dest.to_string());
        self.folder.change_queue.borrow_mut().push(change);
    }
}

impl fmt::Debug for AppFileContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppFilecontext")
            .field("src", &self.get_src())
            .field("src_descriptor", &self.get_src_descriptor())
            .field("action", &self.get_action())
            .field("dest", &self.get_dest())
            .field("is_enabled", &self.get_is_enabled())
            .field("is_conflict", &self.get_is_conflict())
            .finish()
    }
}
