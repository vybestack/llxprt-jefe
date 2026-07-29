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
$BinaryName = "$AppName.exe"
$ChecksumName = "$BinaryName.sha256"
$BackupRetentionDays = 7

if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs' $AppName
}
if (-not $SourceDir) {
    $SourceDir = $PSScriptRoot
}

$InstallDir = [IO.Path]::GetFullPath($InstallDir).TrimEnd([IO.Path]::DirectorySeparatorChar)
$SourceDir = [IO.Path]::GetFullPath($SourceDir).TrimEnd([IO.Path]::DirectorySeparatorChar)

function Assert-SafeInstallDir {
    $root = [IO.Path]::GetPathRoot($InstallDir)
    if (-not $root -or $InstallDir -eq $root.TrimEnd([IO.Path]::DirectorySeparatorChar)) {
        throw "InstallDir must not be a drive root: $InstallDir"
    }

    $protectedRoots = @(
        $env:SystemRoot,
        $env:ProgramFiles,
        ${env:ProgramFiles(x86)},
        $env:ProgramData
    ) | Where-Object { $_ } | ForEach-Object {
        [IO.Path]::GetFullPath($_).TrimEnd(
            [IO.Path]::DirectorySeparatorChar,
            [IO.Path]::AltDirectorySeparatorChar
        )
    }
    foreach ($protectedRoot in $protectedRoots) {
        if ($InstallDir -eq $protectedRoot -or $InstallDir.StartsWith("$protectedRoot\", [StringComparison]::OrdinalIgnoreCase)) {
            throw "InstallDir must not be a protected system directory: $InstallDir"
        }
    }

    foreach ($personalRoot in @($env:USERPROFILE, $env:LOCALAPPDATA, $env:APPDATA)) {
        if ($personalRoot -and $InstallDir -eq ([IO.Path]::GetFullPath($personalRoot).TrimEnd([IO.Path]::DirectorySeparatorChar))) {
            throw "InstallDir must be a package-owned child directory: $InstallDir"
        }
    }
}

Assert-SafeInstallDir

function Write-Step([string]$Message) {
    Write-Host "[jefe] $Message" -ForegroundColor Cyan
}

function Write-OK([string]$Message) {
    Write-Host "[jefe] $Message" -ForegroundColor Green
}

function Assert-PackageSource {
    foreach ($name in @($BinaryName, 'LICENSE')) {
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

function Get-JefeUserPath {
    return [Environment]::GetEnvironmentVariable('Path', 'User')
}

function Set-JefeUserPath([string]$Value) {
    [Environment]::SetEnvironmentVariable('Path', $Value, 'User')
}

function Test-PathValueContainsEntry([string]$UserPath, [string]$Entry) {
    if (-not $UserPath) {
        return $false
    }
    $normalized = Normalize-PathEntry $Entry
    foreach ($existing in @($UserPath -split ';')) {
        if ((Normalize-PathEntry $existing) -eq $normalized) {
            return $true
        }
    }
    return $false
}

function Add-JefeUserPath {
    $userPath = Get-JefeUserPath
    if (Test-PathValueContainsEntry $userPath $InstallDir) {
        return $false
    }
    $separator = if ($userPath -and -not $userPath.EndsWith(';')) { ';' } else { '' }
    $newUserPath = "$userPath$separator$InstallDir"
    if ($newUserPath.Length -ge 32000) {
        throw "adding Jefe would make the user PATH exceed the safe Windows environment-variable limit"
    }
    Set-JefeUserPath $newUserPath
    Write-Step "added $InstallDir to user PATH"
    return $true
}

function Remove-JefeUserPath {
    $userPath = Get-JefeUserPath
    if (-not $userPath) {
        return $false
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
        Set-JefeUserPath ($entries -join ';')
        Write-Step "removed $InstallDir from user PATH"
    }
    return $removed
}

function Write-OwnerMetadata([string]$Directory, [bool]$PathAdded) {
    $metadata = [ordered]@{
        app = $AppName
        installed = (Get-Date).ToUniversalTime().ToString('o')
        pathAdded = $PathAdded
    }
    $metadata | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Directory $OwnerMarker) -Encoding UTF8
}

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $hasher = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
    } finally {
        $hasher.Dispose()
        $stream.Dispose()
    }
}

