#Requires -Version 7
# Build a Windows x64 release of Flow into target\dist:
#   Flow-<ver>-setup.exe          Inno Setup installer, the build WinSparkle updates
#   Flow-<ver>-x64-portable.zip   flow.exe without the updater; updated by hand
#   SHA256SUMS.txt
# Both carry flow-help.exe, flow-settings.exe, flowapp.exe, flow-mcp.exe and the
# flow command (flow-command.exe, as flow.exe) in a games folder beside
# flow.exe; from 0.1.34 the games and the demos come from flowapp, not
# with Flow, and the installer takes an older Flow's copies away. <ver> is a patch
# release's too (0.1.25.1), as the app shows it.
# Commit first: the build number is the commit count (plus build_offset), so it
# has to name HEAD.
#
#   pwsh windows\scripts\release.ps1              build and package
#   pwsh windows\scripts\release.ps1 -SmokeTest   also install to a temp dir, check, uninstall
#
# Flow updates itself only in a build made with its update key at hand
# (docs\design\updates-for-releases.md): this builds with FLOW_UPDATES=1
# where -KeyFile, which make-appcast.ps1 signs with, holds the key the build
# trusts (UPDATE_PUBLIC_KEY), and a release made without it stops unless
# given -NoUpdates.
[CmdletBinding()]
param(
    [switch]$SkipBuild,    # package the flow.exe already in target\x86_64-pc-windows-msvc\release
    [switch]$AllowDirty,   # allow uncommitted changes, for test builds only
    [switch]$SmokeTest,
    [switch]$NoUpdates,    # a release that never updates itself, made without Flow's update key
    [string]$KeyFile = (Join-Path $env:USERPROFILE '.flow-signing\flow-update.key')
)
$ErrorActionPreference = 'Stop'
# This script is in windows\scripts; the repo root is two levels up.
$root = Split-Path -Path (Split-Path -Path $PSScriptRoot -Parent) -Parent
$win = Join-Path $root 'windows'
$dist = Join-Path $root 'target\dist'
$stage = Join-Path $dist 'stage'
# Always x64, even on an ARM64 PC, where a plain build would be ARM64: the
# installer, the zip, WinSparkle.dll and busybox.exe are all x64, and ARM64
# Windows runs them emulated.
$target = 'x86_64-pc-windows-msvc'
$exe = Join-Path $root "target\$target\release\flow.exe"
$games = 'flow-help.exe', 'flow-settings.exe', 'flowapp.exe', 'flow-mcp.exe' | ForEach-Object { Join-Path $root "target\$target\release\$_" }

