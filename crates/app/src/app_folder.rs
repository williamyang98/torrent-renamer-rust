use async_recursion;
use futures;
use std::collections::{HashMap,HashSet};
use serde_json;
use std::sync::Arc;
use std::fmt;
use tvdb::models::{Episode, Series};
use tvdb::api::LoginSession;
use walkdir;
use std::path;
use tokio;
use tokio::sync::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use enum_map;
use crate::app_folder_cache::{EpisodeKey, AppFolderCache};
use crate::file_intent::{FilterRules, Action, get_file_intent};
use crate::bookmarks::{BookmarkTable, deserialize_bookmarks, serialize_bookmarks};

const PATH_STR_BOOKMARKS: &str = "bookmarks.json";
const PATH_STR_EPISODES_DATA: &str = "episodes.json";
const PATH_STR_SERIES_DATA: &str = "series.json";

#[derive(Debug, Eq, PartialEq, Copy, Clone, enum_map::Enum)]
pub enum FolderStatus {
    Unknown,
    Empty,
    Pending,
    Done,
}

impl FolderStatus {
    pub fn iterator() -> std::slice::Iter<'static, Self> {
        static STATUS: [FolderStatus;4] = [
            FolderStatus::Unknown,
            FolderStatus::Empty,
            FolderStatus::Pending,
            FolderStatus::Done,
        ];
        STATUS.iter()
    }   

    pub fn to_str(&self) -> &'static str {
        match self {
            FolderStatus::Unknown => "Unknown",
            FolderStatus::Empty => "Empty",
            FolderStatus::Pending => "Pending",
            FolderStatus::Done => "Done",
        }
    }
}

pub struct FileTracker {
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

pub struct AppFileImmutableContext<'a> {
    file_list: RwLockReadGuard<'a, Vec<AppFile>>,
    file_tracker: RwLockReadGuard<'a, FileTracker>,
}

// We queue all our changes to our files so we can iterate over them while submitting changes
// We iterate over an immutable reference to the files while submitting to a mutable queue
// Then we take a mutable reference to the file and queue and perform the changes
enum AppFileChange {
    SetAction(usize, Action),
    IsEnabled(usize, bool),
    Destination(usize, String),
}

pub struct AppFileMutableContext<'a> {
    file_list: RwLockReadGuard<'a, Vec<AppFile>>,
    file_tracker: RwLockReadGuard<'a, FileTracker>,
    change_queue: RwLockWriteGuard<'a, Vec<AppFileChange>>,
}

pub struct AppFolder {
    folder_path: String,
    folder_name: String,
    bookmarks_path: String,
    series_path: String,
    episodes_path: String,

    filter_rules: Arc<FilterRules>,
    cache: Arc<RwLock<Option<AppFolderCache>>>,

    file_list: Arc<RwLock<Vec<AppFile>>>,
    file_tracker: Arc<RwLock<FileTracker>>,
    change_queue: Arc<RwLock<Vec<AppFileChange>>>,

    bookmarks: Arc<RwLock<BookmarkTable>>,

    errors: Arc<RwLock<Vec<String>>>,
    busy_lock: Arc<Mutex<()>>,
    selected_descriptor: Arc<RwLock<Option<EpisodeKey>>>,
    is_initial_load: Arc<Mutex<bool>>,
    is_file_count_init: Arc<Mutex<bool>>,
}

impl FileTracker {
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
        let mut total_files = 0;
        if self.existing_sources.get(dest).is_some() {
            total_files += 1;
        }
        // NOTE: Exit early to avoid extra table lookup
        if total_files > 1 {
            return true;
        }
        if let Some(entries) = self.pending_writes.get(dest) {
            total_files += entries.len();
        } 

        total_files > 1
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

        let series_path = path::Path::new(folder_path).join(PATH_STR_SERIES_DATA).to_string_lossy().to_string();
        let episodes_path = path::Path::new(folder_path).join(PATH_STR_EPISODES_DATA).to_string_lossy().to_string();
        let bookmarks_path = path::Path::new(folder_path).join(PATH_STR_BOOKMARKS).to_string_lossy().to_string();

