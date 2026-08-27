use super::*;

fn geometry() -> SurfaceGeometry {
    SurfaceGeometry {
        width: 1280,
        height: 720,
        scale_factor: 2.0,
    }
}

#[test]
fn lifecycle_requires_a_real_parent_and_valid_geometry() {
    let mut lifecycle = EmbeddedSurfaceLifecycle::default();

    assert_eq!(
        lifecycle.attach(AttachRequest {
            parent_handle: 0,
            geometry: geometry(),
        }),
        Err(SurfaceError::NullParentHandle)
    );
    assert_eq!(
        lifecycle.attach(AttachRequest {
            parent_handle: 42,
            geometry: SurfaceGeometry {
                width: 0,
                ..geometry()
            },
        }),
        Err(SurfaceError::InvalidGeometry)
    );
    assert_eq!(lifecycle.state(), SurfaceState::Detached);
}

#[test]
fn lifecycle_attaches_resizes_focuses_and_detaches() {
    let mut lifecycle = EmbeddedSurfaceLifecycle::default();
    lifecycle
        .attach(AttachRequest {
            parent_handle: 42,
            geometry: geometry(),
        })
        .unwrap();

    let resized = SurfaceGeometry {
        width: 800,
        height: 600,
        scale_factor: 1.0,
    };
    lifecycle.resize(resized).unwrap();
    lifecycle.set_focused(true).unwrap();

    assert_eq!(lifecycle.geometry(), Some(resized));
    assert!(lifecycle.is_focused());
    assert_eq!(lifecycle.detach(7), Err(SurfaceError::ParentHandleMismatch));
    lifecycle.detach(42).unwrap();
    assert_eq!(lifecycle.state(), SurfaceState::Detached);
    assert_eq!(lifecycle.geometry(), None);
    assert!(!lifecycle.is_focused());
}

#[test]
fn lifecycle_fails_closed_for_invalid_transitions() {
    let mut lifecycle = EmbeddedSurfaceLifecycle::default();

    assert_eq!(lifecycle.resize(geometry()), Err(SurfaceError::NotAttached));
    assert_eq!(lifecycle.set_focused(true), Err(SurfaceError::NotAttached));
    assert_eq!(lifecycle.detach(42), Err(SurfaceError::NotAttached));

    lifecycle
        .attach(AttachRequest {
            parent_handle: 42,
            geometry: geometry(),
        })
        .unwrap();
    assert_eq!(
        lifecycle.attach(AttachRequest {
            parent_handle: 43,
            geometry: geometry(),
        }),
        Err(SurfaceError::AlreadyAttached)
    );
}
