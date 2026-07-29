$script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$script:InstallerPath = Join-Path $script:RepoRoot 'scripts\jefe-install.ps1'
$script:InstallerText = [IO.File]::ReadAllText($script:InstallerPath)
$script:TestRoot = Join-Path $env:TEMP ("jefe-installer-pester-$PID-$([Guid]::NewGuid().ToString('N'))")
$script:FixtureExe = Join-Path $script:TestRoot 'fixture-jefe.exe'
New-Item -ItemType Directory -Force -Path $script:TestRoot | Out-Null

$fixtureSource = @'
using System;
using System.IO;
public static class FixtureJefe {
    public static int Main(string[] args) {
        if (args.Length != 1 || args[0] != "--version") { return 2; }
        var marker = Environment.GetEnvironmentVariable("JEFE_TEST_LAUNCH_MARKER");
        if (!String.IsNullOrEmpty(marker)) { File.WriteAllText(marker, "launched"); }
        Console.WriteLine("jefe 0.0.0-test");
        return 0;
    }
}
'@
Add-Type -TypeDefinition $fixtureSource -OutputAssembly $script:FixtureExe -OutputType ConsoleApplication

$tokens = $null
$parseErrors = $null
$installerAst = [Management.Automation.Language.Parser]::ParseFile(
    $script:InstallerPath,
    [ref]$tokens,
    [ref]$parseErrors
)
if ($parseErrors.Count -ne 0) {
    throw "installer parse failed: $($parseErrors[0].Message)"
}
$functionAsts = $installerAst.FindAll({
    param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst]
}, $true)
foreach ($functionAst in $functionAsts) {
    . ([scriptblock]::Create($functionAst.Extent.Text))
}

$script:OwnerMarker = '.jefe-installed'
$script:AppName = 'jefe'
$script:BinaryName = "$script:AppName.exe"
$script:ChecksumName = "$script:BinaryName.sha256"
$script:BackupRetentionDays = 7

function Get-InstallerFunctionText([string]$Name) {
    $match = $functionAsts | Where-Object { $_.Name -eq $Name } | Select-Object -First 1
    if ($null -eq $match) { return '' }
    return $match.Extent.Text
}

function New-TestDirectory([string]$Name) {
    $path = Join-Path $script:TestRoot ("$Name-$([Guid]::NewGuid().ToString('N'))")
    New-Item -ItemType Directory -Force -Path $path | Out-Null
    return $path
}

function New-TestPackage([string]$Directory) {
    New-Item -ItemType Directory -Force -Path $Directory | Out-Null
    Copy-Item -LiteralPath $script:FixtureExe -Destination (Join-Path $Directory 'jefe.exe')
    Set-Content -LiteralPath (Join-Path $Directory 'LICENSE') -Value 'test license' -Encoding UTF8
}

function Write-TestOwnerMarker([string]$Directory, [bool]$PathAdded) {
    New-Item -ItemType Directory -Force -Path $Directory | Out-Null
    [ordered]@{
        app = 'jefe'
        installed = '2026-07-28T00:00:00.0000000Z'
        pathAdded = $PathAdded
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Directory '.jefe-installed') -Encoding UTF8
}

function Set-InstallerDirectories([string]$Install, [string]$Source) {
    $script:InstallDir = [IO.Path]::GetFullPath($Install).TrimEnd([IO.Path]::DirectorySeparatorChar)
    $script:SourceDir = [IO.Path]::GetFullPath($Source).TrimEnd([IO.Path]::DirectorySeparatorChar)
}

function Start-InstallerChild([string]$ScriptText) {
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($ScriptText))
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = (Get-Process -Id $PID).Path
    $info.Arguments = "-NoProfile -EncodedCommand $encoded"
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.CreateNoWindow = $true
    return [Diagnostics.Process]::Start($info)
}

