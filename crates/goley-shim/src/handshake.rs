

use thiserror::Error;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::Threading::{EVENT_MODIFY_STATE, OpenEventW, SetEvent},
    },
    core::PCWSTR,
};

use crate::config::ShimConfig;

pub(crate) struct Handshake {
    loaded: Option<NamedEvent>,
    ready: Option<NamedEvent>,
}

impl Handshake {
    pub(crate) fn open(config: &ShimConfig) -> Result<Self, HandshakeError> {
        Ok(Self {
            loaded: config
                .loaded_event
                .as_deref()
                .map(NamedEvent::open)
                .transpose()?,
            ready: config
                .ready_event
                .as_deref()
                .map(NamedEvent::open)
                .transpose()?,
        })
    }

    pub(crate) fn signal_loaded(&self) -> Result<(), HandshakeError> {
        if let Some(event) = &self.loaded {
            event.signal()?;
        }
        Ok(())
    }

    pub(crate) fn signal_ready(&self) -> Result<(), HandshakeError> {
        if let Some(event) = &self.ready {
            event.signal()?;
        }
        Ok(())
    }
}

struct NamedEvent {
    name: String,
    handle: HANDLE,
}

impl NamedEvent {
    fn open(name: &str) -> Result<Self, HandshakeError> {
        let mut wide: Vec<u16> = name.encode_utf16().collect();
        wide.push(0);
        
        let handle = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(wide.as_ptr())) }
            .map_err(|source| HandshakeError::Open {
                name: name.to_owned(),
                source,
            })?;
        Ok(Self {
            name: name.to_owned(),
            handle,
        })
    }

    fn signal(&self) -> Result<(), HandshakeError> {
        
        unsafe { SetEvent(self.handle) }.map_err(|source| HandshakeError::Signal {
            name: self.name.clone(),
            source,
        })
    }
}

impl Drop for NamedEvent {
    fn drop(&mut self) {
        
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

#[derive(Debug, Error)]
pub(crate) enum HandshakeError {
    #[error("could not open handshake event {name:?}: {source}")]
    Open {
        name: String,
        source: windows::core::Error,
    },
    #[error("could not signal handshake event {name:?}: {source}")]
    Signal {
        name: String,
        source: windows::core::Error,
    },
}
