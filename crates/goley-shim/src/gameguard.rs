

use std::sync::{Arc, OnceLock};

use thiserror::Error;
use windows::Win32::{Foundation::HANDLE, System::Threading::SetEvent};

use crate::config::{ShimConfig, ShimMode};

static CONTROLLER: OnceLock<Arc<GameGuardController>> = OnceLock::new();

#[derive(Debug)]
pub struct GameGuardController {
    event_name: Option<String>,
    enabled: bool,
}

impl GameGuardController {

pub fn from_config(config: &ShimConfig) -> Self {
        Self {
            event_name: config.gameguard_ready_event.clone(),
            enabled: config.mode == ShimMode::Run,
        }
    }

pub fn selected_event(&self) -> Option<&str> {
        self.enabled.then_some(self.event_name.as_deref()).flatten()
    }

pub fn signal_if_selected(
        &self,
        handle: HANDLE,
        observed_name: &str,
    ) -> Result<bool, GameGuardError> {
        let Some(selected) = self.selected_event() else {
            return Ok(false);
        };
        if !selected.eq_ignore_ascii_case(observed_name) {
            return Ok(false);
        }

unsafe { SetEvent(handle) }.map_err(GameGuardError::Signal)?;
        Ok(true)
    }
}

pub fn initialize(config: &ShimConfig) -> Result<(), GameGuardError> {
    CONTROLLER
        .set(Arc::new(GameGuardController::from_config(config)))
        .map_err(|_| GameGuardError::AlreadyInitialized)
}

pub fn controller() -> Option<&'static Arc<GameGuardController>> {
    CONTROLLER.get()
}

#[derive(Debug, Error)]
pub enum GameGuardError {
    
    #[error("GameGuard controller was already initialized")]
    AlreadyInitialized,
    
    #[error("could not signal selected GameGuard event: {0}")]
    Signal(windows::core::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_mode_never_selects_event() {
        let controller = GameGuardController::from_config(&ShimConfig {
            mode: ShimMode::CaptureWaits,
            gameguard_ready_event: Some("measured-name".to_owned()),
            ..ShimConfig::default()
        });
        assert_eq!(controller.selected_event(), None);
    }
}
