//! Telegram Desktop Keyboard Shortcuts Router & Command Registry
//! Upstream sources: `Telegram/SourceFiles/core/shortcuts.h`, `shortcuts.cpp`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Command {
    Close,
    Lock,
    Minimize,
    Quit,
    ReopenClosedWindow,
    CloseOtherWindows,

    MediaPlay,
    MediaPause,
    MediaPlayPause,
    MediaStop,
    MediaPrevious,
    MediaNext,

    Search,

    ChatPrevious,
    ChatNext,
    ChatFirst,
    ChatLast,
    ChatSelf,
    ChatPinned1,
    ChatPinned2,
    ChatPinned3,
    ChatPinned4,
    ChatPinned5,
    ChatPinned6,
    ChatPinned7,
    ChatPinned8,

    ShowAccount1,
    ShowAccount2,
    ShowAccount3,
    ShowAccount4,
    ShowAccount5,
    ShowAccount6,

    ShowAllChats,
    ShowFolder1,
    ShowFolder2,
    ShowFolder3,
    ShowFolder4,
    ShowFolder5,
    ShowFolder6,
    ShowFolderLast,

    FolderNext,
    FolderPrevious,

    ShowScheduled,
    ShowArchive,
    ShowContacts,

    JustSendMessage,
    SendSilentMessage,
    ScheduleMessage,
    ComposeAiApplyInPlace,
    ShowRichEditor,
    ToggleWebPagePreview,
    StashMessage,

    RecordVoice,
    RecordRound,

    ReadChat,
    ArchiveChat,

    MediaViewerFullscreen,
    ShowChatMenu,
    ShowChatPreview,
    ShowAdminLog,
}

pub type ShortcutHandler = Arc<dyn Fn() -> bool + Send + Sync>;

/// Central keyboard shortcut dispatcher routing key combinations to semantic commands.
#[derive(Default)]
pub struct Shortcuts {
    handlers: HashMap<Command, Vec<ShortcutHandler>>,
    bindings: HashMap<String, Command>,
    media_shortcuts_enabled: bool,
    support_shortcuts_enabled: bool,
    paused: bool,
}

impl Shortcuts {
    pub fn new() -> Self {
        let mut s = Self::default();
        s.init_defaults();
        s
    }

    fn init_defaults(&mut self) {
        self.bind("Ctrl+W", Command::Close);
        self.bind("Ctrl+L", Command::Lock);
        self.bind("Ctrl+M", Command::Minimize);
        self.bind("Ctrl+Q", Command::Quit);
        self.bind("Ctrl+F", Command::Search);
        self.bind("Ctrl+Alt+Up", Command::ChatPrevious);
        self.bind("Ctrl+Alt+Down", Command::ChatNext);
        self.bind("Ctrl+1", Command::ChatPinned1);
        self.bind("Ctrl+2", Command::ChatPinned2);
        self.bind("Ctrl+3", Command::ChatPinned3);
        self.bind("Ctrl+4", Command::ChatPinned4);
        self.bind("Ctrl+5", Command::ChatPinned5);
        self.bind("Ctrl+Shift+Up", Command::FolderPrevious);
        self.bind("Ctrl+Shift+Down", Command::FolderNext);
        self.bind("Return", Command::JustSendMessage);
        self.bind("Ctrl+Return", Command::JustSendMessage);
        self.bind("Shift+Return", Command::SendSilentMessage);
    }

    pub fn bind(&mut self, key_sequence: impl Into<String>, command: Command) {
        self.bindings.insert(key_sequence.into(), command);
    }

    pub fn register(&mut self, command: Command, handler: impl Fn() -> bool + Send + Sync + 'static) {
        self.handlers.entry(command).or_default().push(Arc::new(handler));
    }

    pub fn dispatch(&self, command: Command) -> bool {
        if self.paused {
            return false;
        }
        if let Some(handlers) = self.handlers.get(&command) {
            for handler in handlers {
                if handler() {
                    return true;
                }
            }
        }
        false
    }

    pub fn dispatch_key_sequence(&self, sequence: &str) -> bool {
        if self.paused {
            return false;
        }
        if let Some(&command) = self.bindings.get(sequence) {
            self.dispatch(command)
        } else {
            false
        }
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn unpause(&mut self) {
        self.paused = false;
    }

    pub fn toggle_media_shortcuts(&mut self, enabled: bool) {
        self.media_shortcuts_enabled = enabled;
    }

    pub fn toggle_support_shortcuts(&mut self, enabled: bool) {
        self.support_shortcuts_enabled = enabled;
    }
}