        Self {
            folder_path: folder_path.to_string(),
            folder_name,
            series_path,
            episodes_path,
            bookmarks_path,

            filter_rules,
            cache: Arc::new(RwLock::new(None)),

            file_list: Arc::new(RwLock::new(Vec::new())),
            file_tracker: Arc::new(RwLock::new(FileTracker::new())),
            change_queue: Arc::new(RwLock::new(Vec::new())),

            bookmarks: Arc::new(RwLock::new(BookmarkTable::new())),

            errors: Arc::new(RwLock::new(Vec::new())),
            busy_lock: Arc::new(Mutex::new(())),
            selected_descriptor: Arc::new(RwLock::new(None)),
            is_initial_load: Arc::new(Mutex::new(false)),
            is_file_count_init: Arc::new(Mutex::new(false)),
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
    for entry in walkdir::WalkDir::new(path).into_iter().flatten() {
        if entry.file_type().is_file() {
            return false;
        }
    }
    true
}

impl AppFolder {
    pub async fn perform_initial_load(&self) -> Option<()> {
        {
            let mut is_loaded = self.is_initial_load.lock().await;
            if *is_loaded {
                return None;
            }
            *is_loaded = true;
        }
        let (res_0, res_1) = tokio::join!(
            async {
                self.load_cache_from_file().await?;
                self.update_file_intents().await
            },
            self.load_bookmarks_from_file(),
        );
        res_0.or(res_1)
    }

    pub fn get_folder_status(&self) -> FolderStatus {
        if !*self.is_file_count_init.blocking_lock() {
            return FolderStatus::Unknown; 
        }

        let action_count = &self.file_tracker.as_ref().blocking_read().action_count;
        let file_count = Action::iterator()
            .map(|action| action_count[*action])
            .reduce(|acc, v| acc + v);
        let file_count = match file_count {
            Some(count) => count,
            None => return FolderStatus::Unknown,
        };
        
        if file_count == 0 {
            return FolderStatus::Empty;
        }

        let pending_count = action_count[Action::Delete] + action_count[Action::Rename];
        if pending_count > 0 {
            return FolderStatus::Pending;
        }

        FolderStatus::Done
    }

    pub async fn load_bookmarks_from_file(&self) -> Option<()> {
        let bookmarks_data = tokio::fs::read_to_string(self.bookmarks_path.as_str()).await;
        if let Err(err) = bookmarks_data.as_ref() {
            let message = format!("IO while reading bookmarks: {}", err);
            self.errors.write().await.push(message);
        }

        let bookmarks_data = bookmarks_data.as_ref().ok()?;

        let bookmarks = match deserialize_bookmarks(bookmarks_data.as_str()) {
            Ok(bookmarks) => bookmarks,
            Err(err) => {
                let message = format!("JSON decoding error reading bookmarks from file: {}", err); 
                self.errors.write().await.push(message);
                return None;
            },
        };

        *self.bookmarks.write().await = bookmarks;
        Some(())
    }

    pub async fn save_bookmarks_to_file(&self) -> Option<()> {
        let bookmarks_data = {
            let bookmarks = self.bookmarks.read().await;
            serialize_bookmarks(&bookmarks)
        };

        if let Err(err) = bookmarks_data.as_ref() {
            let message = format!("JSON encoding error writing bookmarks to file: {}", err);
            self.errors.write().await.push(message);
            return None;
        }

        let bookmarks_data = bookmarks_data.as_ref().ok()?;
        let res = tokio::fs::write(self.bookmarks_path.as_str(), bookmarks_data).await;

        if let Err(err) = res {
            let message = format!("IO error while writing bookmarks to file: {}", err);
            self.errors.write().await.push(message);
            return None;
        };
        Some(())
    }

    pub fn get_bookmarks(&self) -> &Arc<RwLock<BookmarkTable>> {
        &self.bookmarks
    }

