# SPDX-License-Identifier: AGPL-3.0-only
# Zero-friction Windows installer for BuildLang (buildc, qdb, port).
# Verifies SHA256SUMS fail-closed before extracting any archive.
#
# Quick install (PowerShell):
#   irm https://raw.githubusercontent.com/HarperZ9/buildlang/main/install.ps1 | iex
#
# Local verification (zero network):
#   powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 -SelfTest
#   powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 -FromDir .\dist -InstallDir .\out -NoPath

$ErrorActionPreference = "Stop"

$Version = $env:BUILDLANG_VERSION
$InstallDir = $env:BUILDLANG_INSTALL_DIR
$Repo = if ($env:BUILDLANG_REPO) { $env:BUILDLANG_REPO } else { "HarperZ9/buildlang" }
$FromDir = ""
$NoPath = $false
$SelfTest = $false
$ShowHelp = $false

$idx = 0
while ($idx -lt $args.Count) {
    $a = [string]$args[$idx]
    switch -Regex ($a) {
        '^(-Version|--version)$' {
            $idx++
            if ($idx -ge $args.Count) { throw "Missing value for $a" }
            $Version = [string]$args[$idx]
        }
        '^(-InstallDir|--install-dir)$' {
            $idx++
            if ($idx -ge $args.Count) { throw "Missing value for $a" }
            $InstallDir = [string]$args[$idx]
        }
        '^(-FromDir|--from-dir)$' {
            $idx++
            if ($idx -ge $args.Count) { throw "Missing value for $a" }
            $FromDir = [string]$args[$idx]
        }
        '^(-NoPath|--no-path)$' { $NoPath = $true }
        '^(-SelfTest|--self-test)$' { $SelfTest = $true }
        '^(-Help|--help|-h)$' { $ShowHelp = $true }
        default { throw "Unknown option: $a" }
    }
    $idx++
}

if ($ShowHelp) {
    Write-Host "Usage: install.ps1 [-Version v1.15.0] [-InstallDir <dir>] [-FromDir <dir>] [-NoPath] [-SelfTest]"
    exit 0
}

function Resolve-BuildLangVersion {
    param([string]$RepoSlug, [string]$Requested)
    if ($Requested -and $Requested.Trim().Length -gt 0) {
        $v = $Requested.Trim()
        if (-not $v.StartsWith("v")) { $v = "v$v" }
        return $v
    }
    $latestUrl = "https://github.com/$RepoSlug/releases/latest"
    $req = [System.Net.HttpWebRequest]::Create($latestUrl)
    $req.AllowAutoRedirect = $false
    $req.UserAgent = "buildlang-installer/1.15"
    try {
        $resp = $req.GetResponse()
        $loc = $resp.Headers["Location"]
        $resp.Close()
    } catch [System.Net.WebException] {
        $resp = $_.Exception.Response
        if ($null -eq $resp) { throw "Failed to resolve latest release from $latestUrl" }
        $loc = $resp.Headers["Location"]
        $resp.Close()
    }
    if (-not $loc -or $loc -notmatch '/tag/([^/?#]+)$') {
        throw "Could not resolve latest tag from $latestUrl (Location: $loc)"
    }
    return $Matches[1]
}

function Get-ExpectedSha256 {
    param([string]$SumsPath, [string]$AssetName)
    if (-not (Test-Path -LiteralPath $SumsPath)) {
        throw "Checksum file not found: $SumsPath"
    }
    foreach ($line in (Get-Content -LiteralPath $SumsPath)) {
        $trimmed = $line.Trim()
        if (-not $trimmed -or $trimmed.StartsWith("#")) { continue }
        if ($trimmed -match '^([0-9a-fA-F]{64})\s+\*?(.+)$') {
            $hash = $Matches[1].ToLowerInvariant()
            $name = $Matches[2].Trim()
            if ($name -eq $AssetName) {
                return $hash
            }
        }
    }
    throw "Fail-closed: $AssetName not listed in $SumsPath"
}

