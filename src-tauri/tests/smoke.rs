//! Integration smoke test — also guarantees the package has a test target so
//! the build script's `cargo::rustc-link-arg-tests` manifest directive is
//! accepted by cargo (Windows test binaries need the embedded Common-Controls
//! v6 manifest; see src-tauri/build.rs).

#[test]
fn crate_smoke() {
    // The lib links and its public command surface resolves.
    assert!(app::retry::compute_retry_backoff_ms(1, None, || 1.0) > 0);
}