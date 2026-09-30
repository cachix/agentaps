use super::*;
use std::ops::{Deref, DerefMut};

/// The UI owns form entities; the controller owns protocol and session state.
pub(super) struct AgentView {
    pub(super) controller: SessionController,
    pub(super) elicitations: Vec<Elicitation>,
}

impl AgentView {
    pub(super) fn new(config: AgentConfig) -> Self {
        Self {
            controller: SessionController::new(config),
            elicitations: Vec::new(),
        }
    }

    pub(super) fn snapshot(&self) -> AgentConfig {
        self.controller.snapshot()
    }
}

impl Deref for AgentView {
    type Target = SessionController;
    fn deref(&self) -> &Self::Target {
        &self.controller
    }
}

impl DerefMut for AgentView {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.controller
    }
}

impl Status {
    pub(crate) fn color(self) -> u32 {
        match self {
            Self::Connecting => STATUS_CONNECTING,
            Self::Idle => STATUS_IDLE,
            Self::Working => STATUS_WORKING,
            Self::Done => STATUS_DONE,
            Self::Error => STATUS_ERROR,
        }
    }
}
