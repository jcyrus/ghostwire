# Windows Installation Guide

## Quick Install

Open **PowerShell** (not Command Prompt) and run:

```powershell
irm https://ghost.jcyrus.com/install.ps1 | iex
```

## After Installation

### If `ghostwire` is not recognized

The installer automatically adds GhostWire to your PATH, but you may need to refresh your terminal session.

**Option 1: Restart your terminal** (Recommended)

- Close PowerShell completely
- Open a new PowerShell window
- Try running `ghostwire` again

**Option 2: Refresh PATH in current session**

```powershell
$env:Path = [System.Environment]::GetEnvironmentVariable('Path','User')
```

**Option 3: Run with full path**

```powershell
& "$env:LOCALAPPDATA\GhostWire\ghostwire.exe"
```

## Verify Installation

Check if GhostWire is installed:

```powershell
Get-Command ghostwire
```

Check your PATH:

```powershell
$env:Path -split ';' | Select-String "GhostWire"
```

## Uninstall

To remove GhostWire:

```powershell
# Remove the binary
Remove-Item "$env:LOCALAPPDATA\GhostWire" -Recurse -Force

# Remove from PATH (manual)
# 1. Press Win + X, select "System"
# 2. Click "Advanced system settings"
# 3. Click "Environment Variables"
# 4. Under "User variables", select "Path" and click "Edit"
# 5. Remove the entry containing "GhostWire"
# 6. Click OK
```

## Troubleshooting

### PowerShell Execution Policy

If you get an error about execution policy, run:

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

Then try the installation again.

### Download Fails

If the download fails, you can manually download the binary:

1. Go to https://github.com/jcyrus/GhostWire/releases/latest
2. Download `ghostwire-windows-amd64.exe`
3. Create directory: `New-Item -ItemType Directory -Path "$env:LOCALAPPDATA\GhostWire" -Force`
4. Move the file: `Move-Item ghostwire-windows-amd64.exe "$env:LOCALAPPDATA\GhostWire\ghostwire.exe"`
5. Add to PATH manually (see Uninstall section for PATH editor instructions)

## Usage

After installation, connect to the public relay:

```powershell
ghostwire your_username
```

For more usage information, see the main [README](../../README.md#-usage) or the [User Guide](GUIDE.md).