function New-LockChildScript([string]$Install, [string]$EnteredEvent, [string]$ReleaseEvent) {
    $escapedInstaller = $script:InstallerPath.Replace("'", "''")
    $escapedInstall = $Install.Replace("'", "''")
    $escapedEntered = $EnteredEvent.Replace("'", "''")
    $escapedRelease = $ReleaseEvent.Replace("'", "''")
    return @"
`$ErrorActionPreference = 'Stop'
`$InstallDir = '$escapedInstall'
`$tokens = `$null
`$errors = `$null
`$ast = [Management.Automation.Language.Parser]::ParseFile('$escapedInstaller', [ref]`$tokens, [ref]`$errors)
`$functions = `$ast.FindAll({ param(`$node) `$node -is [Management.Automation.Language.FunctionDefinitionAst] }, `$true)
foreach (`$function in `$functions) { . ([scriptblock]::Create(`$function.Extent.Text)) }
`$entered = [Threading.EventWaitHandle]::OpenExisting('$escapedEntered')
`$release = [Threading.EventWaitHandle]::OpenExisting('$escapedRelease')
try {
    Invoke-WithInstallLock {
        [void]`$entered.Set()
        if (-not `$release.WaitOne(10000)) { throw 'timed out waiting for release event' }
    }
} finally {
    `$entered.Dispose()
    `$release.Dispose()
}
"@
}

Describe 'Jefe installer PATH mutation' {
    BeforeEach {
        $script:FakeUserPath = ''
        $script:PathReads = 0
        $script:PathWrites = 0
        Mock Get-JefeUserPath {
            $script:PathReads++
            return $script:FakeUserPath
        }
        Mock Set-JefeUserPath {
            param([string]$Value)
            $script:PathWrites++
            $script:FakeUserPath = $Value
        }
    }

    It 'reads one snapshot and writes once when adding an absent entry' {
        (Get-InstallerFunctionText 'Get-JefeUserPath') | Should Not Be ''
        (Get-InstallerFunctionText 'Set-JefeUserPath') | Should Not Be ''
        Set-InstallerDirectories (New-TestDirectory 'path-add-install') (New-TestDirectory 'path-add-source')

        $changed = Add-JefeUserPath

        $changed | Should Be $true
        $script:PathReads | Should Be 1
        $script:PathWrites | Should Be 1
        $script:FakeUserPath | Should Be $script:InstallDir
    }

    It 'does not write when a case and trailing-separator variant is present' {
        (Get-InstallerFunctionText 'Get-JefeUserPath') | Should Not Be ''
        (Get-InstallerFunctionText 'Set-JefeUserPath') | Should Not Be ''
        Set-InstallerDirectories (New-TestDirectory 'path-existing-install') (New-TestDirectory 'path-existing-source')
        $script:FakeUserPath = $script:InstallDir.ToUpperInvariant() + '\'

        $changed = Add-JefeUserPath

        $changed | Should Be $false
        $script:PathReads | Should Be 1
        $script:PathWrites | Should Be 0
    }

    It 'removes every owned duplicate from one snapshot and one write' {
        (Get-InstallerFunctionText 'Get-JefeUserPath') | Should Not Be ''
        (Get-InstallerFunctionText 'Set-JefeUserPath') | Should Not Be ''
        Set-InstallerDirectories (New-TestDirectory 'path-remove-install') (New-TestDirectory 'path-remove-source')
        $script:FakeUserPath = "C:\keep;$($script:InstallDir)\;$($script:InstallDir.ToUpperInvariant())"

        $changed = Remove-JefeUserPath

        $changed | Should Be $true
        $script:PathReads | Should Be 1
        $script:PathWrites | Should Be 1
        $script:FakeUserPath | Should Be 'C:\keep'
    }

    It 'rejects an over-limit PATH without writing' {
        (Get-InstallerFunctionText 'Get-JefeUserPath') | Should Not Be ''
        (Get-InstallerFunctionText 'Set-JefeUserPath') | Should Not Be ''
        Set-InstallerDirectories (New-TestDirectory 'path-limit-install') (New-TestDirectory 'path-limit-source')
        $script:FakeUserPath = 'x' * 31990

        { Add-JefeUserPath } | Should Throw 'safe Windows environment-variable limit'
        $script:PathReads | Should Be 1
        $script:PathWrites | Should Be 0
    }
}

Describe 'Jefe installer concurrency' {
    It 'serializes lifecycle work for the same normalized install path' {
        $root = New-TestDirectory 'mutex'
        $install = Join-Path $root 'install'
        $id = [Guid]::NewGuid().ToString('N')
        $firstEnteredName = "Local\jefe-pester-first-$id"
        $releaseFirstName = "Local\jefe-pester-release-$id"
        $secondEnteredName = "Local\jefe-pester-second-$id"
        $releaseSecondName = "Local\jefe-pester-release-second-$id"
        $firstEntered = [Threading.EventWaitHandle]::new($false, [Threading.EventResetMode]::ManualReset, $firstEnteredName)
        $releaseFirst = [Threading.EventWaitHandle]::new($false, [Threading.EventResetMode]::ManualReset, $releaseFirstName)
        $secondEntered = [Threading.EventWaitHandle]::new($false, [Threading.EventResetMode]::ManualReset, $secondEnteredName)
        $releaseSecond = [Threading.EventWaitHandle]::new($false, [Threading.EventResetMode]::ManualReset, $releaseSecondName)
        $first = $null
        $second = $null
        try {
            $first = Start-InstallerChild (New-LockChildScript $install $firstEnteredName $releaseFirstName)
            $firstEntered.WaitOne(5000) | Should Be $true
            $second = Start-InstallerChild (New-LockChildScript $install $secondEnteredName $releaseSecondName)
            $secondEntered.WaitOne(500) | Should Be $false
            [void]$releaseFirst.Set()
            $secondEntered.WaitOne(5000) | Should Be $true
            [void]$releaseSecond.Set()
            $first.WaitForExit(5000) | Should Be $true
            $second.WaitForExit(5000) | Should Be $true
            $first.ExitCode | Should Be 0
            $second.ExitCode | Should Be 0
        } finally {
            [void]$releaseFirst.Set()
            [void]$releaseSecond.Set()
            foreach ($process in @($first, $second)) {
                if ($null -ne $process -and -not $process.HasExited) { $process.Kill() }
                if ($null -ne $process) { $process.Dispose() }
            }
            $firstEntered.Dispose()
            $releaseFirst.Dispose()
            $secondEntered.Dispose()
            $releaseSecond.Dispose()
        }
    }
}

Describe 'Jefe installer rollback and uninstall' {
    It 'restores the prior owned install when publishing the stage fails' {
        $root = New-TestDirectory 'rollback'
        $source = Join-Path $root 'source'
        $install = Join-Path $root 'install'
        New-TestPackage $source
        Write-TestOwnerMarker $install $false
        Set-Content -LiteralPath (Join-Path $install 'sentinel.txt') -Value 'previous install' -Encoding UTF8
        Set-InstallerDirectories $install $source
        $script:MoveCalls = 0
        Mock Move-Item {
            param($LiteralPath, $Destination)
            $script:MoveCalls++
            if ($script:MoveCalls -eq 2) { throw 'forced publish move failure' }
            Microsoft.PowerShell.Management\Move-Item -LiteralPath $LiteralPath -Destination $Destination
        }

        { Publish-Jefe $true } | Should Throw 'forced publish move failure'
        (Get-Content -LiteralPath (Join-Path $install 'sentinel.txt') -Raw).Trim() | Should Be 'previous install'
        @(Get-ChildItem -LiteralPath $root -Directory -Filter 'install.stage-*').Count | Should Be 0
    }

    It 'removes owned files and only the metadata-owned PATH entry on uninstall' {
        (Get-InstallerFunctionText 'Get-JefeUserPath') | Should Not Be ''
        (Get-InstallerFunctionText 'Set-JefeUserPath') | Should Not Be ''
        $root = New-TestDirectory 'uninstall'
        $source = Join-Path $root 'source'
        $install = Join-Path $root 'install'
        New-TestPackage $source
        Write-TestOwnerMarker $install $true
        Set-InstallerDirectories $install $source
        $script:FakeUserPath = "C:\keep;$($script:InstallDir)"
        Mock Get-JefeUserPath { return $script:FakeUserPath }
        Mock Set-JefeUserPath { param([string]$Value) $script:FakeUserPath = $Value }

        Uninstall-Jefe

        (Test-Path -LiteralPath $install) | Should Be $false
        $script:FakeUserPath | Should Be 'C:\keep'
    }

    It 'leaves PATH unchanged when uninstall metadata does not own the entry' {
        (Get-InstallerFunctionText 'Get-JefeUserPath') | Should Not Be ''
        (Get-InstallerFunctionText 'Set-JefeUserPath') | Should Not Be ''
        $root = New-TestDirectory 'uninstall-unowned-path'
        $source = Join-Path $root 'source'
        $install = Join-Path $root 'install'
        New-TestPackage $source
        Write-TestOwnerMarker $install $false
        Set-InstallerDirectories $install $source
        $script:FakeUserPath = "C:\keep;$($script:InstallDir)"
        $script:PathWrites = 0
        Mock Get-JefeUserPath { return $script:FakeUserPath }
        Mock Set-JefeUserPath { param([string]$Value) $script:PathWrites++; $script:FakeUserPath = $Value }

        Uninstall-Jefe

        $script:PathWrites | Should Be 0
        $script:FakeUserPath | Should Be "C:\keep;$($script:InstallDir)"
    }
}

Describe 'Jefe stale backup cleanup' {
    It 'removes stale owned backups while preserving fresh and unowned siblings' {
        (Get-InstallerFunctionText 'Remove-StaleJefeBackups') | Should Not Be ''
        $root = New-TestDirectory 'backups'
        $install = Join-Path $root 'install'
        Set-InstallerDirectories $install (Join-Path $root 'source')
        $stale = "$install.backup-stale"
        $fresh = "$install.backup-fresh"
        $unowned = "$install.backup-unowned"
        $malformed = "$install.backup-malformed"
        Write-TestOwnerMarker $stale $false
        Write-TestOwnerMarker $fresh $false
        New-Item -ItemType Directory -Force -Path $unowned | Out-Null
        New-Item -ItemType Directory -Force -Path $malformed | Out-Null
        Set-Content -LiteralPath (Join-Path $malformed '.jefe-installed') -Value 'not-json' -Encoding UTF8
        (Get-Item -LiteralPath $stale).LastWriteTimeUtc = (Get-Date).ToUniversalTime().AddDays(-8)
        (Get-Item -LiteralPath $fresh).LastWriteTimeUtc = (Get-Date).ToUniversalTime().AddDays(-6)
        (Get-Item -LiteralPath $unowned).LastWriteTimeUtc = (Get-Date).ToUniversalTime().AddDays(-30)
        (Get-Item -LiteralPath $malformed).LastWriteTimeUtc = (Get-Date).ToUniversalTime().AddDays(-30)

        Remove-StaleJefeBackups -WarningAction SilentlyContinue

        (Test-Path -LiteralPath $stale) | Should Be $false
        (Test-Path -LiteralPath $fresh) | Should Be $true
        (Test-Path -LiteralPath $unowned) | Should Be $true
        (Test-Path -LiteralPath $malformed) | Should Be $true
    }

    It 'warns and continues when a stale owned backup cannot be removed' {
        $root = New-TestDirectory 'backup-removal-failure'
        $install = Join-Path $root 'install'
        Set-InstallerDirectories $install (Join-Path $root 'source')
        $stale = "$install.backup-locked"
        Write-TestOwnerMarker $stale $false
        (Get-Item -LiteralPath $stale).LastWriteTimeUtc = (Get-Date).ToUniversalTime().AddDays(-8)
        Mock Remove-Item { throw 'locked backup' }
        $warningText = & { Remove-StaleJefeBackups } 3>&1 | Out-String

        (Test-Path -LiteralPath $stale) | Should Be $true
        $warningText | Should Match 'could not remove stale Jefe backup'
    }

    It 'runs stale cleanup only after acquiring the existing install mutex' {
        $lock = Get-InstallerFunctionText 'Invoke-WithInstallLock'
        $lock | Should Match 'Remove-StaleJefeBackups'
        ($lock.IndexOf('WaitOne') -lt $lock.IndexOf('Remove-StaleJefeBackups')) | Should Be $true
    }
}

Describe 'Jefe staged checksum verification' {
    BeforeEach {
        $script:ChecksumRoot = New-TestDirectory 'checksum'
        $script:ChecksumSource = Join-Path $script:ChecksumRoot 'source'
        $script:ChecksumInstall = Join-Path $script:ChecksumRoot 'install'
        $script:ChecksumStage = Join-Path $script:ChecksumRoot 'stage'
        $script:LaunchMarker = Join-Path $script:ChecksumRoot 'launched.txt'
        New-TestPackage $script:ChecksumSource
        Set-InstallerDirectories $script:ChecksumInstall $script:ChecksumSource
        $env:JEFE_TEST_LAUNCH_MARKER = $script:LaunchMarker
    }

    AfterEach {
        Remove-Item env:JEFE_TEST_LAUNCH_MARKER -ErrorAction SilentlyContinue
    }

    It 'keeps compatibility when the package has no checksum' {
        $version = New-StagedInstall $script:ChecksumStage $false

        $version | Should Match 'jefe 0.0.0-test'
        (Test-Path -LiteralPath $script:LaunchMarker) | Should Be $true
    }

    It 'accepts a matching bare checksum' {
        $hash = Get-Sha256 (Join-Path $script:ChecksumSource 'jefe.exe')
        $hash | Set-Content -LiteralPath (Join-Path $script:ChecksumSource 'jefe.exe.sha256') -Encoding ASCII

        $version = New-StagedInstall $script:ChecksumStage $false

        $version | Should Match 'jefe 0.0.0-test'
    }

    It 'accepts a matching conventional checksum' {
        $hash = Get-Sha256 (Join-Path $script:ChecksumSource 'jefe.exe')
        "$hash  jefe.exe" | Set-Content -LiteralPath (Join-Path $script:ChecksumSource 'jefe.exe.sha256') -Encoding ASCII

        $version = New-StagedInstall $script:ChecksumStage $false

        $version | Should Match 'jefe 0.0.0-test'
        (Test-Path -LiteralPath (Join-Path $script:ChecksumStage 'jefe.exe.sha256')) | Should Be $true
    }

    It 'rejects a checksum mismatch before launching the staged executable' {
        (('0' * 64) + '  jefe.exe') | Set-Content -LiteralPath (Join-Path $script:ChecksumSource 'jefe.exe.sha256') -Encoding ASCII

        { New-StagedInstall $script:ChecksumStage $false } | Should Throw 'checksum mismatch'
        (Test-Path -LiteralPath $script:LaunchMarker) | Should Be $false
    }

    It 'rejects a malformed checksum before launching the staged executable' {
        'not-a-sha256' | Set-Content -LiteralPath (Join-Path $script:ChecksumSource 'jefe.exe.sha256') -Encoding ASCII

        { New-StagedInstall $script:ChecksumStage $false } | Should Throw 'invalid checksum'
        (Test-Path -LiteralPath $script:LaunchMarker) | Should Be $false
    }
}

Describe 'Jefe binary name derivation' {
    It 'derives one binary name from AppName and reuses it for source validation' {
        $script:InstallerText | Should Match '\$BinaryName\s*=\s*"\$AppName\.exe"'
        $assertSource = Get-InstallerFunctionText 'Assert-PackageSource'
        $assertSource | Should Match '\$BinaryName'
        $assertSource | Should Not Match "'jefe\.exe'"
    }
}
