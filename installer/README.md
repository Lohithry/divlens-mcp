# DivLens Installer — Developer Guide

## Architecture

```
installer/
├── unix/                    # macOS + Linux
│   ├── install.sh           # Entry point — sources all lib/ modules
│   ├── uninstall.sh         # Standalone uninstaller
│   └── lib/
│       ├── ui.sh            # Colors, spinners, banners, print helpers
│       ├── platform.sh      # OS/arch detection (Rosetta 2 aware)
│       ├── download.sh      # Dependencies, version fetch, download, checksum
│       ├── install.sh       # Binary placement (sudo → local fallback)
│       ├── configure.sh     # AI client config patching (Python 3 JSON)
│       └── service.sh       # launchd (macOS) / systemd (Linux)
│
├── windows/                 # Windows
│   ├── install.ps1          # Entry point — dot-sources all lib/ modules
│   ├── uninstall.ps1        # Standalone uninstaller
│   └── lib/
│       ├── UI.ps1           # Colors, spinners, banners
│       ├── Platform.ps1     # Architecture detection (WoW64-safe)
│       ├── Download.ps1     # Version fetch, download, checksum
│       ├── Install.ps1      # Binary placement, PATH management
│       ├── Configure.ps1    # BOM-free JSON, MSIX dual-path writing
│       └── Service.ps1      # Task Scheduler registration
│
└── README.md                # This file
```

## Design Principles

1. **Modular**: Each concern (UI, platform, download, install, configure, service) is a separate file
2. **Testable**: Modules can be sourced independently for unit testing
3. **Cross-platform**: Same logical flow, platform-specific implementations
4. **No admin required**: Installs to user-writable directories
5. **Safe**: Backups before every config modification, BOM-free writes on Windows

## Root Scripts

The root `install.sh` and `install.ps1` are the **distributable** versions used by
the one-liner install commands. They are self-contained copies that combine all
modules into a single file for easy `curl | bash` / `irm | iex` usage.

## Key Windows Fixes

| Issue | Root Cause | Solution |
|:---|:---|:---|
| Download fails | TLS 1.0/1.1 default | Force TLS 1.2 via `[Net.ServicePointManager]` |
| Config not read | MSIX sandbox | Write to both `%APPDATA%` and MSIX LocalCache path |
| JSON parse fails | UTF-8 BOM | Use `[System.IO.File]::WriteAllText` with `UTF8Encoding($false)` |
| Wrong arch | WoW64 emulation | Query `RuntimeInformation.OSArchitecture` instead of env var |
