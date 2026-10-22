//! HS2 – Hypersynth 2 version of this library (recorded by build.rs).

pub const NAME: &str = "HS2 – Hypersynth 2";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COMMIT: &str = env!("APP_COMMIT");
pub const CORE_COMMIT: &str = env!("APP_CORE_COMMIT");
pub const BUILD_DATE: &str = env!("APP_BUILD_DATE");

/// Engine part of the version text; the Swift side adds the app build.
pub fn about() -> String {
    format!("{NAME} engine {VERSION}\nEngine commit: {COMMIT}\nCore commit: {CORE_COMMIT}\nEngine built: {BUILD_DATE}")
}
