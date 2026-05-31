<#
.SYNOPSIS
    DivLens MCP — Smart Installer for Windows
    https://github.com/Lohithry/divlens-mcp

.DESCRIPTION
    Downloads the correct pre-built binary for your Windows machine,
    installs it to %LOCALAPPDATA%\DivLens\ (no admin rights required),
    adds it to your user PATH, and automatically configures
    Claude Desktop, Cursor, Windsurf, and Antigravity.

.USAGE
    Run in PowerShell (no admin needed):
    irm https://raw.githubusercontent.com/Lohithry/divlens-mcp/main/install.ps1 | iex

.NOTES
    Requires: PowerShell 5.1+ (Windows 10/11 built-in)
    Creates backup (.divlens.bak) before modifying any config file
#>

#Requires -Version 5.1
Set-StrictMode -Version Latest

# ─── TLS 1.2 ──────────────────────────────────────────────────────────────────
# GitHub rejects connections using legacy TLS 1.0/1.1. Older .NET hosts in
# Windows PowerShell 5.1 default to these obsolete protocols, silently
# blocking binary downloads before the installer even starts.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# ─── Constants ────────────────────────────────────────────────────────────────
$REPO           = "Lohithry/divlens-mcp"
$BINARY_NAME    = "divlens-core.exe"
$ARTIFACT_NAME  = "divlens-core-x86_64-windows.exe"
$INSTALL_DIR    = Join-Path $env:LOCALAPPDATA "DivLens"
$INSTALL_PATH   = Join-Path $INSTALL_DIR "divlens-core.exe"
$GITHUB_API     = "https://api.github.com/repos/$REPO/releases/latest"
$TMP_DIR        = $null
$VERSION        = $null

# ─── ANSI escape code (PowerShell 5.1 compatible) ────────────────────────────
# CRITICAL: The `e backtick escape is PowerShell 7+ ONLY.
# On Windows PowerShell 5.1 (the default), `e is treated as literal "e",
# causing garbled output and crashes — especially inside Start-Job.
# We use [char]0x1B which works on ALL PowerShell versions.
$ESC = [char]0x1B

# Detect ANSI support safely
$PSSupportsAnsi = $false
try {
    if ($Host.UI.SupportsVirtualTerminal) {
        $PSSupportsAnsi = $true
    } elseif ($PSVersionTable.PSVersion.Major -ge 7) {
        $PSSupportsAnsi = $true
    }
} catch {
    $PSSupportsAnsi = $false
}

function Write-Color([string]$Text, [string]$Color = "White", [switch]$NoNewline) {
    $codes = @{
        'Red'     = "${ESC}[91m"; 'Green'  = "${ESC}[92m"; 'Yellow' = "${ESC}[93m"
        'Cyan'    = "${ESC}[96m"; 'Orange' = "${ESC}[38;5;208m"; 'White' = "${ESC}[97m"
        'Dim'     = "${ESC}[2m";  'Bold'   = "${ESC}[1m";  'Reset'  = "${ESC}[0m"
    }
    if ($PSSupportsAnsi -and $codes.Contains($Color)) {
        $out = "$($codes[$Color])$Text$($codes['Reset'])"
    } else {
        $out = $Text
    }
    if ($NoNewline) { Write-Host $out -NoNewline } else { Write-Host $out }
}

function Write-Ok    { Write-Color "  $([char]0x2713)  $($args -join ' ')" 'Green' }
function Write-Warn  { Write-Color "  [!] $($args -join ' ')" 'Yellow' }
function Write-Skip  { Write-Color "  o  $($args -join ' ')" 'Dim' }
function Write-Info  { Write-Color "  ->  $($args -join ' ')" 'Cyan' }
function Write-Step  {
    Write-Host ""
    Write-Color "  $($args -join ' ')" 'Bold'
}

function Write-Line  { Write-Color "  $('─' * 50)" 'Dim' }

