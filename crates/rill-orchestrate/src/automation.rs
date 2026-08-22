//! Typed event/action foundation for the configuration + automation
//! architecture.
//!
//! This module intentionally stays out of the PTY / VT / render hot path. The
//! runtime emits semantic events and the automation layer may schedule actions
//! through the same action dispatcher used by native RILL features where
//! reasonable. The hot path never invokes Lua directly.

use std::collections::{HashMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventClass {
    V1,
    Future,
    NotAllowed,
}

/// Semantic events emitted by the runtime and observed by automation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RillEventKind {
    WorkspaceOpened,
    WorkspaceClosed,
    TabCreated,
    TabFocused,
    PaneCreated,
    PaneFocused,
    CwdChanged,
    CommandStarted,
    CommandCompleted,
    Bell,
    AgentStarted,
    AgentCompleted,
    AgentAttention,
    AttentionCreated,
    ConfigReloaded,
}

impl RillEventKind {
    /// Classify events by the current architecture. The v1 set is intentionally
    /// conservative and aligned with the codebase's current capabilities.
    pub fn maturity(self) -> EventClass {
        match self {
            Self::WorkspaceOpened
            | Self::WorkspaceClosed
            | Self::TabCreated
            | Self::TabFocused
            | Self::PaneCreated
            | Self::PaneFocused
            | Self::CwdChanged
            | Self::CommandStarted
            | Self::CommandCompleted
            | Self::Bell
            | Self::ConfigReloaded => EventClass::V1,
            Self::AgentStarted
            | Self::AgentCompleted
            | Self::AgentAttention
            | Self::AttentionCreated => EventClass::Future,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RillEvent {
    pub kind: RillEventKind,
    pub event_id: String,
    pub source: String,
    pub created_at_ms: u64,
    pub payload: HashMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Left,
    Right,
    Up,
    Down,
}

impl SplitDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// Typed actions that leave the PTY/VT/render hot path and are applied through a
/// regular action dispatcher.
#[derive(Clone, Debug, PartialEq)]
pub enum RillAction {
    CreateTab {
        tab_id: Option<String>,
        title: Option<String>,
        command: Option<String>,
    },
    CloseTab {
        tab_id: String,
    },
    SplitPane {
        pane_id: String,
        direction: SplitDirection,
        command: Option<String>,
    },
    FocusPane {
        pane_id: String,
    },
    SetTabTitle {
        tab_id: String,
        title: String,
    },
    SetTabBadge {
        tab_id: String,
        badge: String,
    },
    ShowNotification {
        title: String,
        body: String,
    },
    ChangeRuntimeAppearanceOverride {
        theme: String,
        background_opacity: Option<f64>,
    },
    RunCommand {
        command: String,
        cwd: Option<String>,
    },
    OpenWorkspace {
        path: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionValidationError {
    EmptyValue(String),
    MissingIdentifier(String),
    InvalidOpacity,
}

impl RillAction {
    /// Canonical validation. The action layer accepts only typed, bounded inputs
    /// and rejects empty or contradictory values before enqueueing.
    pub fn validate(&self) -> Result<(), ActionValidationError> {
        match self {
            Self::CreateTab {
                tab_id: Some(id),
                title,
                command,
            } => {
                if id.trim().is_empty() {
                    return Err(ActionValidationError::MissingIdentifier("tab_id".into()));
                }
                if let Some(title) = title {
                    if title.trim().is_empty() {
                        return Err(ActionValidationError::EmptyValue("title".into()));
                    }
                }
                if let Some(command) = command {
                    if command.trim().is_empty() {
                        return Err(ActionValidationError::EmptyValue("command".into()));
                    }
                }
                Ok(())
            }
            Self::CreateTab { tab_id: None, .. } => Ok(()),
            Self::CloseTab { tab_id } => {
                if tab_id.trim().is_empty() {
                    return Err(ActionValidationError::MissingIdentifier("tab_id".into()));
                }
                Ok(())
            }
            Self::SplitPane {
                pane_id,
                direction: _,
                command,
            } => {
                if pane_id.trim().is_empty() {
                    return Err(ActionValidationError::MissingIdentifier("pane_id".into()));
                }
                if let Some(command) = command {
                    if command.trim().is_empty() {
                        return Err(ActionValidationError::EmptyValue("command".into()));
                    }
                }
                Ok(())
            }
            Self::FocusPane { pane_id } => {
                if pane_id.trim().is_empty() {
                    return Err(ActionValidationError::MissingIdentifier("pane_id".into()));
                }
                Ok(())
            }
            Self::SetTabTitle { tab_id, title } => {
                if tab_id.trim().is_empty() {
                    return Err(ActionValidationError::MissingIdentifier("tab_id".into()));
                }
                if title.trim().is_empty() {
                    return Err(ActionValidationError::EmptyValue("title".into()));
                }
                Ok(())
            }
            Self::SetTabBadge { tab_id, badge } => {
                if tab_id.trim().is_empty() {
                    return Err(ActionValidationError::MissingIdentifier("tab_id".into()));
                }
                if badge.trim().is_empty() {
                    return Err(ActionValidationError::EmptyValue("badge".into()));
                }
                Ok(())
            }
            Self::ShowNotification { title, body } => {
                if title.trim().is_empty() {
                    return Err(ActionValidationError::EmptyValue("title".into()));
                }
                if body.trim().is_empty() {
                    return Err(ActionValidationError::EmptyValue("body".into()));
                }
                Ok(())
            }
            Self::ChangeRuntimeAppearanceOverride {
                theme,
                background_opacity,
            } => {
                if theme.trim().is_empty() {
                    return Err(ActionValidationError::EmptyValue("theme".into()));
                }
                if let Some(opacity) = background_opacity {
                    if !(*opacity >= 0.0 && *opacity <= 1.0) {
                        return Err(ActionValidationError::InvalidOpacity);
                    }
                }
                Ok(())
            }
            Self::RunCommand { command, cwd } => {
                if command.trim().is_empty() {
                    return Err(ActionValidationError::EmptyValue("command".into()));
                }
                if let Some(cwd) = cwd {
                    if cwd.trim().is_empty() {
                        return Err(ActionValidationError::EmptyValue("cwd".into()));
                    }
                }
                Ok(())
            }
            Self::OpenWorkspace { path } => {
                if path.trim().is_empty() {
                    return Err(ActionValidationError::EmptyValue("path".into()));
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionDispatchError {
    QueueFull { limit: usize },
    Validation(ActionValidationError),
}

/// Queue-based dispatcher used by automation actions. The queue is bounded to
/// prevent a slow or broken Lua extension from stalling the runtime.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionDispatcher {
    queue: VecDeque<RillAction>,
    limit: usize,
}

impl ActionDispatcher {
    pub fn new(limit: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            limit: limit.max(1),
        }
    }

    pub fn enqueue(&mut self, action: RillAction) -> Result<(), ActionDispatchError> {
        action.validate().map_err(ActionDispatchError::Validation)?;
        if self.queue.len() >= self.limit {
            return Err(ActionDispatchError::QueueFull { limit: self.limit });
        }
        self.queue.push_back(action);
        Ok(())
    }

    pub fn dispatch(&mut self, action: RillAction) -> Result<(), ActionDispatchError> {
        self.enqueue(action)
    }

    pub fn drain(&mut self) -> Vec<RillAction> {
        let mut out = Vec::new();
        while let Some(action) = self.queue.pop_front() {
            out.push(action);
        }
        out
    }

    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LuaHostPolicy {
    pub enabled: bool,
    pub safe_mode: bool,
    pub max_actions_per_event: usize,
    pub timeout_ms: u64,
    pub queue_limit: usize,
}

impl Default for LuaHostPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            safe_mode: true,
            max_actions_per_event: 16,
            timeout_ms: 50,
            queue_limit: 256,
        }
    }
}
