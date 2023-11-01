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
    pub(crate) action_count: enum_map::EnumMap<Action, usize>,
}

pub struct AppFileImmutableContext<'a> {
    pub(crate) file_list: RwLockReadGuard<'a, Vec<AppFile>>,
    pub(crate) file_tracker: RwLockReadGuard<'a, FileTracker>,
}

pub struct AppFileMutableContext<'a> {
    pub(crate) file_list: RwLockReadGuard<'a, Vec<AppFile>>,
    pub(crate) file_tracker: RwLockReadGuard<'a, FileTracker>,
    pub(crate) change_queue: RwLockWriteGuard<'a, Vec<FileChange>>,
}

// We queue all our changes to our files so we can iterate over them while submitting changes
// We iterate over an immutable reference to the files while submitting to a mutable queue
// Then we take a mutable reference to the file and queue and perform the changes
pub(crate) enum FileChange {
    SetAction(usize, Action),
    IsEnabled(usize, bool),
    Destination(usize, String),
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

macro_rules! generate_app_file_context_getters {
    ($name: ident) => {
        impl $name<'_> {
            pub fn get_src(&self, index: usize) -> &str {
                self.file_list[index].src.as_str() 
            }

            pub fn get_src_descriptor(&self, index: usize) -> &Option<EpisodeKey> {
                &self.file_list[index].src_descriptor
            }

            pub fn get_action(&self, index: usize) -> Action {
                self.file_list[index].action
            }

            pub fn get_dest(&self, index: usize) -> &str {
                self.file_list[index].dest.as_str()
            }

            pub fn get_is_enabled(&self, index: usize) -> bool {
                self.file_list[index].is_enabled
            }

            pub fn get_is_conflict(&self, index: usize) -> bool {
                let file = &self.file_list[index];
                if !file.is_enabled || file.action != Action::Rename {
                    return false;
                }
                self.file_tracker.check_if_write_conflicts(file.dest.as_str())
            }

            pub fn get_total_items(&self) -> usize {
                self.file_list.len()
            }

            pub fn is_empty(&self) -> bool {
                self.file_list.len() == 0
            }
        }
    }
}

generate_app_file_context_getters!(AppFileMutableContext);
generate_app_file_context_getters!(AppFileImmutableContext);

impl AppFileMutableContext<'_> {
    pub fn set_action(&mut self, new_action: Action, index: usize) {
        self.change_queue.push(FileChange::SetAction(index, new_action));
        let file = &self.file_list[index];
        // Automatically set destination to src is not set
        if file.action != Action::Rename && new_action == Action::Rename && file.dest.is_empty() {
            self.change_queue.push(FileChange::Destination(index, file.src.to_owned())); 
        }
        // Automatically disable enabled if we are deleting it
        if new_action == Action::Delete {
            self.change_queue.push(FileChange::IsEnabled(index, false));
        }
    }

    pub fn set_is_enabled(&mut self, new_is_enabled: bool, index: usize) {
        let change = FileChange::IsEnabled(index, new_is_enabled);
        self.change_queue.push(change);
    }

    pub fn set_dest(&mut self, new_dest: String, index: usize) {
        let change = FileChange::Destination(index, new_dest);
        self.change_queue.push(change);
    }
}

