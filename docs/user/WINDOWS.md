# Windows Installation Guide

## Quick Install

Open **PowerShell** (not Command Prompt) and run:

```powershell
irm https://zerodrop.jcyrus.com/install.ps1 | iex
```

## After Installation

### If `zerodrop` is not recognized

The installer automatically adds ZeroDrop to your PATH, but you may need to refresh your terminal session.

**Option 1: Restart your terminal** (Recommended)

- Close PowerShell completely
- Open a new PowerShell window
- Try running `zerodrop` again

**Option 2: Refresh PATH in current session**

```powershell
$env:Path = [System.Environment]::GetEnvironmentVariable('Path','User')
```

**Option 3: Run with full path**

```powershell
& "$env:LOCALAPPDATA\ZeroDrop\zerodrop.exe"
```

## Verify Installation

Check if ZeroDrop is installed:

```powershell
Get-Command zerodrop
```

Check your PATH:

```powershell
$env:Path -split ';' | Select-String "ZeroDrop"
```

## Uninstall

To remove ZeroDrop:

```powershell
# Remove the binary
Remove-Item "$env:LOCALAPPDATA\ZeroDrop" -Recurse -Force

# Remove from PATH (manual)
# 1. Press Win + X, select "System"
# 2. Click "Advanced system settings"
# 3. Click "Environment Variables"
# 4. Under "User variables", select "Path" and click "Edit"
# 5. Remove the entry containing "ZeroDrop"
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

1. Go to https://github.com/jcyrus/zerodrop/releases/latest
2. Download `zerodrop-windows-amd64.exe`
3. Create directory: `New-Item -ItemType Directory -Path "$env:LOCALAPPDATA\ZeroDrop" -Force`
4. Move the file: `Move-Item zerodrop-windows-amd64.exe "$env:LOCALAPPDATA\ZeroDrop\zerodrop.exe"`
5. Add to PATH manually (see Uninstall section for PATH editor instructions)

## Usage

After installation, connect to the public relay:

```powershell
zerodrop your_username
```

For more usage information, see the main [README](../../README.md#-usage) or the [User Guide](GUIDE.md).
