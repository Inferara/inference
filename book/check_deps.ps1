# INFC DLL Dependency Check + (Optional) Install + Copy Script (UCRT64 / MinGW-w64)
#
#   Order   DLL File             MSYS2 Package (UCRT64)                      Purpose
#   1       libwinpthread-1.dll  mingw-w64-ucrt-x86_64-libwinpthread          Threading Support
#   2       libffi-8.dll         mingw-w64-ucrt-x86_64-libffi                 Language Interoperability
#   3       libgcc_s_seh-1.dll   mingw-w64-ucrt-x86_64-gcc-libs               GCC Runtime
#   4       libzstd.dll          mingw-w64-ucrt-x86_64-zstd                   Zstd Compression
#   5       zlib1.dll            mingw-w64-ucrt-x86_64-zlib                   Data Compression
#
# Behavior:
# - Checks for required DLLs in CURRENT DIRECTORY first, then in PATH.
# - If any are missing, prints what would be installed, asks for explicit approval (y/yes).
# - If approved, installs required MSYS2 UCRT64 packages and copies DLLs from C:\msys64\ucrt64\bin\ into CURRENT DIRECTORY.
# - Re-checks and prints final status.
#
# Run:
#   powershell -ExecutionPolicy Bypass -File .\deps.ps1
#
# Notes:
# - Requires MSYS2 installed in C:\msys64 (or adjust $msysRoot).
# - This script does NOT install MSYS2 itself.

$ErrorActionPreference = "Stop"

# --- Config ---
$msysRoot = "C:\msys64"
$bash     = Join-Path $msysRoot "usr\bin\bash.exe"
$ucrtBin  = Join-Path $msysRoot "ucrt64\bin"
$appDir   = (Get-Location).Path

$required = [ordered]@{
  "libwinpthread-1.dll" = "pacman -S --needed --noconfirm mingw-w64-ucrt-x86_64-libwinpthread"
  "libffi-8.dll"        = "pacman -S --needed --noconfirm mingw-w64-ucrt-x86_64-libffi"
  "libgcc_s_seh-1.dll"  = "pacman -S --needed --noconfirm mingw-w64-ucrt-x86_64-gcc-libs"
  "libzstd.dll"         = "pacman -S --needed --noconfirm mingw-w64-ucrt-x86_64-zstd"
  "zlib1.dll"           = "pacman -S --needed --noconfirm mingw-w64-ucrt-x86_64-zlib"
}

function Find-Dll {
  param([Parameter(Mandatory=$true)][string]$DllName)

  $localPath = Join-Path $appDir $DllName
  if (Test-Path $localPath) {
    return @{ Found=$true; Where="current directory"; Path=$localPath }
  }

  foreach ($dir in ($env:Path -split ';')) {
    if (-not $dir) { continue }
    $candidate = Join-Path $dir $DllName
    if (Test-Path $candidate -ErrorAction SilentlyContinue) {
      return @{ Found=$true; Where="PATH"; Path=$candidate }
    }
  }

  return @{ Found=$false; Where=""; Path="" }
}

function Check-Dependencies {
  $missing = @()
  Write-Host "`n--- INFC Dependency Check Starting ---" -ForegroundColor Cyan
  Write-Host "Directory: $appDir" -ForegroundColor DarkCyan

  foreach ($dll in $required.Keys) {
    $r = Find-Dll -DllName $dll
    if ($r.Found) {
      if ($r.Where -eq "current directory") {
        Write-Host "[FOUND]    $dll (in current directory)" -ForegroundColor Green
      } else {
        Write-Host "[FOUND]    $dll (in PATH: $($r.Path))" -ForegroundColor Yellow
      }
    } else {
      Write-Host "[MISSING]  $dll (not in current directory or PATH)" -ForegroundColor Red
      $missing += $dll
    }
  }

  Write-Host "---------------------------------" -ForegroundColor Cyan
  return $missing
}

function Ensure-Msys2 {
  if (-not (Test-Path $bash)) {
    Write-Host "MSYS2 bash not found at: $bash" -ForegroundColor Red
    Write-Host "Install MSYS2 (UCRT64) first, then re-run this script." -ForegroundColor Yellow
    return $false
  }
  if (-not (Test-Path $ucrtBin)) {
    Write-Host "MSYS2 UCRT64 bin not found at: $ucrtBin" -ForegroundColor Red
    Write-Host "Ensure your MSYS2 installation includes the UCRT64 environment." -ForegroundColor Yellow
    return $false
  }
  return $true
}

function Install-And-Copy {
  param([string[]]$MissingDlls)

  if (-not (Ensure-Msys2)) { return $false }

  # Build a unique set of package commands needed for missing DLLs
  $cmds = @()
  foreach ($dll in $MissingDlls) {
    $cmds += $required[$dll]
  }
  $cmds = $cmds | Where-Object { $_ -and $_.Trim() -ne "" } | Select-Object -Unique

  Write-Host "`nThe following commands will be executed inside MSYS2 (UCRT64):" -ForegroundColor Cyan
  Write-Host "  pacman -Syu --noconfirm" -ForegroundColor Gray
  foreach ($c in $cmds) { Write-Host "  $c" -ForegroundColor Gray }

  $answer = Read-Host "`nInstall missing dependencies and copy DLLs into the current directory? (y/N)"
  if ($answer -notmatch '^(y|yes)$') {
    Write-Host "`nInstallation cancelled by user. No changes were made." -ForegroundColor Yellow
    return $false
  }

  Write-Host "`nUpdating MSYS2 packages (pacman -Syu)..." -ForegroundColor Cyan
  & $bash -lc "pacman -Syu --noconfirm"

  foreach ($c in $cmds) {
    Write-Host "Installing: $c" -ForegroundColor Cyan
    & $bash -lc $c
  }

  Write-Host "`nCopying DLLs into current directory..." -ForegroundColor Cyan
  foreach ($dll in $MissingDlls) {
    $src = Join-Path $ucrtBin $dll
    $dst = Join-Path $appDir $dll

    if (-not (Test-Path $src)) {
      Write-Host "[ERROR] Expected DLL not found in MSYS2 UCRT64 bin: $src" -ForegroundColor Red
      return $false
    }

    Copy-Item $src -Destination $dst -Force
    Write-Host "[COPIED]   $dll -> $dst" -ForegroundColor Green
  }

  return $true
}

# --- Main ---
$missing = Check-Dependencies

if ($missing.Count -eq 0) {
  Write-Host "SUCCESS: All identified dependencies are present." -ForegroundColor Green
  Write-Host "Ready to run Inference" -ForegroundColor Yellow
} else {
  Write-Host "FAILURE: $($missing.Count) file(s) are missing." -ForegroundColor Red

  $didInstall = Install-And-Copy -MissingDlls $missing

  if ($didInstall) {
    # Re-check after install/copy
    $missingAfter = Check-Dependencies
    if ($missingAfter.Count -eq 0) {
      Write-Host "SUCCESS: All dependencies are now present (after install/copy)." -ForegroundColor Green
      Write-Host "Ready to run Inference" -ForegroundColor Yellow
    } else {
      Write-Host "FAILURE: Still missing $($missingAfter.Count) file(s): $($missingAfter -join ', ')" -ForegroundColor Red
      Write-Host "You may have a different toolchain flavor mismatch or additional unseen dependencies." -ForegroundColor Yellow
    }
  } else {
    Write-Host "No installation performed. Please install missing dependencies or place the DLLs next to the .exe." -ForegroundColor Yellow
  }
}

Write-Host "`nExecution finished. Press Enter to close this window..." -ForegroundColor White
Read-Host | Out-Null