function Assert-StagedBinaryChecksum([string]$StageDir) {
    $checksumPath = Join-Path $StageDir $ChecksumName
    if (-not (Test-Path -LiteralPath $checksumPath -PathType Leaf)) {
        return
    }

    $checksumText = (Get-Content -LiteralPath $checksumPath -Raw).Trim()
    $match = [regex]::Match($checksumText, '^([0-9a-fA-F]{64})(?:\s+\*?(\S+))?$')
    if (-not $match.Success) {
        throw "invalid checksum in $ChecksumName; expected a SHA256 digest with an optional $BinaryName filename"
    }
    if ($match.Groups[2].Success -and ([IO.Path]::GetFileName($match.Groups[2].Value) -ne $BinaryName)) {
        throw "invalid checksum in $ChecksumName; expected filename $BinaryName"
    }

    $expected = $match.Groups[1].Value.ToLowerInvariant()
    $actual = Get-Sha256 (Join-Path $StageDir $BinaryName)
    if ($actual -ne $expected) {
        throw "$BinaryName checksum mismatch: expected $expected, got $actual"
    }
}

function New-StagedInstall([string]$StageDir, [bool]$PathAdded) {
    New-Item -ItemType Directory -Path $StageDir | Out-Null
    Copy-Item -LiteralPath (Join-Path $SourceDir $BinaryName) -Destination (Join-Path $StageDir $BinaryName)
    Copy-Item -LiteralPath (Join-Path $SourceDir 'LICENSE') -Destination (Join-Path $StageDir 'LICENSE')
    $sourceChecksum = Join-Path $SourceDir $ChecksumName
    if (Test-Path -LiteralPath $sourceChecksum -PathType Leaf) {
        Copy-Item -LiteralPath $sourceChecksum -Destination (Join-Path $StageDir $ChecksumName)
    }
    Write-OwnerMetadata $StageDir $PathAdded

    Assert-StagedBinaryChecksum $StageDir
    $version = & (Join-Path $StageDir $BinaryName) --version 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "staged $BinaryName failed --version with exit $LASTEXITCODE`: $version"
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
    $pathAdded = $hadExisting -and [bool]$existingMetadata.pathAdded
    $pathChanged = $false
    $published = $false

    try {
        $version = New-StagedInstall $stageDir $pathAdded
        if ($hadExisting) {
            Move-Item -LiteralPath $InstallDir -Destination $backupDir
        }
        Move-Item -LiteralPath $stageDir -Destination $InstallDir
        $published = $true

        if (-not $hadExisting -or $pathAdded) {
            $pathChanged = Add-JefeUserPath
            if (-not $hadExisting) {
                $pathAdded = $pathChanged
            }
            Write-OwnerMetadata $InstallDir $pathAdded
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

    $restorePath = $false
    if ([bool]$metadata.pathAdded) {
        $restorePath = Remove-JefeUserPath
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

function Remove-StaleJefeBackups {
    $parent = Split-Path -Parent $InstallDir
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
        return
    }
    $prefix = (Split-Path -Leaf $InstallDir) + '.backup-'
    $cutoff = (Get-Date).ToUniversalTime().AddDays(-$BackupRetentionDays)
    try {
        $candidates = @(Get-ChildItem -LiteralPath $parent -Directory -Force)
    } catch {
        Write-Warning "could not inspect stale Jefe backups beside $InstallDir`: $_"
        return
    }

    foreach ($candidate in $candidates) {
        if (-not $candidate.Name.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
            continue
        }
        if ($candidate.LastWriteTimeUtc -gt $cutoff) {
            continue
        }
        try {
            $metadata = Read-OwnerMetadata $candidate.FullName
        } catch {
            Write-Warning "preserving stale backup with invalid ownership metadata at $($candidate.FullName): $_"
            continue
        }
        if ($null -eq $metadata) {
            continue
        }
        try {
            Remove-Item -LiteralPath $candidate.FullName -Recurse -Force -ErrorAction Stop
            Write-Step "removed stale backup $($candidate.FullName)"
        } catch {
            Write-Warning "could not remove stale Jefe backup at $($candidate.FullName): $_"
        }
    }
}

function Invoke-WithInstallLock([scriptblock]$Action) {
    # Serialize concurrent installs targeting the same InstallDir. Two
    # simultaneous executions could otherwise both stage installations and
    # race on the final move into InstallDir, corrupting the install or
    # losing a backup. The mutex name is derived from the normalized path so
    # distinct install directories are independent.
    $hasher = [Security.Cryptography.SHA256]::Create()
    try {
        $pathBytes = [Text.Encoding]::UTF8.GetBytes($InstallDir.ToUpperInvariant())
        $pathHash = [BitConverter]::ToString($hasher.ComputeHash($pathBytes)).Replace('-', '').ToLowerInvariant()
    } finally {
        $hasher.Dispose()
    }
    $mutexName = 'Local\jefe-install-' + $pathHash
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
        Remove-StaleJefeBackups
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
