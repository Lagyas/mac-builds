# Build the browser version into web/pkg (needs: rustup target add wasm32-unknown-unknown;
# cargo install wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock>).
$ErrorActionPreference = 'Stop'
$cargo = "$env:USERPROFILE\.cargo\bin"
Push-Location $PSScriptRoot
try {
    & "$cargo\cargo.exe" build --release --target wasm32-unknown-unknown
    if ($LASTEXITCODE) { throw "cargo build failed" }
    & "$cargo\wasm-bindgen.exe" --target web --no-typescript --out-dir web/pkg target/wasm32-unknown-unknown/release/xgworks_web.wasm
    if ($LASTEXITCODE) { throw "wasm-bindgen failed" }
    "built web/pkg"
} finally { Pop-Location }