# ─── Safe exit function ──────────────────────────────────────────────────────
# CRITICAL: When running via `irm ... | iex`, calling `exit` terminates the
# ENTIRE PowerShell host window — not just the script. This is the #1 cause
# of "PowerShell closes suddenly without showing anything".
# Instead, we throw a custom exception that our top-level try/catch handles.
function Write-Error-Exit([string]$Msg) {
    Write-Host ""
    Write-Color "  [X] Error: $Msg" 'Red'
    Write-Host ""
    Write-Info "For help, visit: https://github.com/$REPO/issues"
    Write-Host ""
    if ($null -ne $TMP_DIR -and (Test-Path $TMP_DIR)) {
        Remove-Item $TMP_DIR -Recurse -Force -ErrorAction SilentlyContinue
    }
    # Throw instead of exit to avoid killing the PowerShell window
    throw "DIVLENS_INSTALL_FAILED: $Msg"
}

# ─── Spinner (simplified, PS 5.1 safe) ───────────────────────────────────────
# IMPORTANT: We deliberately do NOT use Start-Job for spinners.
# Start-Job in `irm | iex` context is unreliable and can crash the host.
# Instead, we use simple progress messages.
function Start-Spinner([string]$Msg) {
    Write-Host "  ... $Msg" -NoNewline
}

function Stop-Spinner {
    Write-Host ""
}

# ─── Banner ───────────────────────────────────────────────────────────────────
function Show-Banner {
    Write-Host ""
    Write-Color "  +------------------------------------------+" 'Orange'
    Write-Color "  |                                          |" 'Orange'
    Write-Color "  |   DivLens MCP  *  Windows Installer     |" 'Orange'
    Write-Color "  |   Real-time system intelligence for AI  |" 'Orange'
    Write-Color "  |                                          |" 'Orange'
    Write-Color "  +------------------------------------------+" 'Orange'
    Write-Host ""
}

# ─── Check network ────────────────────────────────────────────────────────────
function Test-Network {
    Write-Step "Checking connectivity"
    Start-Spinner "Testing GitHub API..."
    try {
        $null = Invoke-WebRequest -Uri "https://github.com" -UseBasicParsing -TimeoutSec 10 -ErrorAction Stop
        Stop-Spinner
        Write-Ok "Internet connection OK"
    } catch {
        Stop-Spinner
        Write-Error-Exit "No internet connection. Please check your network and try again."
    }
}

# ─── Detect architecture ──────────────────────────────────────────────────────
# IMPORTANT: We query the OS kernel architecture directly instead of using
# $env:PROCESSOR_ARCHITECTURE, which returns 'x86' when PowerShell runs in
# 32-bit (WoW64) mode — even on 64-bit machines. This caused false failures.
function Get-Platform {
    Write-Step "Detecting platform"

    # Primary: Use .NET RuntimeInformation (available in PS 5.1+ on Win10+)
    $arch = 'Unknown'
    try {
        $osArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
        switch ($osArch) {
            ([System.Runtime.InteropServices.Architecture]::X64)   { $arch = 'AMD64' }
            ([System.Runtime.InteropServices.Architecture]::Arm64) { $arch = 'ARM64' }
            ([System.Runtime.InteropServices.Architecture]::X86)   { $arch = 'x86' }
            default { $arch = $osArch.ToString() }
        }
    } catch {
        # Fallback: Query WMI for the OS architecture (immune to WoW64)
        try {
            $wmiArch = (Get-CimInstance Win32_OperatingSystem).OSArchitecture
            if ($wmiArch -match '64') { $arch = 'AMD64' }
            elseif ($wmiArch -match 'ARM') { $arch = 'ARM64' }
            else { $arch = 'x86' }
        } catch {
            # Last resort: environment variable (may be wrong in WoW64)
            $arch = [System.Environment]::GetEnvironmentVariable('PROCESSOR_ARCHITECTURE')
        }
    }

    switch ($arch) {
        'AMD64'  { Write-Ok "Windows x86_64 detected" }
        'ARM64'  { Write-Warn "ARM64 detected — using x86_64 binary (runs via emulation on Windows ARM)." }
        'x86'    { Write-Error-Exit "32-bit Windows is not supported. DivLens requires a 64-bit OS." }
        default  { Write-Error-Exit "Unsupported architecture: $arch" }
    }
    Write-Ok "Platform: Windows ($arch)"
}