function Assert-ArchiveSha256 {
    param([string]$ArchivePath, [string]$SumsPath, [string]$AssetName)
    $expected = Get-ExpectedSha256 -SumsPath $SumsPath -AssetName $AssetName
    $actual = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "Fail-closed SHA-256 mismatch for ${AssetName}: expected $expected, got $actual"
    }
    Write-Host "SHA-256 verified: $AssetName ($actual)"
}

function Add-ToUserPath {
    param([string]$Dir)
    $current = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @()
    if ($current) { $parts = $current -split ';' | Where-Object { $_.Trim().Length -gt 0 } }
    $norm = $Dir.TrimEnd('\').ToLowerInvariant()
    foreach ($p in $parts) {
        if ($p.TrimEnd('\').ToLowerInvariant() -eq $norm) {
            return $false
        }
    }
    $updated = if ($parts.Count -gt 0) { ($parts + $Dir) -join ';' } else { $Dir }
    [Environment]::SetEnvironmentVariable("Path", $updated, "User")
    if ($env:Path -notlike "*$Dir*") {
        $env:Path = "$Dir;$env:Path"
    }
    return $true
}

function Install-BuildLang {
    param(
        [string]$RepoSlug,
        [string]$Tag,
        [string]$TargetDir,
        [string]$LocalDir,
        [bool]$SkipPath,
        [bool]$SkipVerify
    )
    $target = "x86_64-pc-windows-msvc"
    $asset = "buildlang-$Tag-$target.zip"
    $work = Join-Path ([System.IO.Path]::GetTempPath()) ("buildlang-install-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $work -Force | Out-Null
    try {
        $archivePath = Join-Path $work $asset
        $sumsPath = Join-Path $work "SHA256SUMS"
        if ($LocalDir -and $LocalDir.Length -gt 0) {
            $srcArchive = Join-Path $LocalDir $asset
            $srcSums = Join-Path $LocalDir "SHA256SUMS"
            if (-not (Test-Path -LiteralPath $srcArchive)) {
                throw "Archive not found in local dir: $srcArchive"
            }
            Copy-Item -LiteralPath $srcArchive -Destination $archivePath -Force
            Copy-Item -LiteralPath $srcSums -Destination $sumsPath -Force
        } else {
            $baseUrl = "https://github.com/$RepoSlug/releases/download/$Tag"
            Write-Host "Downloading $asset and SHA256SUMS from $RepoSlug ($Tag)..."
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            Invoke-WebRequest -Uri "$baseUrl/SHA256SUMS" -OutFile $sumsPath -UseBasicParsing
            Invoke-WebRequest -Uri "$baseUrl/$asset" -OutFile $archivePath -UseBasicParsing
        }

        Assert-ArchiveSha256 -ArchivePath $archivePath -SumsPath $sumsPath -AssetName $asset

        $extractDir = Join-Path $work "extract"
        Expand-Archive -LiteralPath $archivePath -DestinationPath $extractDir -Force

        New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
        $installed = @()
        foreach ($bin in @("buildc.exe", "qdb.exe", "port.exe")) {
            $found = Get-ChildItem -Path $extractDir -Filter $bin -Recurse -File | Select-Object -First 1
            if ($null -eq $found) {
                throw "Fail-closed: expected binary $bin missing from $asset"
            }
            Copy-Item -LiteralPath $found.FullName -Destination (Join-Path $TargetDir $bin) -Force
            $installed += $bin
        }

        if (-not $SkipPath) {
            $added = Add-ToUserPath -Dir $TargetDir
            if ($added) { Write-Host "Added $TargetDir to User PATH." }
        }

        if (-not $SkipVerify) {
            foreach ($bin in @("buildc.exe", "qdb.exe")) {
                $exe = Join-Path $TargetDir $bin
                $out = & $exe --version 2>&1
                if ($LASTEXITCODE -ne 0) {
                    throw "Verification failed for $bin (exit $LASTEXITCODE): $out"
                }
                Write-Host "Verified $bin`: $out"
            }
            $portExe = Join-Path $TargetDir "port.exe"
            if (-not (Test-Path -LiteralPath $portExe)) {
                throw "Verification failed: port.exe missing in $TargetDir"
            }
            Write-Host "Verified port.exe: installed at $portExe"
        }
        Write-Host "Installed BuildLang $Tag ($($installed -join ', ')) into $TargetDir"
    } finally {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-SelfTest {
    $root = Join-Path ([System.IO.Path]::GetTempPath()) ("buildlang-selftest-" + [Guid]::NewGuid().ToString("N"))
    $stage = Join-Path $root "stage"
    $dist = Join-Path $root "dist"
    $outOk = Join-Path $root "install-ok"
    $outBad = Join-Path $root "install-bad"
    New-Item -ItemType Directory -Path $stage, $dist -Force | Out-Null
    try {
        $tag = "v1.15.0"
        $asset = "buildlang-$tag-x86_64-pc-windows-msvc.zip"
        # Use cmd.exe copy or lightweight .cmd-less PE stubs? Build real tiny stubs using buildc or copy existing if present;
        # otherwise verify with SkipVerify on synthetic binary fixtures AND test real cmd verification.
        $useReal = $false
        $realBuildc = (Get-Command buildc.exe -ErrorAction SilentlyContinue)
        if ($realBuildc -and (Test-Path -LiteralPath $realBuildc.Source)) {
            $useReal = $true
        }
        foreach ($bin in @("buildc.exe", "qdb.exe", "port.exe")) {
            if ($useReal -and $bin -eq "buildc.exe") {
                Copy-Item -LiteralPath $realBuildc.Source -Destination (Join-Path $stage $bin) -Force
            } else {
                [System.IO.File]::WriteAllText((Join-Path $stage $bin), "stub-$bin-$tag`n")
            }
        }
        $zipPath = Join-Path $dist $asset
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -Force
        $hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        [System.IO.File]::WriteAllText((Join-Path $dist "SHA256SUMS"), "$hash  $asset`n")

        # Control 1: Valid archive + matching SHA256SUMS must install all 3 binaries.
        Install-BuildLang -RepoSlug "HarperZ9/buildlang" -Tag $tag -TargetDir $outOk -LocalDir $dist -SkipPath $true -SkipVerify $true
        foreach ($bin in @("buildc.exe", "qdb.exe", "port.exe")) {
            if (-not (Test-Path -LiteralPath (Join-Path $outOk $bin))) {
                throw "Self-test failed: $bin was not installed"
            }
        }
        if ($useReal) {
            $ver = & (Join-Path $outOk "buildc.exe") --version 2>&1
            if ($LASTEXITCODE -ne 0) { throw "Self-test real buildc --version failed: $ver" }
            Write-Host "Self-test real binary check: $ver"
        }

        # Control 2: Planted 1-byte archive tamper must be refused fail-closed before extraction.
        $bytes = [System.IO.File]::ReadAllBytes($zipPath)
        $bytes[$bytes.Length - 1] = $bytes[$bytes.Length - 1] -bxor 0x01
        [System.IO.File]::WriteAllBytes($zipPath, $bytes)
        $refused = $false
        try {
            Install-BuildLang -RepoSlug "HarperZ9/buildlang" -Tag $tag -TargetDir $outBad -LocalDir $dist -SkipPath $true -SkipVerify $true
        } catch {
            if ($_.Exception.Message -match "SHA-256 mismatch") {
                $refused = $true
            } else {
                throw
            }
        }
        if (-not $refused) {
            throw "Self-test failed: tampered archive was NOT refused!"
        }
        if (Test-Path -LiteralPath (Join-Path $outBad "buildc.exe")) {
            throw "Self-test failed: tampered archive extracted binaries!"
        }
        Write-Host "BUILDLANG INSTALL.PS1 SELF_TEST PASS valid_install=1 planted_tamper_refused=1"
    } finally {
        Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if ($SelfTest) {
    Invoke-SelfTest
    exit 0
}

if (-not $InstallDir -or $InstallDir.Trim().Length -eq 0) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "BuildLang\bin"
}
$resolvedTag = if ($FromDir -and (-not $Version)) { "v1.15.0" } else { Resolve-BuildLangVersion -RepoSlug $Repo -Requested $Version }
Install-BuildLang -RepoSlug $Repo -Tag $resolvedTag -TargetDir $InstallDir -LocalDir $FromDir -SkipPath $NoPath -SkipVerify $false
