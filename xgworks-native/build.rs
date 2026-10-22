//! Records the git commits (app and core) and the build date for the
//! version shown in the title bar and the About window.

use std::process::Command;

fn git(dir: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).current_dir(dir).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn commit(dir: &str) -> String {
    let c = git(dir, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(dir, &["status", "--porcelain", "--untracked-files=no"]).map(|s| !s.is_empty()).unwrap_or(false);
    if dirty {
        format!("{c}+changes")
    } else {
        c
    }
}

/// UTC date "YYYY-MM-DD HH:MM" without extra crates.
fn build_date() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil from days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}

fn main() {
    let here = env!("CARGO_MANIFEST_DIR");
    let core = format!("{here}/../xgworks-core");
    println!("cargo:rustc-env=APP_COMMIT={}", commit(here));
    println!("cargo:rustc-env=APP_CORE_COMMIT={}", commit(&core));
    println!("cargo:rustc-env=APP_BUILD_DATE={}", build_date());
    for f in [format!("{here}/.git/HEAD"), format!("{here}/.git/index"), format!("{core}/.git/HEAD"), format!("{core}/.git/index")] {
        println!("cargo:rerun-if-changed={f}");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
