$ErrorActionPreference = "Stop"
cargo build --release --workspace

$dist = Join-Path $PSScriptRoot "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

Copy-Item (Join-Path $PSScriptRoot "target/release/executor-ui.exe") $dist -Force
Copy-Item (Join-Path $PSScriptRoot "target/release/runtime.dll") $dist -Force

Write-Host "Built to $dist"