# Marketing version from [workspace.package] in Cargo.toml; build number =
# commit count plus `build_offset` under [workspace.metadata.flow], as
# core/build_number.rs has it. $full, which the installer and
# WinSparkle compare, leaves a patch release's number out; $shown has it.
$ver = (Select-String -LiteralPath (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$patch = 0
$offset = 0
$dev = $false
$inFlow = $false
foreach ($line in Get-Content -LiteralPath (Join-Path $root 'Cargo.toml')) {
    $line = ($line -split '#')[0].Trim()
    if ($line.StartsWith('[')) { $inFlow = $line -eq '[workspace.metadata.flow]' }
    elseif ($inFlow -and $line -match '^patch\s*=\s*(\d+)$') { $patch = [int]$Matches[1] }
    elseif ($inFlow -and $line -match '^build_offset\s*=\s*(\d+)$') { $offset = [int]$Matches[1] }
    elseif ($inFlow -and $line -match '^dev\s*=\s*true$') { $dev = $true }
}
$shown = if ($patch) { "$ver.$patch" } else { $ver }
# Work toward a version (`dev = true` there) isn't released: Flow says
# 0.1.40.DEV until releasing sets it to false. A test build keeps it.
if ($dev) {
    if (-not $AllowDirty) {
        throw "Flow is $shown.DEV, still worked toward: set dev = false under [workspace.metadata.flow] and commit to release it (or pass -AllowDirty for a test build)."
    }
    $shown = "$shown.DEV"
}
$build = (git -C $root rev-list --count HEAD).Trim()
if ($LASTEXITCODE) { throw 'git rev-list failed' }
$build = "$([int]$build + $offset)"
$full = "$ver.$build"
Write-Host "Flow $shown (build $build)"

if (-not $AllowDirty -and (git -C $root status --porcelain)) {
    throw 'Uncommitted changes. Commit first, since the build number names HEAD (or pass -AllowDirty for a test build).'
}

# Updates, only with Flow's update key at hand: the key file's public half
# must be the one the build trusts, so a fork's release never offers Flow's
# in its place.
$updates = $false
if (-not $NoUpdates) {
    $trusted = (Select-String -LiteralPath (Join-Path $root 'core\src\lib.rs') -Pattern '^pub const UPDATE_PUBLIC_KEY: &str = "([^"]+)";' |
        Select-Object -First 1).Matches[0].Groups[1].Value
    $held = $null
    if (Test-Path -LiteralPath $KeyFile) {
        # As make-appcast.ps1 runs it: Windows takes an exe with "update" in
        # its name for an installer, which RunAsInvoker starts unelevated.
        $layer = $env:__COMPAT_LAYER
        $env:__COMPAT_LAYER = 'RunAsInvoker'
        try {
            $held = cargo run --quiet --locked --example update_key --manifest-path (Join-Path $root 'Cargo.toml') -- public $KeyFile |
                Select-Object -Last 1
        } finally {
            $env:__COMPAT_LAYER = $layer
        }
    }
    if ($held -and "$held".Trim() -eq $trusted) { $updates = $true }
    elseif ($AllowDirty) { Write-Host "note: $KeyFile doesn't hold Flow's update key (UPDATE_PUBLIC_KEY), so this test build won't update itself." }
    else { throw "$KeyFile doesn't hold Flow's update key (UPDATE_PUBLIC_KEY; windows\RELEASING.md), so this release couldn't update itself. Pass -NoUpdates for a release that never does." }
}
$saidUpdates = if ($updates) { 'updates: on' } else { 'updates: off' }
Write-Host "This build: $saidUpdates"
$env:FLOW_UPDATES = if ($updates) { '1' } else { '0' }

if (-not $SkipBuild) {
    if (Get-Process flow -ErrorAction SilentlyContinue | Where-Object Path -like "*\target\$target\release\flow.exe") {
        throw "Flow is running from target\$target\release. Close it so the build can replace flow.exe."
    }
    if (-not (rustup target list --installed | Where-Object { $_ -eq $target })) {
        throw "The $target Rust target isn't installed. Add it: rustup target add $target"
    }
    # The build's own folders out of the programs (panics name their source
    # files): the user's profile as ~ (Cargo's crates are under it) and the
    # repo as flow, so a release names no one's account. Encoded, parted by
    # 0x1f, as a folder may hold spaces.
    $env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~" + [char]0x1f + "--remap-path-prefix=$root=flow"
    cargo build --release --locked --target $target -p flow-terminal --bins --manifest-path (Join-Path $root 'Cargo.toml')
    if ($LASTEXITCODE) { throw "cargo build failed (exit $LASTEXITCODE)" }
}
$stamped = (Get-Item -LiteralPath $exe).VersionInfo.FileVersion
if ($stamped -ne $full) { throw "target\$target\release\flow.exe is $stamped, not $full. Rebuild without -SkipBuild." }
# flow.exe is a window's program, which prints nothing here; the flow
# command, built with it from the same core, says what it was built for.
$env:FLOW_SAY_UPDATES = '1'
try {
    $said = & (Join-Path $root "target\$target\release\flow-command.exe") --version | Select-Object -Last 1
} finally {
    Remove-Item Env:FLOW_SAY_UPDATES -ErrorAction SilentlyContinue
}
if ($said -ne $saidUpdates) { throw "target\$target\release\flow-command.exe says '$said', not '$saidUpdates'. Rebuild without -SkipBuild." }

if (Test-Path -LiteralPath $dist) { Remove-Item -LiteralPath $dist -Recurse -Force }
New-Item -ItemType Directory -Path $stage | Out-Null
Copy-Item -LiteralPath $exe, (Join-Path $win 'third_party\winsparkle\x64\WinSparkle.dll'),
    (Join-Path $root 'README.md'), (Join-Path $root 'LICENSE') -Destination $stage
# The README's Documentation links into docs\, so the manual and the pages
# beside it go too, as the README carried the manual itself before it moved
# there; its pictures stay online, as they always did, and the pages for
# contributors (BACKLOG, CONTRIBUTING, the designs) stay in the source.
New-Item -ItemType Directory -Path (Join-Path $stage 'docs\manual') | Out-Null
Copy-Item -Path (Join-Path $root 'docs\*.md') -Destination (Join-Path $stage 'docs')
Copy-Item -Path (Join-Path $root 'docs\manual\*.md') -Destination (Join-Path $stage 'docs\manual')
# The built-in bash is BusyBox (GPLv2), so its exact source goes out with every release.
Copy-Item -LiteralPath (Join-Path $win 'third_party\busybox\busybox64u.exe') -Destination (Join-Path $stage 'busybox.exe')
# flow-help, flow-settings and flowapp, in the folder Flow puts at the end of its tabs' PATH (Platform::games_dir).
New-Item -ItemType Directory -Path (Join-Path $stage 'games') | Out-Null
Copy-Item -LiteralPath $games -Destination (Join-Path $stage 'games')
# The flow command, a console program a shell waits on, is flow.exe there:
# Flow's own flow.exe above it is a window's, which a shell can't print through.
Copy-Item -LiteralPath (Join-Path $root "target\$target\release\flow-command.exe") -Destination (Join-Path $stage 'games\flow.exe')
$bbSource = Get-ChildItem -LiteralPath (Join-Path $win 'third_party\busybox') -Filter 'busybox-w32-FRP-*.tgz' |
    Select-Object -First 1
if (-not $bbSource) { throw 'windows\third_party\busybox has no busybox-w32-FRP-*.tgz source to publish with the release.' }
Copy-Item -LiteralPath $bbSource.FullName -Destination $dist
& (Join-Path $PSScriptRoot 'third-party-notices.ps1') -Out (Join-Path $stage 'THIRD-PARTY-NOTICES.txt')

$iscc = "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe" | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 not found. Install it: winget install JRSoftware.InnoSetup' }
& $iscc /Q "/DMyVer=$ver" "/DMyShown=$shown" "/DMyBuild=$build" (Join-Path $win 'packaging\installer.iss')
if ($LASTEXITCODE) { throw "ISCC failed (exit $LASTEXITCODE)" }
$setup = Join-Path $dist "Flow-$shown-setup.exe"

# The portable zip leaves WinSparkle.dll out: an update runs the installer, which
# would put a second copy in Program Files instead of updating this one.
$zip = Join-Path $dist "Flow-$shown-x64-portable.zip"
Compress-Archive -Path (Join-Path $stage 'flow.exe'), (Join-Path $stage 'busybox.exe'), (Join-Path $stage 'games'),
    (Join-Path $stage 'README.md'), (Join-Path $stage 'LICENSE'), (Join-Path $stage 'docs'),
    (Join-Path $stage 'THIRD-PARTY-NOTICES.txt') -DestinationPath $zip

$sums = foreach ($f in $setup, $zip, (Join-Path $dist $bbSource.Name)) {
    '{0}  {1}' -f (Get-FileHash -LiteralPath $f -Algorithm SHA256).Hash.ToLowerInvariant(), (Split-Path $f -Leaf)
}
# LF endings, so `sha256sum -c` works on macOS and Linux too.
[System.IO.File]::WriteAllText((Join-Path $dist 'SHA256SUMS.txt'), ($sums -join "`n") + "`n")

if ($SmokeTest) {
    if (Get-Process flow -ErrorAction SilentlyContinue) {
        throw 'Close every Flow window first: setup refuses to run while Flow holds its mutex.'
    }
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) "flow-smoke-$build"
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force }
    New-Item -ItemType Directory -Path $tmp | Out-Null
    # Run a local copy: Windows asks before starting an exe from a network share.
    Copy-Item -LiteralPath $setup -Destination $tmp
    $app = Join-Path $tmp 'app'
    $p = Start-Process -FilePath (Join-Path $tmp (Split-Path $setup -Leaf)) -Wait -PassThru -ArgumentList `
        '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', "/DIR=`"$app`"", "/LOG=`"$tmp\setup.log`""
    if ($p.ExitCode) { throw "Setup exited with $($p.ExitCode); see $tmp\setup.log" }
    foreach ($f in 'flow.exe', 'WinSparkle.dll', 'busybox.exe', 'games\flow-help.exe', 'games\flow-settings.exe', 'games\flowapp.exe', 'games\flow-mcp.exe', 'games\flow.exe', 'README.md', 'LICENSE', 'docs\manual\install.md', 'THIRD-PARTY-NOTICES.txt', 'unins000.exe') {
        if (-not (Test-Path -LiteralPath (Join-Path $app $f))) { throw "Setup didn't install $f" }
    }
    $installed = (Get-Item -LiteralPath (Join-Path $app 'flow.exe')).VersionInfo.FileVersion
    if ($installed -ne $full) { throw "Installed flow.exe is $installed, not $full" }
    foreach ($name in 'flow-rogue', 'flow-mines', 'flow-demo') {
        if (Test-Path -LiteralPath (Join-Path $app "games\$name.exe")) { throw "Setup installed games\$name.exe, which flowapp installs now" }
    }
    $apps = & (Join-Path $app 'games\flowapp.exe') --version
    if ($apps -notlike 'flowapp *') { throw "Installed flowapp.exe --version printed '$apps'" }
    $command = & (Join-Path $app 'games\flow.exe') --version
    if ($command -ne "Flow $shown (build $build)") { throw "Installed games\flow.exe --version printed '$command'" }
    $listed = & (Join-Path $app 'games\flow-help.exe') --list
    if (-not ($listed -match '^settings\s')) { throw "Installed flow-help.exe --list printed '$listed'" }
    $u = Start-Process -FilePath (Join-Path $app 'unins000.exe') -Wait -PassThru `
        -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'
    if ($u.ExitCode) { throw "Uninstall exited with $($u.ExitCode)" }
    # The uninstaller hands off to a copy of itself in %TEMP%, so wait for the files to go.
    $deadline = (Get-Date).AddSeconds(30)
    while ((Test-Path -LiteralPath (Join-Path $app 'flow.exe')) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 250 }
    if (Test-Path -LiteralPath (Join-Path $app 'flow.exe')) { throw "Uninstall left $app\flow.exe behind" }
    if (Test-Path -LiteralPath (Join-Path $app 'games')) { throw "Uninstall left $app\games behind" }
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "Smoke test passed: installed $installed per-user, then uninstalled it."
}

Write-Host ''
Write-Host "Built in ${dist}:"
Get-ChildItem -LiteralPath $dist -File | ForEach-Object { '  {0,-40} {1,8:N1} MB' -f $_.Name, ($_.Length / 1MB) }
if ($dev) {
    Write-Host "`nA test build of work toward $ver, not for release."
    return
}
if (-not $updates) {
    Write-Host "`nIt never updates itself (-NoUpdates): don't put it in Flow's appcast, where every Flow would update to it and then stop."
}
Write-Host @"

Next (windows\RELEASING.md has the details):
  1. gh release create v$shown --repo arcanii/Flow-releases --title "Flow $shown" target\dist\Flow-$shown-setup.exe target\dist\Flow-$shown-x64-portable.zip target\dist\$($bbSource.Name) target\dist\SHA256SUMS.txt
     (the BusyBox source must go up with the binaries: the GPL asks for it)
  2. pwsh windows\scripts\make-appcast.ps1 -Version $full -Tag v$shown -SetupExe target\dist\Flow-$shown-setup.exe
     (signs with ~\.flow-signing\flow-update.key; for a Mac signature add -Signature <sig>)
  3. Commit target\dist\appcast.xml as appcast.xml on main in arcanii/Flow-releases.
"@
