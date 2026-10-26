use crate::location::Location;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub location: Location,
    pub directory: bool,
    pub hidden: bool,
    pub size: u64,
    pub modified: u64,
    pub kind: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub location: Option<Location>,
    pub can_mount: bool,
    pub can_unmount: bool,
    pub space: Option<(u64, u64)>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Name,
    Size,
    Modified,
}

pub struct Browser {
    pub current: Location,
    pub entries: Vec<Entry>,
    pub devices: Vec<Device>,
    pub selected: Option<usize>,
    pub query: String,
    pub page: usize,
    pub show_hidden: bool,
    pub sort: Sort,
    pub back: Vec<Location>,
    pub forward: Vec<Location>,
    pub generation: u64,
    pub busy: bool,
    pub loaded: bool,
    pub notice: String,
}
impl Browser {
    pub fn new(current: Location) -> Self {
        Self {
            current,
            entries: Vec::new(),
            devices: Vec::new(),
            selected: None,
            query: String::new(),
            page: 0,
            show_hidden: false,
            sort: Sort::Name,
            back: Vec::new(),
            forward: Vec::new(),
            generation: 0,
            busy: false,
            loaded: false,
            notice: String::new(),
        }
    }
    pub fn navigate(&mut self, location: Location) {
        if self.current != location {
            self.back.push(self.current.clone());
            self.forward.clear();
            self.current = location;
        }
        self.selected = None;
        self.page = 0;
        self.loaded = false;
        self.query.clear();
    }
    pub fn back(&mut self) -> bool {
        let Some(previous) = self.back.pop() else {
            return false;
        };
        self.forward.push(self.current.clone());
        self.current = previous;
        self.selected = None;
        self.page = 0;
        self.loaded = false;
        self.query.clear();
        true
    }
    pub fn forward(&mut self) -> bool {
        let Some(next) = self.forward.pop() else {
            return false;
        };
        self.back.push(self.current.clone());
        self.current = next;
        self.selected = None;
        self.page = 0;
        self.loaded = false;
        self.query.clear();
        true
    }
    pub fn visible(&self) -> Vec<(usize, &Entry)> {
        let query = self.query.to_lowercase();
        let mut rows: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                (self.show_hidden || !entry.hidden) && entry.name.to_lowercase().contains(&query)
            })
            .collect();
        rows.sort_by(|(_, left), (_, right)| {
            right
                .directory
                .cmp(&left.directory)
                .then_with(|| match self.sort {
                    Sort::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
                    Sort::Size => right.size.cmp(&left.size),
                    Sort::Modified => right.modified.cmp(&left.modified),
                })
                .then_with(|| left.name.cmp(&right.name))
        });
        rows
    }
    pub fn selection(&self) -> Option<&Entry> {
        self.entries.get(self.selected?)
    }
}