# ─── Fetch latest version ─────────────────────────────────────────────────────
function Get-LatestVersion {
    Write-Step "Fetching latest version"
    Start-Spinner "Querying GitHub API..."
    try {
        $response = Invoke-RestMethod -Uri $GITHUB_API -TimeoutSec 15 -UseBasicParsing -ErrorAction Stop
        Stop-Spinner
        $script:VERSION = $response.tag_name
        if (-not $VERSION) { Write-Error-Exit "Could not parse version from GitHub API." }
        Write-Ok "Latest version: $VERSION"
    } catch {
        Stop-Spinner
        $statusCode = $null
        try { $statusCode = $_.Exception.Response.StatusCode.value__ } catch {}
        if ($statusCode -eq 403) {
            Write-Error-Exit "GitHub API rate limit exceeded. Wait a minute and try again."
        }
        Write-Error-Exit "GitHub API error: $($_.Exception.Message)"
    }

    # Check if already up to date
    if (Test-Path $INSTALL_PATH) {
        try {
            $currentVer = & $INSTALL_PATH --version 2>$null | Select-String '[0-9]+\.[0-9]+\.[0-9]+' | ForEach-Object { $_.Matches[0].Value }
            if ($currentVer -and "v$currentVer" -eq $VERSION) {
                Write-Ok "DivLens MCP $VERSION is already installed and up to date."
                Write-Info "Run the installer again with -Force to reinstall."
                # Return gracefully — don't use exit
                return $false
            } elseif ($currentVer) {
                Write-Info "Upgrading from v$currentVer to $VERSION"
            }
        } catch {}
    }
    return $true
}

# ─── Download binary ──────────────────────────────────────────────────────────
function Get-Binary {
    Write-Step "Downloading DivLens MCP"

    $script:TMP_DIR = Join-Path $env:TEMP "divlens-install-$(Get-Random)"
    New-Item -ItemType Directory -Path $TMP_DIR -Force | Out-Null

    $baseUrl    = "https://github.com/$REPO/releases/download/$VERSION"
    $binaryUrl  = "$baseUrl/$ARTIFACT_NAME"
    $shaUrl     = "$binaryUrl.sha256"
    $destBin    = Join-Path $TMP_DIR "divlens-core.exe"
    $destSha    = Join-Path $TMP_DIR "divlens-core.exe.sha256"

    Write-Info "Source: $binaryUrl"

    # ── Download with progress ───────────────────────────────────────────────
    Start-Spinner "Downloading $ARTIFACT_NAME..."
    try {
        $client = New-Object System.Net.WebClient
        $client.DownloadFile($binaryUrl, $destBin)
        Stop-Spinner
    } catch {
        Stop-Spinner
        Write-Error-Exit "Download failed: $($_.Exception.Message)`n  URL: $binaryUrl"
    }

    if (-not (Test-Path $destBin) -or (Get-Item $destBin).Length -eq 0) {
        Write-Error-Exit "Downloaded file is empty. The release asset may not exist for this version."
    }

    Write-Ok "Downloaded: $ARTIFACT_NAME ($([Math]::Round((Get-Item $destBin).Length / 1MB, 1)) MB)"

    # ── Verify checksum ──────────────────────────────────────────────────────
    Start-Spinner "Verifying SHA-256 checksum..."
    try {
        (New-Object System.Net.WebClient).DownloadFile($shaUrl, $destSha)
        Stop-Spinner

        $expectedLine = Get-Content $destSha -Raw
        $expected = ($expectedLine -split '\s+')[0].Trim().ToLower()
        $actual   = (Get-FileHash -Algorithm SHA256 $destBin).Hash.ToLower()

        if ($expected -ne $actual) {
            Write-Error-Exit "Checksum FAILED!`n  Expected: $expected`n  Got:      $actual`n  The download may be corrupted. Please try again."
        }
        Write-Ok "Checksum verified"
    } catch {
        Stop-Spinner
        if ($_.Exception.Message -match '404|not found') {
            Write-Warn "Checksum file not found — skipping verification."
        } elseif ($_.Exception.Message -match 'DIVLENS_INSTALL_FAILED') {
            throw  # Re-throw our own errors
        } else {
            Write-Warn "Checksum verification skipped: $($_.Exception.Message)"
        }
    }

    return $destBin
}

