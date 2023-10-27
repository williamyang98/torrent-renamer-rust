use std::collections::HashMap;

use serde;
use serde_json;

#[serde_with::skip_serializing_none]
#[derive(serde::Serialize, serde::Deserialize)]
struct BookmarkInternal {
    id: String,
    is_read: Option<bool>,
    is_unread: Option<bool>,
    is_favourite: Option<bool>,
}

pub struct Bookmark {
    pub is_read: bool,
    pub is_unread: bool,
    pub is_favourite: bool,
}

impl Bookmark {
    fn is_any_selected(&self) -> bool {
        self.is_read || self.is_unread || self.is_favourite
    }
}

pub struct BookmarkTable {
    bookmarks: HashMap<String, Bookmark>,
}

impl BookmarkTable {
    pub fn new() -> Self {
        Self {
            bookmarks: HashMap::new(),
        }
    }

    pub fn get_mut_with_insert(&mut self, id: &str) -> &mut Bookmark {
        self.bookmarks.entry(id.to_owned()).or_insert(Bookmark {
            is_favourite: false,
            is_unread: false,
            is_read: false,
        })
    }

    pub fn clear(&mut self) {
        self.bookmarks.clear();
    }
}

impl Default for BookmarkTable {
    fn default() -> Self {
        Self::new()
    }
}

pub fn deserialize_bookmarks(data: &str) -> Result<BookmarkTable, serde_json::Error> {
    let bookmarks: Vec<BookmarkInternal> = serde_json::from_str(data)?;
    
    let mut table = BookmarkTable::new();
    for bookmark in bookmarks {
        table.bookmarks.insert(bookmark.id, Bookmark {
            is_read: bookmark.is_read.unwrap_or(false),
            is_unread: bookmark.is_unread.unwrap_or(false),
            is_favourite: bookmark.is_favourite.unwrap_or(false),
        });
    }
    Ok(table)
}

pub fn serialize_bookmarks(table: &BookmarkTable) -> Result<String, serde_json::Error> {
    let mut bookmarks: Vec<BookmarkInternal> = Vec::new();

    for (id, bookmark) in &table.bookmarks {
        if !bookmark.is_any_selected() {
            continue;
        }

        bookmarks.push(BookmarkInternal {
            id: id.clone(),
            is_favourite: if bookmark.is_favourite { Some(true) } else { None },
            is_unread: if bookmark.is_unread { Some(true) } else { None },
            is_read: if bookmark.is_read { Some(true) } else { None },
        })
    }

    serde_json::to_string_pretty(&bookmarks)
}
