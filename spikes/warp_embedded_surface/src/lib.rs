//! Lifecycle contract for embedding a Warp terminal surface in a native host.
//!
//! This standalone spike is intentionally dependency-free.
//! It does not create a platform view yet and is not linked by the Warp app.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceGeometry {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

impl SurfaceGeometry {
    pub fn is_valid(self) -> bool {
        self.width > 0
            && self.height > 0
            && self.scale_factor.is_finite()
            && self.scale_factor > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttachRequest {
    /// Opaque native parent-view handle supplied by the in-process host.
    pub parent_handle: usize,
    pub geometry: SurfaceGeometry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceState {
    Detached,
    Attached { parent_handle: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceError {
    NullParentHandle,
    InvalidGeometry,
    AlreadyAttached,
    NotAttached,
    ParentHandleMismatch,
}

#[derive(Debug, PartialEq)]
pub struct EmbeddedSurfaceLifecycle {
    state: SurfaceState,
    geometry: Option<SurfaceGeometry>,
    focused: bool,
}

impl Default for EmbeddedSurfaceLifecycle {
    fn default() -> Self {
        Self {
            state: SurfaceState::Detached,
            geometry: None,
            focused: false,
        }
    }
}

impl EmbeddedSurfaceLifecycle {
    pub fn state(&self) -> SurfaceState {
        self.state
    }

    pub fn geometry(&self) -> Option<SurfaceGeometry> {
        self.geometry
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn attach(&mut self, request: AttachRequest) -> Result<(), SurfaceError> {
        if request.parent_handle == 0 {
            return Err(SurfaceError::NullParentHandle);
        }
        if !request.geometry.is_valid() {
            return Err(SurfaceError::InvalidGeometry);
        }
        if self.state != SurfaceState::Detached {
            return Err(SurfaceError::AlreadyAttached);
        }

        self.state = SurfaceState::Attached {
            parent_handle: request.parent_handle,
        };
        self.geometry = Some(request.geometry);
        Ok(())
    }

    pub fn resize(&mut self, geometry: SurfaceGeometry) -> Result<(), SurfaceError> {
        if self.state == SurfaceState::Detached {
            return Err(SurfaceError::NotAttached);
        }
        if !geometry.is_valid() {
            return Err(SurfaceError::InvalidGeometry);
        }

        self.geometry = Some(geometry);
        Ok(())
    }

    pub fn set_focused(&mut self, focused: bool) -> Result<(), SurfaceError> {
        if self.state == SurfaceState::Detached {
            return Err(SurfaceError::NotAttached);
        }

        self.focused = focused;
        Ok(())
    }

    pub fn detach(&mut self, parent_handle: usize) -> Result<(), SurfaceError> {
        match self.state {
            SurfaceState::Detached => Err(SurfaceError::NotAttached),
            SurfaceState::Attached {
                parent_handle: attached_parent,
            } if attached_parent != parent_handle => Err(SurfaceError::ParentHandleMismatch),
            SurfaceState::Attached { .. } => {
                self.state = SurfaceState::Detached;
                self.geometry = None;
                self.focused = false;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
