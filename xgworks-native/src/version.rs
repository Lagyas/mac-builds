//! XGWorks NextGen version: Cargo version + git commits + build date
//! (recorded by build.rs). Shown in the title bar and Help ▸ About.

pub const NAME: &str = "XGWorks NextGen";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COMMIT: &str = env!("APP_COMMIT");
pub const CORE_COMMIT: &str = env!("APP_CORE_COMMIT");
pub const BUILD_DATE: &str = env!("APP_BUILD_DATE");

/// "XGWorks NextGen 0.2.0 (a1b2c3d)" — the window title.
pub fn title() -> String {
    format!("{NAME} {VERSION} ({COMMIT})")
}

/// Full version text for About / bug reports.
pub fn about() -> String {
    format!(
        "{NAME} {VERSION}\nApp commit: {COMMIT}\nCore commit: {CORE_COMMIT}\nBuilt: {BUILD_DATE}\nPlatform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}
