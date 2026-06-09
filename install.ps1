# ZeroDrop Windows Installer
# "The server knows nothing. The terminal is everything."

$ErrorActionPreference = "Stop"

$REPO = "jcyrus/zerodrop"
$BINARY_NAME = "zerodrop.exe"
$INSTALL_DIR = "$env:LOCALAPPDATA\ZeroDrop"

Write-Host "👻 Initializing ZeroDrop Sequence..." -ForegroundColor Green

# Detect Architecture
$ARCH = if ([Environment]::Is64BitOperatingSystem) { "amd64" } else { "386" }
$ASSET_NAME = "zerodrop-windows-$ARCH.exe"

Write-Host "Detected: Windows $ARCH" -ForegroundColor Green

# Get Latest Release URL
Write-Host "Fetching latest frequency..."
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$REPO/releases/latest"
    $asset = $release.assets | Where-Object { $_.name -eq $ASSET_NAME }
    
    if (-not $asset) {
        Write-Host "❌ Could not find release asset for $ASSET_NAME" -ForegroundColor Red
        Write-Host "Available assets:" -ForegroundColor Yellow
        $release.assets | ForEach-Object { Write-Host "  - $($_.name)" -ForegroundColor Yellow }
        Write-Host "`nPlease check https://github.com/$REPO/releases" -ForegroundColor Yellow
        exit 1
    }
    
    $DOWNLOAD_URL = $asset.browser_download_url
} catch {
    Write-Host "❌ Failed to fetch release information: $_" -ForegroundColor Red
    exit 1
}

# Create install directory
Write-Host "Creating installation directory..."
if (-not (Test-Path $INSTALL_DIR)) {
    New-Item -ItemType Directory -Path $INSTALL_DIR -Force | Out-Null
}

# Download
Write-Host "Downloading payload..."
$TEMP_FILE = "$env:TEMP\$BINARY_NAME"
try {
    Invoke-WebRequest -Uri $DOWNLOAD_URL -OutFile $TEMP_FILE -UseBasicParsing
} catch {
    Write-Host "❌ Download failed: $_" -ForegroundColor Red
    exit 1
}

# Move to install directory
Write-Host "Installing to system..."
Move-Item -Path $TEMP_FILE -Destination "$INSTALL_DIR\$BINARY_NAME" -Force

# Add to PATH if not already present
$USER_PATH = [Environment]::GetEnvironmentVariable("Path", "User")
if ($USER_PATH -notlike "*$INSTALL_DIR*") {
    Write-Host "Adding to PATH..."
    [Environment]::SetEnvironmentVariable(
        "Path",
        "$USER_PATH;$INSTALL_DIR",
        "User"
    )
    
    # Update current session PATH
    $env:Path = "$env:Path;$INSTALL_DIR"
    
    Write-Host "✅ Added $INSTALL_DIR to your PATH" -ForegroundColor Green
    Write-Host "⚠️  You may need to restart your terminal for PATH changes to take effect" -ForegroundColor Yellow
} else {
    Write-Host "✅ $INSTALL_DIR already in PATH" -ForegroundColor Green
}

Write-Host ""
Write-Host "✅ ZeroDrop Installed Successfully!" -ForegroundColor Green
Write-Host "Installed to: $INSTALL_DIR\$BINARY_NAME" -ForegroundColor Cyan
Write-Host ""
Write-Host "Run with: " -NoNewline
Write-Host "zerodrop" -ForegroundColor Green
Write-Host ""
Write-Host "If the command is not found, restart your terminal or run:" -ForegroundColor Yellow
Write-Host "  `$env:Path = [System.Environment]::GetEnvironmentVariable('Path','User')" -ForegroundColor Cyan
