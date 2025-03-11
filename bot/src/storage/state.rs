use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ChatState {
    #[cfg_attr(not(debug_assertions), default)]
    Paused,
    #[cfg_attr(debug_assertions, default)]
    Active,
    PatiallyActive,
}

impl ChatState {
    pub fn troggle(self) -> Self {
        match self {
            ChatState::Paused => ChatState::Active,
            ChatState::Active => ChatState::PatiallyActive,
            ChatState::PatiallyActive => ChatState::Paused,
        }
    }
}

impl<S: AsRef<str>> From<S> for ChatState {
    fn from(s: S) -> Self {
        match s.as_ref() {
            "Paused" => ChatState::Paused,
            "Active" => ChatState::Active,
            "PartiallyActive" => ChatState::PatiallyActive,
            _ => panic!("Unknown chat state: {}", s.as_ref()),
        }
    }
}

impl fmt::Display for ChatState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ChatState::Paused => write!(f, "Paused"),
            ChatState::Active => write!(f, "Active"),
            ChatState::PatiallyActive => write!(f, "PartiallyActive"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TransportState {
    #[default]
    Pending,
    Downloading,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

impl<S: AsRef<str>> From<S> for TransportState {
    fn from(s: S) -> Self {
        match s.as_ref() {
            "Pending" => TransportState::Pending,
            "Downloading" => TransportState::Downloading,
            "Paused" => TransportState::Paused,
            "Completed" => TransportState::Completed,
            "Cancelled" => TransportState::Cancelled,
            "Failed" => TransportState::Failed,
            _ => panic!("Unknown download state: {}", s.as_ref()),
        }
    }
}

impl fmt::Display for TransportState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TransportState::Pending => write!(f, "Pending"),
            TransportState::Downloading => write!(f, "Downloading"),
            TransportState::Paused => write!(f, "Paused"),
            TransportState::Completed => write!(f, "Completed"),
            TransportState::Cancelled => write!(f, "Cancelled"),
            TransportState::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FileState {
    #[default]
    Normal,
    Fav,
    Trash,
}

impl<S: AsRef<str>> From<S> for FileState {
    fn from(s: S) -> Self {
        match s.as_ref() {
            "Normal" => FileState::Normal,
            "Fav" => FileState::Fav,
            "Trash" => FileState::Trash,
            _ => panic!("Unknown file state: {}", s.as_ref()),
        }
    }
}

impl fmt::Display for FileState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileState::Fav => write!(f, "Fav"),
            FileState::Trash => write!(f, "Trash"),
            FileState::Normal => write!(f, "Normal"),
        }
    }
}
