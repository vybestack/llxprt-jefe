<#
.SYNOPSIS
    Installs, upgrades, or uninstalls Jefe for the current Windows user.

.DESCRIPTION
    Publishes package-owned files under LOCALAPPDATA by default, records
    ownership before recursive removal, and changes only the current user's
    PATH. Configuration, state, and psmux sessions are outside InstallDir and
    are never removed.

.PARAMETER Action
    Install, Upgrade, or Uninstall.

.PARAMETER SourceDir
    Directory containing jefe.exe and LICENSE. Defaults to this script's
    directory.

.PARAMETER InstallDir
    Package-owned target directory. Defaults to
    "$env:LOCALAPPDATA\Programs\jefe".

.NOTES
    Install psmux separately with:
      winget install --id marlocarlo.psmux --exact
#>

[CmdletBinding()]
param(
    [ValidateSet('Install', 'Upgrade', 'Uninstall')]
    [string]$Action = 'Install',

    [string]$SourceDir,

    [string]$InstallDir
)

$ErrorActionPreference = 'Stop'
$OwnerMarker = '.jefe-installed'
$AppName = 'jefe'

if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs' $AppName
}
if (-not $SourceDir) {
    $SourceDir = $PSScriptRoot
}

$InstallDir = [IO.Path]::GetFullPath($InstallDir).TrimEnd([IO.Path]::DirectorySeparatorChar)
$SourceDir = [IO.Path]::GetFullPath($SourceDir).TrimEnd([IO.Path]::DirectorySeparatorChar)

function Write-Step([string]$Message) {
    Write-Host "[jefe] $Message" -ForegroundColor Cyan
}

function Write-OK([string]$Message) {
    Write-Host "[jefe] $Message" -ForegroundColor Green
}

function Assert-PackageSource {
    foreach ($name in @('jefe.exe', 'LICENSE')) {
        $path = Join-Path $SourceDir $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "$name not found in SourceDir: $SourceDir"
        }
    }
}

function Read-OwnerMetadata([string]$Directory) {
    $marker = Join-Path $Directory $OwnerMarker
    if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
        return $null
    }
    try {
        $metadata = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
    } catch {
        throw "invalid Jefe ownership marker at $marker"
    }
    if ($metadata.app -ne $AppName) {
        throw "invalid Jefe ownership marker at $marker"
    }
    return $metadata
}

function Normalize-PathEntry([string]$Entry) {
    # PATH entries from the registry may carry a trailing directory separator
    # that $InstallDir is normalized to remove. Trim separators from both
    # sides of every comparison so an entry like `C:\...\jefe` matches
    # `C:\...\jefe` and uninstall does not leave a stale PATH reference.
    return $Entry.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
}

function Test-UserPathEntry([string]$Entry) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not $userPath) {
        return $false
    }
    $normalized = Normalize-PathEntry $Entry
    foreach ($existing in @($userPath -split ';')) {
        if ((Normalize-PathEntry $existing) -eq $normalized) {
            return $true
        }
    }
    return $false
}

function Add-JefeUserPath {
    if (Test-UserPathEntry $InstallDir) {
        return $false
    }
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $separator = if ($userPath -and -not $userPath.EndsWith(';')) { ';' } else { '' }
    [Environment]::SetEnvironmentVariable('Path', "$userPath$separator$InstallDir", 'User')
    Write-Step "added $InstallDir to user PATH"
    return $true
}

function Remove-JefeUserPath {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not $userPath) {
        return
    }
    $normalized = Normalize-PathEntry $InstallDir
    $entries = [Collections.Generic.List[string]]::new()
    $removed = $false
    foreach ($entry in @($userPath -split ';')) {
        if ((Normalize-PathEntry $entry) -eq $normalized) {
            $removed = $true
            continue
        }
        $entries.Add($entry)
    }
    if ($removed) {
        [Environment]::SetEnvironmentVariable('Path', ($entries -join ';'), 'User')
        Write-Step "removed $InstallDir from user PATH"
    }
}

function Write-OwnerMetadata([string]$Directory, [bool]$PathAdded) {
    $metadata = [ordered]@{
        app = $AppName
        installed = (Get-Date).ToUniversalTime().ToString('o')
        pathAdded = $PathAdded
    }
    $metadata | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Directory $OwnerMarker) -Encoding UTF8
}

function New-StagedInstall([string]$StageDir, [bool]$PathAdded) {
    New-Item -ItemType Directory -Path $StageDir | Out-Null
    Copy-Item -LiteralPath (Join-Path $SourceDir 'jefe.exe') -Destination (Join-Path $StageDir 'jefe.exe')
    Copy-Item -LiteralPath (Join-Path $SourceDir 'LICENSE') -Destination (Join-Path $StageDir 'LICENSE')
    Write-OwnerMetadata $StageDir $PathAdded

    $version = & (Join-Path $StageDir 'jefe.exe') --version 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "staged jefe.exe failed --version with exit $LASTEXITCODE`: $version"
    }
    return ($version | Out-String).Trim()
}

