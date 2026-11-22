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

## Injection methods and Roblox

The launcher's **Inject** button uses the stock `CreateRemoteThread` route. The codebase also contains experimental alternatives used by `examples/probe.rs`:

| Method | Switch | Result on ordinary processes | Result on Roblox (current build) |
| --- | --- | --- | --- |
| `CreateRemoteThread` | default | works | thread exits with `STATUS_INVALID_THREAD` (`0xC000071C`) |
| `NtCreateThreadEx` | `--nt` | works | same `STATUS_INVALID_THREAD` |
| APC queue | `--apc` | works | never fires (no alertable thread) |
| Thread hijack (one thread) | `--hijack` | works | DLL does not load |
| Thread hijack (all threads) | `--hijack-all` | works | avoid — can hang the target |

Roblox's anti-cheat blocks these user-mode routes. The launcher now verifies the module is actually present after injection, so it reports failure instead of claiming success. `LoadLibraryA` itself is not hooked in the target — the rejection happens at thread creation / module-load level.

## Research notes: Hyperion and loaders

I researched current public material (RMMInject, Hyperion analysis write-ups, offset indexes, manual-mapping references). The important findings:

- Hyperion encrypts Roblox's `.text` and decrypts it JIT, and maintains a page-integrity whitelist keyed by a hash of each page. Pages that aren't whitelisted are rejected. That is why `LoadLibrary` remote threads, `NtCreateThreadEx`, APC, and context hijack all fail here even though they work on ordinary processes.
- The public working approach ([RMMInject](https://github.com/Deccatron/RMMInject)) doesn't create a thread. It hooks `ntdll!NtQuerySystemInformation` in the target by patching the target jump in its syscall stub. When Roblox calls that API, a self-contained detour runs **inside** the target and:
  1. inserts the detour page and the mapped DLL's pages into Hyperion's whitelist set using `insert_set` and the set pointer from `RobloxPlayerBeta.dll`;
  2. applies base relocations;
  3. resolves imports with the target's own `GetModuleHandleA`/`LoadLibraryA`/`GetProcAddress`;
  4. runs TLS callbacks and `DllMain`;
  5. restores the hook.
- The three values that make this work — `kPageHash`, `Offset_InsertSet`, and `Offset_WhitelistedPages` — are version-specific. Published values are for `version-1e91b4133e334c9c`; this machine runs `version-2c6fd5e09aaf4cfa`. They must be rediscovered per build (IDA/reversing) or obtained from a current offset index. Several "auto-updated offset" repositories turned out to be low-quality or non-substantive, so I did not bake unverified numbers into the injector.

I implemented the generic half of that pipeline in `crates/launcher/src/inj/manual.rs`: PE parsing, section mapping, base relocations, import resolution, TLS callback discovery, and a remote call-table stub driven through a hijacked thread. It is exposed only through `probe --manual`, **not** the GUI, because it is not reliable yet: invoking a Rust/MSVC DLL entry point without the real loader's setup still crashes ordinary targets. A production fix needs either import/TLS execution through the target's own loader (as RMMInject does) or a DLL built to tolerate manual mapping, plus the per-version Hyperion offsets.

The embedded Lua VM and IPC remain verified end-to-end; the loader is the open piece.

`examples/probe.rs` usage:

```powershell
cargo run --release -p launcher --example probe -- <pid> [--nt | --apc | --hijack | --hijack-all | --ll]
```

## Reference executor analysis (`reference/`)

The supplied reference is the Solara V3 client. What its files reveal:

- `Solara.exe` — WPF UI (Wpf.Ui + WebView2 hosting a Monaco editor). It has a bridge port/token, `ExecuteScriptAsync`, a multi-client manager that scans Roblox instances, a ScriptHub backed by scriptblox.com, and an experimental "Fix ROBLOX version by replacing the ROBLOX protocol" action.
- `SolaraV3.dll` — native x64 C++ core, ~18 MB, exporting only `SetEventSink` and `Start`. It embeds the Luau compiler/bytecode pipeline, a Crow HTTP server, ixwebsocket, nlohmann JSON, and curl/OpenSSL. Its local HTTP routes include `/execute_script`, `/state`, `/client_manager`, `/request`, `/DLI`, `/SG_`, plus a randomized route.
- The engine exposes the familiar exploit globals: `getreg`, `getgenv`, `getfenv`/`setfenv`, `getscriptbytecode`, `getscripthash`, `firesignal`, `clearqueueonteleport`, `getcustomasset`, `setreadonly`, `setscriptable`, `getthreadidentity`/`setthreadidentity`, `readinteger`/`readbuffer`/`writeinteger`, `setclipboard`, `gethwid`, and `request`.
- It imports `SetProcessValidCallTargets`, which confirms CFG call-target whitelisting is part of the real approach. I added that call to `manual.rs`; on a normal process it returns false and does not fix the mapper crash, so it is not a complete solution on its own.
- `bin/ALGA` (token/hash), `bin/DLCTBL_FRTS` (encoded table), `bin/version.txt` (`3.293`); `SLaunch.exe` is a WinHTTP bootstrapper.

The actual Hyperion bypass lives in the packed `.cmdsoft` section of `SolaraV3.dll`; there is no readable source for it, so it cannot be copied from these files. The useful takeaways are the architecture (UI bridge + token, local HTTP routes, per-client manager), the Luau runtime, and the CFG whitelisting clue.

## Where to extend

- `crates/runtime/src/vm.rs` — expose native bindings, seed the environment, or bridge into the game's own Lua state.
- `crates/runtime/src/lib.rs` — install hooks from the thread entry point.
- `crates/launcher/src/inj/inject.rs` — swap the LoadLibrary route for manual mapping if a stealthier load is needed.
- `crates/launcher/src/app.rs` — add saved scripts, profiles, syntax highlighting.
