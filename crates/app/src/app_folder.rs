use async_recursion;
use futures;
use serde_json;
use std::collections::{HashMap,HashSet};
use std::sync::Arc;
use std::fmt;
use tvdb::models::{Episode, Series};
use tvdb::api::{LoginSession, ApiError};
use walkdir;
use std::path;
use tokio;
use tokio::sync::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use enum_map;
use crate::app_folder_cache::{EpisodeKey, AppFolderCache};
use crate::file_intent::{FilterRules, Action, get_file_intent};

const PATH_STR_BOOKMARKS: &str = "bookmarks.json";
const PATH_STR_EPISODES_DATA: &str = "episodes.json";
const PATH_STR_SERIES_DATA: &str = "series.json";

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

pub struct AppFileImmutableContext<'a> {
    file_table: RwLockReadGuard<'a, Vec<AppFile>>,
    conflict_table: RwLockReadGuard<'a, ConflictTable>,
}

pub struct AppFileMutableContext<'a> {
    file_table: RwLockReadGuard<'a, Vec<AppFile>>,
    conflict_table: RwLockReadGuard<'a, ConflictTable>,
    change_queue: RwLockWriteGuard<'a, Vec<AppFileChange>>,
}

pub struct AppFolder {
    folder_path: String,
    folder_name: String,
    filter_rules: Arc<FilterRules>,
    cache: Arc<RwLock<Option<AppFolderCache>>>,

    file_table: Arc<RwLock<Vec<AppFile>>>,
    conflict_table: Arc<RwLock<ConflictTable>>,
    // Only one AppFileContext can perform a modification on the change queue
    change_queue: Arc<RwLock<Vec<AppFileChange>>>,

