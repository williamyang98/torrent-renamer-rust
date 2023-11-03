use std::collections::{HashMap,HashSet};
use tokio::sync::{RwLockReadGuard, RwLockWriteGuard};
use crate::file_intent::Action;
use crate::tvdb_cache::EpisodeKey;

pub(crate) struct AppFile {
    pub(crate) src: String,
    pub(crate) src_descriptor: Option<EpisodeKey>,
    pub(crate) action: Action,
    pub(crate) dest: String,
    pub(crate) is_enabled: bool,
}

pub struct FileTracker {
    pending_writes: HashMap<String, HashSet<usize>>,
    existing_sources: HashMap<String, usize>,
    action_count: enum_map::EnumMap<Action, usize>,
}

// We queue all our changes to our files so we can iterate over them while submitting changes
// We iterate over an immutable reference to the files while submitting to a mutable queue
// Then we take a mutable reference to the file and queue and perform the changes
pub(crate) enum FileChange {
    SetAction(usize, Action),
    IsEnabled(usize, bool),
    Destination(usize, String),
}

pub struct ImmutableAppFileList<'a> {
    file_list: RwLockReadGuard<'a, Vec<AppFile>>,
    file_tracker: RwLockReadGuard<'a, FileTracker>,
}

pub struct MutableAppFileList<'a> {
    file_list: RwLockReadGuard<'a, Vec<AppFile>>,
    file_tracker: RwLockReadGuard<'a, FileTracker>,
    change_queue: RwLockWriteGuard<'a, Vec<FileChange>>,
}

pub struct MutableAppFile<'a> {
    index: usize,
    file: &'a AppFile,
    change_queue: &'a mut Vec<FileChange>,
    file_tracker: &'a FileTracker,
}

pub struct MutableAppFileIterator<'a> {
    index: usize,
    file_list: &'a [AppFile],
    change_queue: &'a mut Vec<FileChange>,
    file_tracker: &'a FileTracker,
}

pub struct ImmutableAppFile<'a> {
    file: &'a AppFile,
    file_tracker: &'a FileTracker,
}

pub struct ImmutableAppFileIterator<'a> {
    index: usize,
    file_list: &'a [AppFile],
    file_tracker: &'a FileTracker,
}

impl AppFile {
    pub(crate) fn new(src: String, src_descriptor: Option<EpisodeKey>, action: Action, dest: String) -> Self {
        Self {
            src,
            src_descriptor,
            action,
            dest,
            is_enabled: false,
        }
    }
}

impl FileTracker {
    pub(crate) fn new() -> Self {
        Self {
            pending_writes: HashMap::new(),
            existing_sources: HashMap::new(),
            action_count: enum_map::enum_map!{ _ => 0 },
        }
    }

    pub(crate) fn clear(&mut self) {
        self.pending_writes.clear();
        self.existing_sources.clear();
        self.action_count.clear();
    }

