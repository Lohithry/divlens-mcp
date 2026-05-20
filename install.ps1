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
$ErrorActionPreference = 'Stop'

# ─── Constants ────────────────────────────────────────────────────────────────
$REPO           = "Lohithry/divlens-mcp"
$BINARY_NAME    = "divlens-core.exe"
$ARTIFACT_NAME  = "divlens-core-x86_64-windows.exe"
$INSTALL_DIR    = Join-Path $env:LOCALAPPDATA "DivLens"
$INSTALL_PATH   = Join-Path $INSTALL_DIR "divlens-core.exe"
$GITHUB_API     = "https://api.github.com/repos/$REPO/releases/latest"
$TMP_DIR        = $null
$VERSION        = $null

# ─── ANSI colours (PowerShell 7+ native; fallback for 5.1) ───────────────────
$PSSupportsAnsi = $Host.UI.SupportsVirtualTerminal
function Write-Color([string]$Text, [string]$Color = "White", [switch]$NoNewline) {
    $codes = @{
        'Red'     = "`e[91m"; 'Green'  = "`e[92m"; 'Yellow' = "`e[93m"
        'Cyan'    = "`e[96m"; 'Orange' = "`e[38;5;208m"; 'White' = "`e[97m"
        'Dim'     = "`e[2m";  'Bold'   = "`e[1m";  'Reset'  = "`e[0m"
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

function Write-Error-Exit([string]$Msg) {
    Write-Host ""
    Write-Color "  [X] Error: $Msg" 'Red'
    Write-Host ""
    Write-Info "For help, visit: https://github.com/$REPO/issues"
    Write-Host ""
    if ($null -ne $TMP_DIR -and (Test-Path $TMP_DIR)) {
        Remove-Item $TMP_DIR -Recurse -Force -ErrorAction SilentlyContinue
    }
    exit 1
}

# ─── Spinner ──────────────────────────────────────────────────────────────────
$SpinnerJob = $null

function Start-Spinner([string]$Msg) {
    Stop-Spinner
    if (-not $PSSupportsAnsi) { Write-Host "  ... $Msg"; return }
    $script:SpinnerJob = Start-Job -ScriptBlock {
        param($m)
        $frames = @('⠋','⠙','⠹','⠸','⠼','⠴','⠦','⠧','⠇','⠏')
        $i = 0
        [Console]::CursorVisible = $false
        while ($true) {
            [Console]::Write("`r  `e[96m$($frames[$i])`e[0m  `e[2m$m`e[0m   ")
            $i = ($i + 1) % $frames.Length
            Start-Sleep -Milliseconds 80
        }
    } -ArgumentList $Msg
}

function Stop-Spinner {
    if ($null -ne $script:SpinnerJob) {
        Stop-Job  $script:SpinnerJob -ErrorAction SilentlyContinue
        Remove-Job $script:SpinnerJob -ErrorAction SilentlyContinue
        $script:SpinnerJob = $null
        [Console]::Write("`r" + (" " * 60) + "`r")
        try { [Console]::CursorVisible = $true } catch {}
    }
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
    Start-Spinner "Testing GitHub API…"
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
function Get-Platform {
    Write-Step "Detecting platform"
    $arch = [System.Environment]::GetEnvironmentVariable('PROCESSOR_ARCHITECTURE')
    switch ($arch) {
        'AMD64'  { Write-Ok "Windows x86_64 detected" }
        'ARM64'  { Write-Warn "ARM64 detected — using x86_64 binary (runs via emulation on Windows ARM)." }
        default  { Write-Error-Exit "Unsupported architecture: $arch" }
    }
    Write-Ok "Platform: Windows ($arch)"
}

# ─── Fetch latest version ─────────────────────────────────────────────────────
function Get-LatestVersion {
    Write-Step "Fetching latest version"
    Start-Spinner "Querying GitHub API…"
    try {
        $response = Invoke-RestMethod -Uri $GITHUB_API -TimeoutSec 15 -UseBasicParsing -ErrorAction Stop
        Stop-Spinner
        $script:VERSION = $response.tag_name
        if (-not $VERSION) { Write-Error-Exit "Could not parse version from GitHub API." }
        Write-Ok "Latest version: $VERSION"
    } catch {
        Stop-Spinner
        $statusCode = $_.Exception.Response.StatusCode.value__
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
                exit 0
            } elseif ($currentVer) {
                Write-Info "Upgrading from v$currentVer to $VERSION"
            }
        } catch {}
    }
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
    Start-Spinner "Downloading $ARTIFACT_NAME…"
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
    Start-Spinner "Verifying SHA-256 checksum…"
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

# Recursive helper to convert PSCustomObject to Ordered Hashtable on older PowerShell versions
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

    # Ensure mcpServers key
    if (-not $config.Contains('mcpServers') -or $null -eq $config['mcpServers']) {
        $config['mcpServers'] = [ordered]@{}
    }

    # Check if already configured correctly
    $existing = $config['mcpServers']['divlens']
    if ($existing -and $existing.command -eq $INSTALL_PATH -and
        ($existing.args -is [array]) -and $existing.args -contains '--mcp') {
        Write-Ok "${ClientName}: already configured correctly"
        return
    }

    # Add / update entry
    $config['mcpServers']['divlens'] = [ordered]@{
        command = $INSTALL_PATH
        args    = @('--mcp')
    }

    # Ensure parent dir exists and write
    New-Item -ItemType Directory -Path $configDir -Force | Out-Null
    ConvertTo-Json $config -Depth 10 | Set-Content $ConfigPath -Encoding UTF8
    Write-Ok "${ClientName}: config updated ($ConfigPath)"
}

function Configure-AllClients {
    Write-Step "Connecting to AI clients"

    $AppData  = $env:APPDATA
    $UserHome = $env:USERPROFILE

    Update-Config "Claude Desktop"  "$AppData\Claude\claude_desktop_config.json"
    Update-Config "Cursor"          "$UserHome\.cursor\mcp.json"
    Update-Config "Windsurf"        "$AppData\Codeium\windsurf\mcp_config.json"
    Update-Config "Antigravity"     "$UserHome\.gemini\mcp_config.json"
}

# ─── Verify server works ─────────────────────────────────────────────────────
function Test-Server {
    Write-Step "Verifying installation"
    Start-Spinner "Testing MCP server…"

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
        $proc.Kill()
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
    Write-Line
    Write-Host ""
    Write-Info  "Docs:   https://github.com/$REPO"
    Write-Info  "Issues: https://github.com/$REPO/issues"
    Write-Host ""
}

# ─── Cleanup on error ─────────────────────────────────────────────────────────
Register-EngineEvent PowerShell.Exiting -Action {
    Stop-Spinner
    if ($null -ne $TMP_DIR -and (Test-Path $TMP_DIR)) {
        Remove-Item $TMP_DIR -Recurse -Force -ErrorAction SilentlyContinue
    }
} | Out-Null

# ─── Main ─────────────────────────────────────────────────────────────────────
Show-Banner
Test-Network
Get-Platform
Get-LatestVersion
$binaryPath = Get-Binary
Install-Binary $binaryPath
Configure-AllClients
Test-Server
Show-Success

# Cleanup temp files
if ($null -ne $TMP_DIR -and (Test-Path $TMP_DIR)) {
    Remove-Item $TMP_DIR -Recurse -Force -ErrorAction SilentlyContinue
}