    errors: Arc<RwLock<Vec<String>>>,
    busy_lock: Arc<Mutex<()>>,
    selected_descriptor: Arc<RwLock<Option<EpisodeKey>>>,
    is_initial_load: Arc<RwLock<bool>>,
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
        self.action_count.clear();
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

impl AppFolder {
    pub fn new(root_path: &str, folder_path: &str, filter_rules: Arc<FilterRules>) -> Self {
        let folder_name = match path::Path::new(folder_path).strip_prefix(root_path) {
            Ok(name) => name.to_string_lossy().to_string(), 
            Err(_) => folder_path.to_string(),
        };

        Self {
            folder_path: folder_path.to_string(),
            folder_name,
            filter_rules,
            cache: Arc::new(RwLock::new(None)),
            file_table: Arc::new(RwLock::new(Vec::new())),
            conflict_table: Arc::new(RwLock::new(ConflictTable::new())),
            change_queue: Arc::new(RwLock::new(Vec::new())),
            errors: Arc::new(RwLock::new(Vec::new())),
            busy_lock: Arc::new(Mutex::new(())),
            selected_descriptor: Arc::new(RwLock::new(None)),
            is_initial_load: Arc::new(RwLock::new(false)),
        }
    }
}

#[async_recursion::async_recursion]
async fn recursive_search_file_intents(root_path: &str, curr_folder: &str, cache: &AppFolderCache, intents: &mut Vec<AppFile>, rules: &FilterRules) -> Result<(), std::io::Error> {
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
            let rel_path = match path.strip_prefix(root_path) {
                Ok(rel_path) => rel_path,
                Err(_) => continue,
            };

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

fn check_folder_empty(path: &path::Path) -> bool {
    for entry_res in walkdir::WalkDir::new(path) {
        if let Ok(entry) = entry_res {
            if entry.file_type().is_file() {
                return false;
            }
        }
    }
    true
}

impl AppFolder {
    pub async fn update_file_intents(&self) -> Option<()> {
        let _busy_lock = self.busy_lock.lock().await;

        let mut new_file_table = Vec::<AppFile>::new();
        {
            let cache_guard = self.cache.read().await;
            let cache = match cache_guard.as_ref() {
                Some(cache) => cache,
                None => {
                    let message = "Couldn't update file intents since cache is unloaded";
                    self.errors.write().await.push(message.to_string()); 
                    return None;
                },
            };
            let res = recursive_search_file_intents(self.folder_path.as_str(), self.folder_path.as_str(), cache, &mut new_file_table, &self.filter_rules).await;
            if let Err(err) = res {
                let message = format!("IO error while reading files for intent update: {:?}", err);
                self.errors.write().await.push(message);
                return None;
            }
        }

        new_file_table.sort_by(|a,b| {
            let a_name = a.src.as_str();
            let b_name = b.src.as_str();
            a_name.partial_cmp(b_name).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        {
            let (mut file_table, mut conflict_table, mut change_queue) = tokio::join!(
                self.file_table.write(),
                self.conflict_table.write(),
                self.change_queue.write(),
            );

            *file_table = new_file_table;
            conflict_table.clear();
            change_queue.clear();

            // seed conflict table
            for (index, file) in file_table.iter().enumerate() {
                conflict_table.insert_existing_source(file.src.as_str(), index);
                conflict_table.action_count[file.action] += 1usize;
            }
        }

        {
            // automatically enable renames
            let mut files = self.get_mut_files().await;
            for i in 0..files.get_total_items() {
                if files.get_action(i) == Action::Rename {
                    files.set_is_enabled(true, i); 
                }
            }
        }

        self.flush_file_changes().await;
        Some(())
    }

    pub async fn load_cache_from_file(&self) -> Option<()> {
        let _busy_lock = self.busy_lock.lock().await;

        let (series_data, episodes_data) = tokio::join!(
            tokio::fs::read_to_string(format!("{}/{}", self.folder_path, PATH_STR_SERIES_DATA)),
            tokio::fs::read_to_string(format!("{}/{}", self.folder_path, PATH_STR_EPISODES_DATA))
        );
        
        if let Err(err) = series_data.as_ref() {
            let message = format!("IO error while reading series cache: {:?}", err);
            self.errors.write().await.push(message);
        }

        if let Err(err) = episodes_data.as_ref() {
            let message = format!("IO error while reading episodes cache: {:?}", err);
            self.errors.write().await.push(message);
        }

        let series_data = series_data.as_ref().ok()?;
        let episodes_data = episodes_data.as_ref().ok()?;

        let series: Series = match serde_json::from_str(series_data.as_str()) {
            Ok(series) => series,
            Err(err) => {
                let message = format!("JSON decoding error reading series from file: {:?}", err);
                self.errors.write().await.push(message);
                return None;
            },
        };

        let episodes: Vec<Episode> = match serde_json::from_str(episodes_data.as_str()) {
            Ok(episodes) => episodes,
            Err(err) => {
                let message = format!("JSON decoding error reading episodes from file: {:?}", err);
                self.errors.write().await.push(message);
                return None;
            },
        };

        let mut cache = self.cache.write().await;
        *cache = Some(AppFolderCache::new(series, episodes));
        Some(())
    }

    pub async fn is_cache_loaded(&self) -> bool {
        self.cache.read().await.is_some()
    }

    pub async fn load_cache_from_api(&self, session: Arc<LoginSession>, series_id: u32) -> Option<()> {
        let _busy_lock = self.busy_lock.lock().await;

        let (series_res, episodes_res) = tokio::join!(
            session.get_series(series_id),
            session.get_episodes(series_id),
        );

        let series = match series_res {
            Ok(series) => series,
            Err(err) => {
                let message = format!("Api error while fetching series: {:?}", err);
                self.errors.write().await.push(message);
                return None;
            },
        };

        let episodes = match episodes_res {
            Ok(episodes) => episodes,
            Err(err) => {
                let message = format!("Api error while fetching episodes: {:?}", err);
                self.errors.write().await.push(message);
                return None;
            },
        };

        let mut cache = self.cache.write().await;
        *cache = Some(AppFolderCache::new(series, episodes));
        Some(())
    }

    pub async fn refresh_cache_from_api(&self, session: Arc<LoginSession>) -> Option<()> {
        let series_id = {
            let cache_guard = self.cache.read().await;
            match cache_guard.as_ref() {
                Some(cache) => cache.series.id,
                None => {
                    let message = format!("Couldn't save cache to file since it is unloaded");
                    self.errors.write().await.push(message);
                    return None;
                },
            }
        };
        self.load_cache_from_api(session, series_id).await
    }

    pub async fn save_cache_to_file(&self) -> Option<()> {
        let _busy_lock = self.busy_lock.lock().await;

        let (series_str, episodes_str) = {
            let cache_guard = self.cache.read().await;
            let cache = match cache_guard.as_ref() {
                Some(cache) => cache,
                None => {
                    let message = format!("Couldn't save cache to file since it is unloaded");
                    self.errors.write().await.push(message);
                    return None;
                },
            };
            let series_str = match serde_json::to_string_pretty(&cache.series) {
                Ok(data) => data,
                Err(err) => {
                    let message = format!("JSON encode error when saving series cache: {:?}", err);
                    self.errors.write().await.push(message);
                    return None;
                },
            };
            let episodes_str = match serde_json::to_string_pretty(&cache.episodes) {
                Ok(data) => data,
                Err(err) => {
                    let message = format!("JSON encode error when saving episodes cache: {:?}", err);
                    self.errors.write().await.push(message);
                    return None;
                },
            };
            (series_str, episodes_str)
        };

        let (res_0, res_1) = tokio::join!(
            tokio::fs::write(format!("{}/{}", self.folder_path, PATH_STR_SERIES_DATA), series_str),
            tokio::fs::write(format!("{}/{}", self.folder_path, PATH_STR_EPISODES_DATA), episodes_str),
        );

        if let Err(err) = res_0.as_ref() {
            let message = format!("IO error while saving series cache: {:?}", err);
            self.errors.write().await.push(message);
        }

        if let Err(err) = res_1.as_ref() {
            let message = format!("IO error while saving episodes cache: {:?}", err);
            self.errors.write().await.push(message);
        }
        
        if res_0.is_err() || res_1.is_err() {
            return None;
        }
        Some(())
    }
    
    pub async fn flush_file_changes(&self) -> usize {
        let (mut file_table, mut conflict_table, mut change_queue) = tokio::join!(
            self.file_table.write(),
            self.conflict_table.write(),
            self.change_queue.write(),
        );

        let mut total_changes: usize = 0;
        for file_change in change_queue.iter() {
            match file_change {
                AppFileChange::SetAction(index, new_action) => {
                    let index = *index;
                    let new_action = *new_action;
                    let file = match file_table.get_mut(index) {
                        Some(file) => file,
                        None => continue,
                    };

                    let old_action = file.action;
                    file.action = new_action;

                    if old_action == new_action {
                        continue;
                    }

                    conflict_table.action_count[old_action] -= 1usize;
                    conflict_table.action_count[new_action] += 1usize;

                    if !file.is_enabled {
                        continue;
                    };

                    if old_action != Action::Rename && new_action != Action::Rename {
                        continue;
                    }

                    if old_action == Action::Rename {
                        conflict_table.remove_pending_write(file.dest.as_str(), index);
                    } else {
                        conflict_table.add_pending_write(file.dest.as_str(), index);
                    };
                    total_changes += 1;
                },
                AppFileChange::SetIsEnabled(index, new_is_enabled) => {
                    let index = *index;
                    let new_is_enabled = *new_is_enabled;
                    let file = match file_table.get_mut(index) {
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
                        conflict_table.add_pending_write(file.dest.as_str(), index);
                    } else {
                        conflict_table.remove_pending_write(file.dest.as_str(), index);
                    };
                    total_changes += 1;
                },
                AppFileChange::SetDest(index, new_dest) => {
                    let index = *index;
                    let file = match file_table.get_mut(index) {
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

                    conflict_table.remove_pending_write(file.dest.as_str(), index);
                    conflict_table.add_pending_write(new_dest.as_str(), index);

                    file.dest.clear();
                    file.dest.push_str(new_dest.as_str());
                    total_changes += 1;
                },
            }
        }

        change_queue.clear();
        total_changes
    }

    pub async fn execute_file_changes(&self) {
        let _busy_lock = self.busy_lock.lock().await;

        use std::pin::Pin;
        use std::future::Future;
        type F = Pin<Box<dyn Future<Output = Result<(), std::io::Error>> + Send>>;

        let mut tasks = Vec::<F>::new();
        {
            let files = self.get_files().await;
            for i in 0..files.get_total_items() {
                if !files.get_is_enabled(i) {
                    continue;
                }

                if files.get_action(i) == Action::Delete {
                    let src = path::Path::new(&self.folder_path).join(files.get_src(i));
                    tasks.push(Box::pin({
                        async move {
                            tokio::fs::remove_file(src).await
                        }
                    }));
                    continue;
                }

                if files.get_action(i) == Action::Rename && !files.get_is_conflict(i) {
                    tasks.push(Box::pin({
                        let src = path::Path::new(&self.folder_path).join(files.get_src(i));
                        let dest = path::Path::new(&self.folder_path).join(files.get_dest(i));
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

        for res in futures::future::join_all(tasks).await.into_iter() {
            if let Err(err) = res {
                // TODO: Error logging
                println!("{:?}", err);
            };
        }
    }

    pub async fn delete_empty_folders(&self) {
        let mut tasks = Vec::new();

        let walker = walkdir::WalkDir::new(self.folder_path.as_str())
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

            let is_empty = check_folder_empty(entry.path());
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

    pub fn get_folder_path(&self) -> &str {
        self.folder_path.as_str() 
    }

    pub fn get_folder_name(&self) -> &str {
        self.folder_name.as_str() 
    }

    pub fn get_conflict_table(&self) -> &Arc<RwLock<ConflictTable>> {
        &self.conflict_table
    }

    pub fn get_busy_lock(&self) -> &Arc<Mutex<()>> {
        &self.busy_lock
    }

    pub fn get_errors(&self) -> &Arc<RwLock<Vec<String>>> {
        &self.errors
    }

    pub fn get_selected_descriptor(&self) -> &Arc<RwLock<Option<EpisodeKey>>> {
        &self.selected_descriptor
    }

    pub fn get_is_initial_load(&self) -> &Arc<RwLock<bool>> {
        &self.is_initial_load
    }
    
    pub fn get_cache(&self) -> &Arc<RwLock<Option<AppFolderCache>>> {
        &self.cache
    }

    pub async fn get_files(&self) -> AppFileImmutableContext {
        let (file_table, conflict_table) = tokio::join!(
            self.file_table.read(),
            self.conflict_table.read(),
        );
        AppFileImmutableContext {
            file_table,
            conflict_table,
        }
    }

    pub async fn get_mut_files(&self) -> AppFileMutableContext {
        let (file_table, conflict_table, change_queue) = tokio::join!(
            self.file_table.read(),
            self.conflict_table.read(),
            self.change_queue.write(),
        );
        AppFileMutableContext {
            file_table,
            conflict_table,
            change_queue,
        }
    }
    
    pub fn get_files_blocking(&self) -> AppFileImmutableContext {
        let file_table = self.file_table.blocking_read();
        let conflict_table = self.conflict_table.blocking_read();
        AppFileImmutableContext {
            file_table,
            conflict_table,
        }
    }

    pub fn get_mut_files_blocking(&self) -> AppFileMutableContext {
        let file_table = self.file_table.blocking_read();
        let conflict_table = self.conflict_table.blocking_read();
        let change_queue = self.change_queue.blocking_write();
        AppFileMutableContext {
            file_table,
            conflict_table,
            change_queue,
        }
    }
    
    pub fn get_files_try_blocking(&self) -> Option<AppFileImmutableContext> {
        let file_table = self.file_table.try_read().ok()?;
        let conflict_table = self.conflict_table.try_read().ok()?;
        Some(AppFileImmutableContext {
            file_table,
            conflict_table,
        })
    }

    pub fn get_mut_files_try_blocking(&self) -> Option<AppFileMutableContext> {
        let file_table = self.file_table.try_read().ok()?;
        let conflict_table = self.conflict_table.try_read().ok()?;
        let change_queue = self.change_queue.try_write().ok()?;
        Some(AppFileMutableContext {
            file_table,
            conflict_table,
            change_queue,
        })
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

pub trait AppFileContextGetter {
    fn get_src(&self, index: usize) -> &str;
    fn get_src_descriptor(&self, index: usize) -> &Option<EpisodeKey>;
    fn get_action(&self, index: usize) -> Action;
    fn get_dest(&self, index: usize) -> &str;
    fn get_is_enabled(&self, index: usize) -> bool;
    fn get_is_conflict(&self, index: usize) -> bool;
}

pub trait AppFileContextSetter {
    fn set_action(&mut self, new_action: Action, index: usize);
    fn set_is_enabled(&mut self, new_is_enabled: bool, index: usize);
    fn set_dest(&mut self, new_dest: String, index: usize); 
}

pub struct AppFileContextFormatter<'a, T> 
where T: AppFileContextGetter 
{
    index: usize,
    context: &'a T,
}

impl<'a, T> fmt::Debug for AppFileContextFormatter<'a, T> 
where T: AppFileContextGetter 
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppFileContext")
            .field("src", &self.context.get_src(self.index))
            .field("src_descriptor", &self.context.get_src_descriptor(self.index))
            .field("action", &self.context.get_action(self.index))
            .field("dest", &self.context.get_dest(self.index))
            .field("is_enabled", &self.context.get_is_enabled(self.index))
            .field("is_conflict", &self.context.get_is_conflict(self.index))
            .finish()
    }
}

macro_rules! generate_app_file_context_getters {
    ($name: ident) => {
        impl AppFileContextGetter for $name<'_> {
            fn get_src(&self, index: usize) -> &str {
                self.file_table[index].src.as_str() 
            }

            fn get_src_descriptor(&self, index: usize) -> &Option<EpisodeKey> {
                &self.file_table[index].src_descriptor
            }

            fn get_action(&self, index: usize) -> Action {
                self.file_table[index].action
            }

            fn get_dest(&self, index: usize) -> &str {
                self.file_table[index].dest.as_str()
            }

            fn get_is_enabled(&self, index: usize) -> bool {
                self.file_table[index].is_enabled
            }

            fn get_is_conflict(&self, index: usize) -> bool {
                let file = &self.file_table[index];
                if !file.is_enabled || file.action != Action::Rename {
                    return false;
                }
                self.conflict_table.check_if_write_conflicts(file.dest.as_str())
            }
        }

        impl $name<'_> {
            pub fn get_total_items(&self) -> usize {
                self.file_table.len()
            }

            pub fn get_formatter(&self, index: usize) -> AppFileContextFormatter<$name<'_>> {
                AppFileContextFormatter {
                    index,
                    context: self,
                }
            }
        }
    }
}

macro_rules! generate_app_file_context_setters {
    ($name: ident) => {
        impl AppFileContextSetter for $name<'_> {
            fn set_action(&mut self, new_action: Action, index: usize) {
                self.change_queue.push(AppFileChange::SetAction(index, new_action));
                let file = &self.file_table[index];
                // Automatically set destination to src is not set
                if file.action != Action::Rename && new_action == Action::Rename && file.dest.len() == 0 {
                    self.change_queue.push(AppFileChange::SetDest(index, file.src.to_owned())); 
                }
            }

            fn set_is_enabled(&mut self, new_is_enabled: bool, index: usize) {
                let change = AppFileChange::SetIsEnabled(index, new_is_enabled);
                self.change_queue.push(change);
            }

            fn set_dest(&mut self, new_dest: String, index: usize) {
                let change = AppFileChange::SetDest(index, new_dest);
                self.change_queue.push(change);
            }
        }
    }
}

generate_app_file_context_getters!(AppFileMutableContext);
generate_app_file_context_getters!(AppFileImmutableContext);
generate_app_file_context_setters!(AppFileMutableContext);