function Publish-Jefe([bool]$RequireExisting) {
    Assert-PackageSource

    $existingMetadata = $null
    if (Test-Path -LiteralPath $InstallDir) {
        $existingMetadata = Read-OwnerMetadata $InstallDir
        if ($null -eq $existingMetadata) {
            throw "InstallDir exists without a valid Jefe ownership marker: $InstallDir"
        }
    } elseif ($RequireExisting) {
        throw "no existing Jefe install found at $InstallDir; run -Action Install first"
    }

    $parent = Split-Path -Parent $InstallDir
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    $suffix = "$PID-$([Guid]::NewGuid().ToString('N'))"
    $stageDir = "$InstallDir.stage-$suffix"
    $backupDir = "$InstallDir.backup-$suffix"
    $hadExisting = $null -ne $existingMetadata
    $pathAdded = if ($hadExisting) { [bool]$existingMetadata.pathAdded } else { -not (Test-UserPathEntry $InstallDir) }
    $pathChanged = $false
    $published = $false

    try {
        $version = New-StagedInstall $stageDir $pathAdded
        if ($hadExisting) {
            Move-Item -LiteralPath $InstallDir -Destination $backupDir
        }
        Move-Item -LiteralPath $stageDir -Destination $InstallDir
        $published = $true

        if ($pathAdded -and -not (Test-UserPathEntry $InstallDir)) {
            $pathChanged = Add-JefeUserPath
        }
        if ($hadExisting) {
            Remove-Item -LiteralPath $backupDir -Recurse -Force
        }
        return $version
    } catch {
        $failure = $_
        if ($pathChanged) {
            Remove-JefeUserPath
        }
        if ($published -and (Test-Path -LiteralPath $InstallDir)) {
            Remove-Item -LiteralPath $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
        }
        $restoreFailure = $null
        if (Test-Path -LiteralPath $backupDir) {
            try {
                Move-Item -LiteralPath $backupDir -Destination $InstallDir -ErrorAction Stop
            } catch {
                $restoreFailure = $_
            }
        }
        if (Test-Path -LiteralPath $stageDir) {
            Remove-Item -LiteralPath $stageDir -Recurse -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $restoreFailure) {
            throw "install failed ($failure) and restoring the previous install also failed ($restoreFailure); backup remains at $backupDir"
        }
        throw $failure
    }
}

function Install-Jefe {
    Write-Step "installing Jefe to $InstallDir"
    $version = Publish-Jefe $false
    Write-OK "installed: $version"
}

function Upgrade-Jefe {
    Write-Step "upgrading Jefe in $InstallDir"
    $version = Publish-Jefe $true
    Write-OK "upgraded: $version"
}

function Uninstall-Jefe {
    Write-Step "uninstalling Jefe from $InstallDir"
    if (-not (Test-Path -LiteralPath $InstallDir)) {
        Write-Step "no install found; nothing to do"
        return
    }
    $metadata = Read-OwnerMetadata $InstallDir
    if ($null -eq $metadata) {
        throw "InstallDir exists without a valid Jefe ownership marker: $InstallDir"
    }

    $restorePath = [bool]$metadata.pathAdded -and (Test-UserPathEntry $InstallDir)
    if ($restorePath) {
        Remove-JefeUserPath
    }
    try {
        Remove-Item -LiteralPath $InstallDir -Recurse -Force
    } catch {
        if ($restorePath) {
            [void](Add-JefeUserPath)
        }
        throw
    }
    Write-OK "removed package-owned files; configuration, state, and psmux sessions were preserved"
}

function Invoke-WithInstallLock([scriptblock]$Action) {
    # Serialize concurrent installs targeting the same InstallDir. Two
    # simultaneous executions could otherwise both stage installations and
    # race on the final move into InstallDir, corrupting the install or
    # losing a backup. The mutex name is derived from the normalized path so
    # distinct install directories are independent.
    $mutexName = 'Local\jefe-install-' + ($InstallDir -replace '[^A-Za-z0-9]', '_')
    $mutex = [System.Threading.Mutex]::new($false, $mutexName)
    try {
        $acquired = $false
        try {
            $mutex.WaitOne()
            $acquired = $true
        } catch [System.Threading.AbandonedMutexException] {
            # A previous process exited while holding the mutex; we still
            # acquired it on the abandoned exception.
            $acquired = $true
        }
        & $Action
    } finally {
        if ($acquired) {
            $mutex.ReleaseMutex()
        }
        $mutex.Dispose()
    }
}

switch ($Action) {
    'Install' { Invoke-WithInstallLock { Install-Jefe } }
    'Upgrade' { Invoke-WithInstallLock { Upgrade-Jefe } }
    'Uninstall' { Invoke-WithInstallLock { Uninstall-Jefe } }
}