    pub async fn update_file_intents(&self) -> Option<()> {
        let _busy_lock = self.busy_lock.lock().await;

        let mut new_file_list = Vec::<AppFile>::new();
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
            let res = recursive_search_file_intents(
                self.folder_path.as_str(), self.folder_path.as_str(), cache, 
                &mut new_file_list, &self.filter_rules,
            ).await;
            if let Err(err) = res {
                let message = format!("IO error while reading files for intent update: {}", err);
                self.errors.write().await.push(message);
                return None;
            }
        }

        new_file_list.sort_by(|a,b| {
            let a_name = a.src.as_str();
            let b_name = b.src.as_str();
            a_name.partial_cmp(b_name).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        {
            let mut file_list = self.file_list.write().await;
            let mut file_tracker = self.file_tracker.write().await;

            *file_list = new_file_list;
            file_tracker.clear();

            // seed conflict table
            for (index, file) in file_list.iter().enumerate() {
                file_tracker.insert_existing_source(file.src.as_str(), index);
                file_tracker.action_count[file.action] += 1usize;
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
        *self.is_file_count_init.lock().await = true;
        Some(())
    }

    pub async fn is_cache_loaded(&self) -> bool {
        self.cache.read().await.is_some()
    }

    pub async fn load_cache_from_file(&self) -> Option<()> {
        let _busy_lock = self.busy_lock.lock().await;

        let (series_data, episodes_data) = tokio::join!(
            tokio::fs::read_to_string(self.series_path.as_str()),
            tokio::fs::read_to_string(self.episodes_path.as_str())
        );
        
        if let Err(err) = series_data.as_ref() {
            let message = format!("IO error while reading series cache: {}", err);
            self.errors.write().await.push(message);
        }

        if let Err(err) = episodes_data.as_ref() {
            let message = format!("IO error while reading episodes cache: {}", err);
            self.errors.write().await.push(message);
        }

        let series_data = series_data.as_ref().ok()?;
        let episodes_data = episodes_data.as_ref().ok()?;

        let series: Series = match serde_json::from_str(series_data.as_str()) {
            Ok(series) => series,
            Err(err) => {
                let message = format!("JSON decoding error reading series from file: {}", err);
                self.errors.write().await.push(message);
                return None;
            },
        };

        let episodes: Vec<Episode> = match serde_json::from_str(episodes_data.as_str()) {
            Ok(episodes) => episodes,
            Err(err) => {
                let message = format!("JSON decoding error reading episodes from file: {}", err);
                self.errors.write().await.push(message);
                return None;
            },
        };

        let mut cache = self.cache.write().await;
        *cache = Some(AppFolderCache::new(series, episodes));
        Some(())
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
                let message = format!("Api error while fetching series: {}", err);
                self.errors.write().await.push(message);
                return None;
            },
        };

        let episodes = match episodes_res {
            Ok(episodes) => episodes,
            Err(err) => {
                let message = format!("Api error while fetching episodes: {}", err);
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
                    let message = "Couldn't refresh cache since it requires an existing loaded cache".to_string();
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
                    let message = "Couldn't save cache to file since it is unloaded".to_string();
                    self.errors.write().await.push(message);
                    return None;
                },
            };
            let series_str = match serde_json::to_string_pretty(&cache.series) {
                Ok(data) => data,
                Err(err) => {
                    let message = format!("JSON encode error when saving series cache: {}", err);
                    self.errors.write().await.push(message);
                    return None;
                },
            };
            let episodes_str = match serde_json::to_string_pretty(&cache.episodes) {
                Ok(data) => data,
                Err(err) => {
                    let message = format!("JSON encode error when saving episodes cache: {}", err);
                    self.errors.write().await.push(message);
                    return None;
                },
            };
            (series_str, episodes_str)
        };

        let (res_0, res_1) = tokio::join!(
            tokio::fs::write(self.series_path.as_str(), series_str),
            tokio::fs::write(self.episodes_path.as_str(), episodes_str),
        );

        if let Err(err) = res_0.as_ref() {
            let message = format!("IO error while saving series cache: {}", err);
            self.errors.write().await.push(message);
        }

        if let Err(err) = res_1.as_ref() {
            let message = format!("IO error while saving episodes cache: {}", err);
            self.errors.write().await.push(message);
        }
        
        if res_0.is_err() || res_1.is_err() {
            return None;
        }
        Some(())
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
        
        let mut errors = self.errors.write().await;
        for res in futures::future::join_all(tasks).await.into_iter() {
            if let Err(err) = res {
                let message = format!("IO error while executing file changes: {}", err);
                errors.push(message);
            };
        }
    }

