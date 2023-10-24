pub struct AppBookmarks {
    pub is_favourite: bool,
    pub is_read: bool,
    pub is_unread: bool,
}

impl AppBookmarks {
    fn new() -> Self {
        Self {
            is_favourite: false,
            is_read: false,
            is_unread: false,
        }
    }

    fn is_any_selected(&self) -> bool {
        self.is_favourite ||
        self.is_read ||
        self.is_unread
    }
}