    pub(crate) fn insert_existing_source(&mut self, src: &str, index: usize) {
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

    pub fn get_action_count_mut(&mut self) -> &mut enum_map::EnumMap<Action, usize> {
        &mut self.action_count
    }
}

pub(crate) fn flush_file_changes_acquired(
    mut file_list: RwLockWriteGuard<'_, Vec<AppFile>>,  
    mut file_tracker: RwLockWriteGuard<'_, FileTracker>,
    mut change_queue: RwLockWriteGuard<'_, Vec<FileChange>>,
) -> usize {
    let mut total_changes: usize = 0;
    for file_change in change_queue.iter() {
        match file_change {
            FileChange::SetAction(index, new_action) => {
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
            FileChange::IsEnabled(index, new_is_enabled) => {
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
            FileChange::Destination(index, new_dest) => {
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

impl<'a> MutableAppFileList<'a> {
    pub(crate) fn new(
        file_list: RwLockReadGuard<'a, Vec<AppFile>>,
        file_tracker: RwLockReadGuard<'a, FileTracker>,
        change_queue: RwLockWriteGuard<'a, Vec<FileChange>>,
    ) -> Self {
        Self { file_list, file_tracker, change_queue }
    }

    pub fn get(&mut self, index: usize) -> Option<MutableAppFile<'_>> {
        let file = self.file_list.get(index)?;
        Some(MutableAppFile { 
            file, 
            index, 
            change_queue: &mut self.change_queue,
            file_tracker: &self.file_tracker,
        })
    }

    pub fn to_iter(&mut self) -> MutableAppFileIterator<'_> {
        MutableAppFileIterator {
            index: 0,
            change_queue: &mut self.change_queue,
            file_tracker: &self.file_tracker,
            file_list: self.file_list.as_slice(),
        }
    }

    pub fn len(&self) -> usize {
        self.file_list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.file_list.len() == 0
    }
}

impl<'a> ImmutableAppFileList<'a> {
    pub(crate) fn new(
        file_list: RwLockReadGuard<'a, Vec<AppFile>>,
        file_tracker: RwLockReadGuard<'a, FileTracker>,
    ) -> Self {
        Self { file_list, file_tracker }
    }

    pub fn get(&self, index: usize) -> Option<ImmutableAppFile<'_>> {
        let file = self.file_list.get(index)?;
        Some(ImmutableAppFile { 
            file, 
            file_tracker: &self.file_tracker,
        })
    }

    pub fn to_iter(&self) -> ImmutableAppFileIterator<'_> {
        ImmutableAppFileIterator {
            index: 0,
            file_tracker: &self.file_tracker,
            file_list: self.file_list.as_slice(),
        }
    }

    pub fn len(&self) -> usize {
        self.file_list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.file_list.len() == 0
    }
}

// Streaming iterator which allows only one mutable reference to a file at a time
impl MutableAppFileIterator<'_> {
    pub fn next_mut(&mut self) -> Option<MutableAppFile<'_>> {
        let file = self.file_list.get(self.index)?;
        let index = self.index;
        self.index += 1;
        Some(MutableAppFile {
            file,
            index,
            change_queue: self.change_queue,
            file_tracker: self.file_tracker,
        })
    }
}

impl<'a> std::iter::Iterator for ImmutableAppFileIterator<'a> {
    type Item = ImmutableAppFile<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let file = self.file_list.get(self.index)?;
        self.index += 1;
        Some(ImmutableAppFile {
            file,
            file_tracker: self.file_tracker,
        })
    }
}

macro_rules! generate_app_file_getters {
    ($name: ident) => {
        impl $name<'_> {
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
                let file = &self.file;
                if !file.is_enabled || file.action != Action::Rename {
                    return false;
                }
                self.file_tracker.check_if_write_conflicts(file.dest.as_str())
            }
        }
    }
}

generate_app_file_getters!(ImmutableAppFile);
generate_app_file_getters!(MutableAppFile);

impl MutableAppFile<'_> {
    pub fn set_action(&mut self, new_action: Action) {
        self.change_queue.push(FileChange::SetAction(self.index, new_action));
        // Automatically set destination to src is not set
        if self.file.action != Action::Rename && new_action == Action::Rename && self.file.dest.is_empty() {
            self.change_queue.push(FileChange::Destination(self.index, self.file.src.to_owned())); 
        }
        // Automatically disable enabled if we are deleting it
        if new_action == Action::Delete {
            self.change_queue.push(FileChange::IsEnabled(self.index, false));
        }
    }

    pub fn set_is_enabled(&mut self, new_is_enabled: bool) {
        let change = FileChange::IsEnabled(self.index, new_is_enabled);
        self.change_queue.push(change);
    }

    pub fn set_dest(&mut self, new_dest: String) {
        let change = FileChange::Destination(self.index, new_dest);
        self.change_queue.push(change);
    }
}