    pub async fn delete_empty_folders(&self) {
        let mut tasks = Vec::new();

        let walker = walkdir::WalkDir::new(self.folder_path.as_str())
            .max_depth(1)
            .follow_links(false)
            .into_iter()
            .flatten(); 
        for entry in walker {
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

        let mut errors = self.errors.write().await;
        for res in futures::future::join_all(tasks).await.into_iter() {
            if let Err(err) = res {
                let message = format!("IO error while deleting empty folders: {}", err);
                errors.push(message);
            };
        }
    }

    pub fn get_folder_path(&self) -> &str {
        self.folder_path.as_str() 
    }

    pub fn get_folder_name(&self) -> &str {
        self.folder_name.as_str() 
    }

    pub fn get_file_tracker(&self) -> &Arc<RwLock<FileTracker>> {
        &self.file_tracker
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

    pub fn get_cache(&self) -> &Arc<RwLock<Option<AppFolderCache>>> {
        &self.cache
    }

    pub async fn get_files(&self) -> AppFileImmutableContext {
        let file_list = self.file_list.read().await;
        let file_tracker = self.file_tracker.read().await;
        AppFileImmutableContext {
            file_list,
            file_tracker,
        }
    }

    pub async fn get_mut_files(&self) -> AppFileMutableContext {
        let file_list = self.file_list.read().await;
        let file_tracker = self.file_tracker.read().await;
        let change_queue = self.change_queue.write().await;
        AppFileMutableContext { file_list, file_tracker, change_queue }
    }
    
    pub fn get_files_blocking(&self) -> AppFileImmutableContext {
        let file_list = self.file_list.blocking_read();
        let file_tracker = self.file_tracker.blocking_read();
        AppFileImmutableContext { file_list, file_tracker }
    }

    pub fn get_mut_files_blocking(&self) -> AppFileMutableContext {
        let file_list = self.file_list.blocking_read();
        let file_tracker = self.file_tracker.blocking_read();
        let change_queue = self.change_queue.blocking_write();
        AppFileMutableContext { file_list, file_tracker, change_queue }
    }
    
    pub fn get_files_try_blocking(&self) -> Option<AppFileImmutableContext> {
        let file_list = self.file_list.try_read().ok()?;
        let file_tracker = self.file_tracker.try_read().ok()?;
        Some(AppFileImmutableContext { file_list, file_tracker })
    }

    pub fn get_mut_files_try_blocking(&self) -> Option<AppFileMutableContext> {
        let file_list = self.file_list.try_read().ok()?;
        let file_tracker = self.file_tracker.try_read().ok()?;
        let change_queue = self.change_queue.try_write().ok()?;
        Some(AppFileMutableContext { file_list, file_tracker, change_queue })
    }

    pub async fn flush_file_changes(&self) -> usize {
        let file_list = self.file_list.write().await;
        let file_tracker = self.file_tracker.write().await;
        let change_queue = self.change_queue.write().await;
        flush_file_changes_acquired(file_list, file_tracker, change_queue)
    }

    pub fn flush_file_changes_blocking(&self) -> usize {
        let file_list = self.file_list.blocking_write();
        let file_tracker = self.file_tracker.blocking_write();
        let change_queue = self.change_queue.blocking_write();
        flush_file_changes_acquired(file_list, file_tracker, change_queue)
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
                self.file_list[index].src.as_str() 
            }

            fn get_src_descriptor(&self, index: usize) -> &Option<EpisodeKey> {
                &self.file_list[index].src_descriptor
            }

            fn get_action(&self, index: usize) -> Action {
                self.file_list[index].action
            }

            fn get_dest(&self, index: usize) -> &str {
                self.file_list[index].dest.as_str()
            }

            fn get_is_enabled(&self, index: usize) -> bool {
                self.file_list[index].is_enabled
            }

            fn get_is_conflict(&self, index: usize) -> bool {
                let file = &self.file_list[index];
                if !file.is_enabled || file.action != Action::Rename {
                    return false;
                }
                self.file_tracker.check_if_write_conflicts(file.dest.as_str())
            }
        }

        impl $name<'_> {
            pub fn get_total_items(&self) -> usize {
                self.file_list.len()
            }

            pub fn is_empty(&self) -> bool {
                self.file_list.len() == 0
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

generate_app_file_context_getters!(AppFileMutableContext);
generate_app_file_context_getters!(AppFileImmutableContext);

impl AppFileMutableContext<'_> {
    pub fn set_action(&mut self, new_action: Action, index: usize) {
        self.change_queue.push(AppFileChange::SetAction(index, new_action));
        let file = &self.file_list[index];
        // Automatically set destination to src is not set
        if file.action != Action::Rename && new_action == Action::Rename && file.dest.is_empty() {
            self.change_queue.push(AppFileChange::Destination(index, file.src.to_owned())); 
        }
        // Automatically disable enabled if we are deleting it
        if new_action == Action::Delete {
            self.change_queue.push(AppFileChange::IsEnabled(index, false));
        }
    }

    pub fn set_is_enabled(&mut self, new_is_enabled: bool, index: usize) {
        let change = AppFileChange::IsEnabled(index, new_is_enabled);
        self.change_queue.push(change);
    }

    pub fn set_dest(&mut self, new_dest: String, index: usize) {
        let change = AppFileChange::Destination(index, new_dest);
        self.change_queue.push(change);
    }
}

fn flush_file_changes_acquired(
    mut file_list: RwLockWriteGuard<'_, Vec<AppFile>>,  
    mut file_tracker: RwLockWriteGuard<'_, FileTracker>,
    mut change_queue: RwLockWriteGuard<'_, Vec<AppFileChange>>,
) -> usize {
    let mut total_changes: usize = 0;
    for file_change in change_queue.iter() {
        match file_change {
            AppFileChange::SetAction(index, new_action) => {
                let index = *index;
                let new_action = *new_action;
                let file = match file_list.get_mut(index) {
                    Some(file) => file,
                    None => continue,
                };

                let old_action = file.action;
                file.action = new_action;

                if old_action == new_action {
                    continue;
                }

                file_tracker.action_count[old_action] -= 1usize;
                file_tracker.action_count[new_action] += 1usize;

                if !file.is_enabled {
                    continue;
                };

                if old_action != Action::Rename && new_action != Action::Rename {
                    continue;
                }

                if old_action == Action::Rename {
                    file_tracker.remove_pending_write(file.dest.as_str(), index);
                } else {
                    file_tracker.add_pending_write(file.dest.as_str(), index);
                };
                total_changes += 1;
            },
            AppFileChange::IsEnabled(index, new_is_enabled) => {
                let index = *index;
                let new_is_enabled = *new_is_enabled;
                let file = match file_list.get_mut(index) {
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
                    file_tracker.add_pending_write(file.dest.as_str(), index);
                } else {
                    file_tracker.remove_pending_write(file.dest.as_str(), index);
                };
                total_changes += 1;
            },
            AppFileChange::Destination(index, new_dest) => {
                let index = *index;
                let file = match file_list.get_mut(index) {
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

                file_tracker.remove_pending_write(file.dest.as_str(), index);
                file_tracker.add_pending_write(new_dest.as_str(), index);

                file.dest.clear();
                file.dest.push_str(new_dest.as_str());
                total_changes += 1;
            },
        }
    }

    change_queue.clear();
    total_changes
}