# ─── Install binary ───────────────────────────────────────────────────────────
function Install-Binary([string]$SrcPath) {
    Write-Step "Installing binary"

    # Create install directory (no admin required — inside %LOCALAPPDATA%)
    New-Item -ItemType Directory -Path $INSTALL_DIR -Force | Out-Null

    # Copy binary (handle locked file if previous version running)
    try {
        Copy-Item $SrcPath $INSTALL_PATH -Force
    } catch {
        Write-Error-Exit "Could not write to ${INSTALL_PATH}. Close any running instance of DivLens and try again."
    }

    Write-Ok "Installed to: $INSTALL_PATH"

    # ── Add to user PATH if not already there ─────────────────────────────────
    $userPath = [System.Environment]::GetEnvironmentVariable('PATH', 'User')
    if ($userPath -notlike "*$INSTALL_DIR*") {
        $newPath = "$INSTALL_DIR;$userPath".TrimEnd(';')
        [System.Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
        $env:PATH = "$INSTALL_DIR;$env:PATH"
        Write-Ok "Added $INSTALL_DIR to user PATH"
    } else {
        Write-Ok "PATH already contains $INSTALL_DIR"
    }
}

# ─── Clean JSON Serializer ────────────────────────────────────────────────────
# PowerShell 5.1's ConvertTo-Json produces malformed output:
#   - Double spaces after colons:  "key":  value  (instead of "key": value)
#   - Pyramid indentation that grows exponentially with nesting depth
# Claude Desktop's config parser silently rejects this formatting.
# This custom serializer produces standard, clean JSON that works everywhere.

function ConvertTo-CleanJson($obj, [int]$depth = 0) {
    $indent = '  ' * $depth
    $innerIndent = '  ' * ($depth + 1)

    if ($null -eq $obj) {
        return 'null'
    }
    if ($obj -is [bool]) {
        if ($obj) { return 'true' } else { return 'false' }
    }
    if ($obj -is [int] -or $obj -is [long] -or $obj -is [double] -or $obj -is [decimal]) {
        return "$obj"
    }
    if ($obj -is [string]) {
        # Escape backslashes, quotes, and control characters
        $escaped = $obj.Replace('\', '\\').Replace('"', '\"').Replace("`r", '').Replace("`n", '\n').Replace("`t", '\t')
        return "`"$escaped`""
    }
    if ($obj -is [System.Collections.IDictionary]) {
        $keys = @($obj.Keys)
        if ($keys.Count -eq 0) { return '{}' }
        $entries = @()
        foreach ($key in $keys) {
            $valJson = ConvertTo-CleanJson $obj[$key] ($depth + 1)
            $entries += "${innerIndent}`"${key}`": ${valJson}"
        }
        $joined = $entries -join ",`n"
        return "{`n${joined}`n${indent}}"
    }
    if ($obj -is [System.Collections.IEnumerable]) {
        $items = @()
        foreach ($item in $obj) {
            $items += $item
        }
        if ($items.Count -eq 0) { return '[]' }
        # Short arrays (single simple items) go on one line
        if ($items.Count -le 3) {
            $inlineItems = @()
            $allSimple = $true
            foreach ($item in $items) {
                if ($item -is [System.Collections.IDictionary] -or
                    ($item -is [System.Collections.IEnumerable] -and $item -isnot [string])) {
                    $allSimple = $false
                    break
                }
                $inlineItems += ConvertTo-CleanJson $item 0
            }
            if ($allSimple) {
                return "[$($inlineItems -join ', ')]"
            }
        }
        # Multi-line array
        $arrayEntries = @()
        foreach ($item in $items) {
            $arrayEntries += "${innerIndent}$(ConvertTo-CleanJson $item ($depth + 1))"
        }
        $joined = $arrayEntries -join ",`n"
        return "[`n${joined}`n${indent}]"
    }
    # PSCustomObject fallback
    if ($obj -is [System.Management.Automation.PSCustomObject]) {
        $hash = [ordered]@{}
        foreach ($prop in $obj.PSObject.Properties) {
            $hash[$prop.Name] = $prop.Value
        }
        return ConvertTo-CleanJson $hash $depth
    }
    # Fallback: treat as string
    $str = "$obj".Replace('\', '\\').Replace('"', '\"')
    return "`"$str`""
}

# Recursive helper to convert PSCustomObject to Ordered Hashtable (PS 5.1 compat)
function Convert-PSCustomObjectToHashtable($obj) {
    if ($null -eq $obj) { return $null }
    if ($obj -is [System.Management.Automation.PSCustomObject]) {
        $hash = [ordered]@{}
        foreach ($prop in $obj.PSObject.Properties) {
            $hash[$prop.Name] = Convert-PSCustomObjectToHashtable $prop.Value
        }
        return $hash
    } elseif ($obj -is [System.Collections.IEnumerable] -and $obj -isnot [string]) {
        $arr = @()
        foreach ($item in $obj) {
            $arr += Convert-PSCustomObjectToHashtable $item
        }
        return $arr
    } else {
        return $obj
    }
}

# ─── AI client config patching ────────────────────────────────────────────────
function Update-Config([string]$ClientName, [string]$ConfigPath) {
    $configDir = Split-Path $ConfigPath -Parent

    # If the config dir doesn't exist, client is likely not installed
    if (-not (Test-Path $configDir)) {
        Write-Skip "${ClientName}: not detected ($configDir not found)"
        return
    }

    # Backup existing config
    if (Test-Path $ConfigPath) {
        $backupPath = "$ConfigPath.divlens.bak"
        Copy-Item $ConfigPath $backupPath -Force
    }

    # Load or create config
    $config = [ordered]@{}
    if (Test-Path $ConfigPath) {
        $raw = Get-Content $ConfigPath -Raw -Encoding UTF8
        if ($raw.Trim()) {
            try {
                $parsed = ConvertFrom-Json $raw -ErrorAction Stop
                $config = Convert-PSCustomObjectToHashtable $parsed
            } catch {
                Write-Warn "${ClientName}: config has invalid JSON — backed up and recreating."
                $config = [ordered]@{}
            }
        }
    }

    # Ensure mcpServers key exists
    if (-not $config.Contains('mcpServers') -or $null -eq $config['mcpServers']) {
        $config['mcpServers'] = [ordered]@{}
    }

    # Add / update the divlens entry (always overwrite to ensure correctness)
    $config['mcpServers']['divlens'] = [ordered]@{
        command = $INSTALL_PATH
        args    = @('--mcp')
    }

    # Write clean JSON using our custom serializer (not PowerShell's broken ConvertTo-Json)
    New-Item -ItemType Directory -Path $configDir -Force | Out-Null
    $jsonText = ConvertTo-CleanJson $config
    $utf8NoBom = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($ConfigPath, $jsonText, $utf8NoBom)
    Write-Ok "${ClientName}: config updated ($ConfigPath)"
}

function Configure-AllClients {
    Write-Step "Connecting to AI clients"

    $AppData      = $env:APPDATA
    $LocalAppData = $env:LOCALAPPDATA
    $UserHome     = $env:USERPROFILE

    # ── Claude Desktop: Standard installation path ────────────────────────────
    Update-Config "Claude Desktop"       "$AppData\Claude\claude_desktop_config.json"

    # ── Claude Desktop: MSIX/Microsoft Store sandboxed path ───────────────────
    # The Microsoft Store version reads config from a virtualized directory that
    # is invisible to the standard %APPDATA% path. We must write to BOTH paths
    # so the server is detected regardless of how Claude was installed.
    $msixBase = "$LocalAppData\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\Claude"
    $msixConfig = "$msixBase\claude_desktop_config.json"
    # Only attempt if the MSIX package container exists (Store version installed)
    $msixPackageDir = "$LocalAppData\Packages\Claude_pzs8sxrjxfjjc"
    if (Test-Path $msixPackageDir) {
        Update-Config "Claude Desktop (Store)" $msixConfig
    } else {
        Write-Skip "Claude Desktop (Store): MSIX package not detected"
    }

    # ── Other AI clients ──────────────────────────────────────────────────────
    Update-Config "Cursor"              "$UserHome\.cursor\mcp.json"
    Update-Config "Windsurf"            "$AppData\Codeium\windsurf\mcp_config.json"
    Update-Config "Antigravity"         "$UserHome\.gemini\mcp_config.json"
}

# ─── Verify server works ─────────────────────────────────────────────────────
function Test-Server {
    Write-Step "Verifying installation"
    Start-Spinner "Testing MCP server..."

    $testPayload = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"installer","version":"1.0"}}}'
    try {
        $proc = New-Object System.Diagnostics.Process
        $proc.StartInfo.FileName               = $INSTALL_PATH
        $proc.StartInfo.Arguments              = "--mcp"
        $proc.StartInfo.UseShellExecute        = $false
        $proc.StartInfo.RedirectStandardInput  = $true
        $proc.StartInfo.RedirectStandardOutput = $true
        $proc.StartInfo.RedirectStandardError  = $true
        $proc.StartInfo.CreateNoWindow         = $true
        $proc.Start() | Out-Null
        $proc.StandardInput.WriteLine($testPayload)
        $proc.StandardInput.Close()
        $response = $proc.StandardOutput.ReadLine()
        try { $proc.Kill() } catch {}
        Stop-Spinner

        if ($response -match '"result"') {
            Write-Ok "Server test passed — MCP protocol confirmed working"
        } else {
            Write-Warn "Unexpected server response (binary installed correctly — restart your AI client)."
        }
    } catch {
        Stop-Spinner
        Write-Warn "Server test skipped — restart your AI client to activate DivLens."
    }
}

# ─── Success message ─────────────────────────────────────────────────────────
function Show-Success {
    Write-Host ""
    Write-Color "  +------------------------------------------+" 'Green'
    Write-Color "  |   [OK] DivLens MCP is ready!            |" 'Green'
    Write-Color "  +------------------------------------------+" 'Green'
    Write-Host ""
    Write-Info  "Binary:  $INSTALL_PATH"
    Write-Info  "Version: $VERSION"
    Write-Host ""
    Write-Line
    Write-Host ""
    Write-Color "  Next steps:" 'Bold'
    Write-Host ""
    Write-Host  "  1. Restart Claude Desktop or Cursor"
    Write-Color '  2. Ask: "What is using my CPU right now?"' 'Cyan'
    Write-Color '        "Is my SSD healthy?"' 'Cyan'
    Write-Color '        "What is eating my disk space?"' 'Cyan'
    Write-Host ""
    Write-Color "  Management:" 'Bold'
    Write-Host ""
    Write-Color '  divlens-core.exe status           Show installation status' 'Cyan'
    Write-Color '  divlens-core.exe doctor           Run health checks' 'Cyan'
    Write-Color '  divlens-core.exe config --show    Show AI client configs' 'Cyan'
    Write-Color '  divlens-core.exe uninstall        Remove DivLens completely' 'Cyan'
    Write-Host ""
    Write-Line
    Write-Host ""
    Write-Info  "Docs:   https://github.com/$REPO"
    Write-Info  "Issues: https://github.com/$REPO/issues"
    Write-Host ""
}

# ─── MAIN EXECUTION ──────────────────────────────────────────────────────────
# CRITICAL: Wrap everything in try/catch so errors display a message instead
# of silently killing the PowerShell window.
# When run via `irm ... | iex`, an unhandled error or `exit` terminates
# the ENTIRE PowerShell host — the window just closes with no output.
try {
    Show-Banner
    Test-Network
    Get-Platform
    $shouldContinue = Get-LatestVersion
    if ($shouldContinue -eq $false) {
        # Already up to date — exit gracefully without closing the window
        Write-Host ""
        return
    }
    $binaryPath = Get-Binary
    Install-Binary $binaryPath
    Configure-AllClients
    Test-Server
    Show-Success
} catch {
    $errMsg = $_.Exception.Message
    if ($errMsg -match 'DIVLENS_INSTALL_FAILED') {
        # Error already printed by Write-Error-Exit — just stop
    } else {
        # Unexpected error — show it so the user can report it
        Write-Host ""
        Write-Host "  [X] Unexpected error during installation:" -ForegroundColor Red
        Write-Host ""
        Write-Host "  $errMsg" -ForegroundColor Red
        Write-Host ""
        Write-Host "  Full error:" -ForegroundColor Yellow
        Write-Host "  $($_.ScriptStackTrace)" -ForegroundColor Yellow
        Write-Host ""
        Write-Host "  Please report this at: https://github.com/$REPO/issues" -ForegroundColor Cyan
        Write-Host ""
    }
} finally {
    # Cleanup temp files
    if ($null -ne $TMP_DIR -and (Test-Path $TMP_DIR)) {
        Remove-Item $TMP_DIR -Recurse -Force -ErrorAction SilentlyContinue
    }
    # IMPORTANT: Do NOT call exit here. Let PowerShell return to the prompt.
    # Using `return` keeps the window open. Using `exit` closes it.

    # Pause so the user can see the output when run by double-clicking
    # (has no effect when run from an existing PowerShell prompt)
    Write-Host "  Press Enter to close..." -ForegroundColor DarkGray
    $null = Read-Host
}
