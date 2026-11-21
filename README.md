# Executor (Rust)

Lightweight internal-executor foundation: an egui launcher with a dependency-free DLL inj backend, plus the injected library that gets loaded into the target.

## Layout

```
.
├── build.ps1                # release build -> dist/
├── Cargo.toml               # workspace
└── crates/
    ├── launcher/            # GUI + inj backend (binary: executor-ui)
    │   └── src/
    │       ├── main.rs      # eframe entry point
    │       ├── app.rs       # UI state + panels
    │       ├── log.rs       # log entries / levels
    │       └── inj/
    │           ├── mod.rs
    │           ├── process.rs   # target process discovery
    │           └── inject.rs    # LoadLibraryA remote-thread injection
    └── runtime/             # injected cdylib (runtime.dll)
        └── src/lib.rs       # DllMain -> worker thread
```

## Build

```powershell
.\build.ps1
```

If PowerShell blocks local scripts, run `powershell -ExecutionPolicy Bypass -File .\build.ps1` instead.

Outputs land in `dist/`:

- `executor-ui.exe` — launcher
- `runtime.dll` — library to inject

Or build manually:

```powershell
cargo build --release --workspace
```

## Use

1. Launch `executor-ui.exe`.
2. Set the target process name (default `RobloxPlayerBeta`) and hit **Refresh** until status shows `RUNNING`.
3. Point **DLL** at `runtime.dll` (or browse).
4. Click **Inject**. Success is reported in the log; the injected library writes a confirmation line to `%TEMP%\executor-runtime.log`.

## Design notes

- The inj path uses raw Win32 declarations, so the only real dependencies are the GUI stack (`eframe`/`egui`), the file picker (`rfd`), and process enumeration (`sysinfo`).
- Injection follows the standard route: open process → resolve `LoadLibraryA` in `kernel32` → allocate + write the DLL path in the target → `CreateRemoteThread` at `LoadLibraryA` → wait and read the exit code. Guards clean up handles and remote memory on every path.
- The injected `DllMain` only calls `DisableThreadLibraryCalls` and spawns a worker thread; no meaningful work happens under the loader lock. The worker writes a marker file and is the intended home for future initialization.

## Where to extend

- `crates/runtime/src/lib.rs` — initialize the scripting/runtime environment, install hooks, set up IPC with the launcher.
- `crates/launcher/src/inj/inject.rs` — swap the LoadLibrary route for manual mapping if a stealthier load is needed.
- `crates/launcher/src/app.rs` — add tabs, script editor, saved profiles, etc.
