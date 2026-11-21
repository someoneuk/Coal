# Coal — Executor (Rust)

Lightweight internal-executor foundation: a native Win32 launcher with a dependency-free DLL inj backend, an injected Lua VM, and a script editor that drives it over local IPC.

## Why it is light

- The GUI is plain Win32 controls — no `eframe`/`egui`/`wgpu` stack (~400 crates removed).
- The file picker uses `GetOpenFileNameA`; process discovery uses Toolhelp32 — no `rfd`/`sysinfo`.
- The embedded VM is vendored **Lua 5.1** (small C build), not Luau C++.
- Release profile uses `opt-level = 2`, `strip = true`, no LTO.
- Total direct dependencies: `coal-shared` and `mlua` (plus their small transitive set).

A clean release build takes well under a minute.

## Layout

```
.
├── build.ps1                # release build -> dist/
├── Cargo.toml               # workspace
└── crates/
    ├── shared/              # IPC framing + named-pipe transport
    │   └── src/ipc.rs
    ├── launcher/            # Win32 GUI + inj backend (binary: executor-ui)
    │   └── src/
    │       ├── main.rs
    │       ├── lib.rs
    │       ├── app.rs       # window, controls, background actions
    │       ├── log.rs
    │       ├── win32.rs     # raw declarations
    │       └── inj/
    │           ├── mod.rs
    │           ├── process.rs   # Toolhelp32 target discovery
    │           └── inject.rs    # LoadLibraryA remote-thread inj
    │       └── examples/e2e.rs  # injection + Lua + IPC smoke test
    └── runtime/             # injected cdylib (runtime.dll)
        └── src/
            ├── lib.rs       # DllMain -> IPC server thread
            └── vm.rs        # long-lived Lua VM
```

## Build

```powershell
.\build.ps1
```

If PowerShell blocks local scripts, run `powershell -ExecutionPolicy Bypass -File .\build.ps1` instead.

Outputs land in `dist/`:

- `executor-ui.exe` — launcher
- `runtime.dll` — injected library with the embedded Lua VM

Or build manually:

```powershell
cargo build --release --workspace --all-targets
```

Use `--all-targets` if you also want the `e2e` example; plain `--workspace` builds the two deliverables.

## Use

1. Launch `executor-ui.exe`.
2. Set the target process name (default `RobloxPlayerBeta`) and hit **Refresh** until the status reads `RUNNING`.
3. Point **DLL** at `runtime.dll` (or browse) and click **Inject**.
4. Write Lua in the **Script** box and click **Run**. Return values and errors appear in the log.

The injected runtime also writes diagnostics to `%TEMP%\coal-runtime.log`.

Example script:

```lua
print("hello from coal")
return 1 + 1
```

## Verify

With the workspace built:

```powershell
cargo run --release -p launcher --example e2e
```

This injects `runtime.dll` into a temporary Notepad process, runs `return 40 + 2` through the VM, asserts the result is `42`, and closes Notepad.

## Design notes

- Injection uses raw Win32 declarations: open process → resolve `LoadLibraryA` in `kernel32` → allocate + write the DLL path in the target → `CreateRemoteThread` at `LoadLibraryA` → wait and read the exit code. Guards clean up handles and remote memory on every path.
- `DllMain` only calls `DisableThreadLibraryCalls` and spawns a worker thread; no meaningful work happens under the loader lock.
- The worker creates one `mlua` Lua 5.1 VM and keeps it alive, so globals persist between scripts. It then hosts a named-pipe server at `\\.\pipe\coal-executor-<pid>`.
- Framing is a 4-byte little-endian length followed by UTF-8 bytes. Responses are prefixed `OK\n` or `ERR\n`. The launcher connects by target PID, so no discovery or configuration is needed.
- Scripts run on the IPC thread inside the embedded VM. They are sandboxed to that VM — they do not yet touch the target game's own Lua state.

## Where to extend

- `crates/runtime/src/vm.rs` — expose native bindings, seed the environment, or bridge into the game's own Lua state.
- `crates/runtime/src/lib.rs` — install hooks from the thread entry point.
- `crates/launcher/src/inj/inject.rs` — swap the LoadLibrary route for manual mapping if a stealthier load is needed.
- `crates/launcher/src/app.rs` — add saved scripts, profiles, syntax highlighting.
