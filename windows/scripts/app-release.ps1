#Requires -Version 7
# Make one of flowapp's apps for Windows, as macos\scripts\app-release.sh does
# on the Mac: build it, pack it as
# target\apps\<app>-<version>-windows-x86_64.tar.gz, sign that with Flow's
# update key, and write its entry into flowapp's catalog (apps.json,
# core\src\apps.rs; examples\app_catalog.rs writes the entry).
#
#   pwsh windows\scripts\app-release.ps1 <app> [-Catalog <file>] [-KeyFile <file>] [-SkipBuild] [-Flow <version>] [-AllowDirty]
#
# <app>: flow-sdk, the SDK's C library (flow_sdk_c.dll, its import library
# flow_sdk_c.dll.lib and the static flow_sdk_c.lib) with its header, its
# Python package (with the DLL in its own folder, where it looks for it) and
# flow-demo, built from Flow-SDK's tag (the one Cargo.toml's flow-sdk names);
# or flowapp itself, built from this checkout at flowapp's own version
# (`flowapp` under [workspace.metadata.flow]), which runs in Flow's tabs ahead
# of the copy Flow carries (docs\design\flowapp-updates.md), from a clean
# tree, as its build number names the commit (-AllowDirty for a test).
# The games' downloads are made in their own repository, arcanii/Flow-Games,
# by its scripts\app-release.ps1, which runs this repository's examples as
# this does.
#
# Always x64, as Flow's Windows releases are: ARM64 Windows runs them
# emulated, and their flowapp asks for windows-x86_64. The programs aren't
# code-signed, as Flow's own aren't yet; the tarball is signed with Flow's
# key (-KeyFile, by default ~\.flow-signing\flow-update.key), which flowapp
# checks before it unpacks anything. Before it signs, the tarball is
# unpacked into a scratch folder and each program run with --version (and
# the SDK's Python package made to load its DLL).
#
# Then, as it prints: the tarball goes on the release <app>-v<version> in
# arcanii/Flow-releases (the Mac's download of the same version is on it
# already, or it's made with --latest=false), and the catalog to that
# repo's main as apps.json. flowapp update reads it from there.
[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)][string]$App,
    [string]$Catalog,      # the catalog to write into (default target\apps\apps.json, begun from the published one)
    [string]$KeyFile = (Join-Path $env:USERPROFILE '.flow-signing\flow-update.key'),
    [switch]$SkipBuild,    # pack what target\ has already
    [string]$Flow,         # the oldest Flow the app works in (0.1.34), which an older Flow's flowapp holds it back for; none for any (without it, what the catalog said before; flowapp's is 0.1.44 unless given)
    [switch]$AllowDirty    # build flowapp from a tree with changes, for a test
)
$ErrorActionPreference = 'Stop'
$releases = 'arcanii/Flow-releases'
$catalogUrl = "https://raw.githubusercontent.com/$releases/main/apps.json"
# This script is in windows\scripts; the repo root is two levels up.
$root = Split-Path -Path (Split-Path -Path $PSScriptRoot -Parent) -Parent
$manifest = Join-Path $root 'Cargo.toml'
$target = 'x86_64-pc-windows-msvc'
$platform = 'windows-x86_64'
$out = Join-Path $root 'target\apps'
New-Item -ItemType Directory -Force -Path $out | Out-Null
if (-not $Catalog) { $Catalog = Join-Path $out 'apps.json' }

# What each app is: its programs and its line in the catalog, the same as
# app-release.sh's.
$bundled = ''
switch ($App) {
    'flow-sdk' {
        $programs = @('flow-demo')
        $summary = "Flow's SDK for C and Python, its library, header and Python package, with flow-demo, its demos"
    }
    'flowapp' {
        $programs = @('flowapp')
        $bundled = 'flowapp'
        if (-not $Flow) { $Flow = '0.1.44' }
        $summary = "flowapp, which keeps Flow's apps: newer than the copy your Flow carries, it runs in Flow's tabs in that one's place"
    }
    { $_ -in 'flow-rogue', 'flow-mines', 'flow-invaders' } { throw "${App}'s download is made in Flow-Games: scripts\app-release.ps1 $App there" }
    default { throw "No app called $App here: flow-sdk or flowapp (the games' are made in Flow-Games)" }
}
if (-not (Test-Path -LiteralPath $KeyFile)) { throw "No key file at $KeyFile to sign with (pass -KeyFile)." }

$stage = Join-Path $out "stage\$App"
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Path (Join-Path $stage 'bin') | Out-Null

# The build's own folders out of the programs (panics name their source
# files): the user's profile as ~ (Cargo's crates are under it) and the repo
# as flow, so a download names no one's account. Encoded, parted by 0x1f, as
# a folder may hold spaces. Only for the programs' builds: the tools run
# below are built as they always are, so they don't build everything again.
function Invoke-Build {
    if (-not (rustup target list --installed | Where-Object { $_ -eq $target })) {
        throw "The $target Rust target isn't installed. Add it: rustup target add $target"
    }
    $flags = $env:CARGO_ENCODED_RUSTFLAGS
    $env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~" + [char]0x1f + "--remap-path-prefix=$root=flow"
    try {
        cargo build --release --locked --target $target @args
        if ($LASTEXITCODE) { throw "cargo build failed (exit $LASTEXITCODE)" }
    } finally {
        $env:CARGO_ENCODED_RUSTFLAGS = $flags
    }
}

function Invoke-Example([string]$Name) {
    # With installer detection on, Windows takes an exe that has no manifest
    # and "update" in its name for an installer, and won't start it without
    # elevation (update_key). RunAsInvoker starts it with the user's own rights.
    $layer = $env:__COMPAT_LAYER
    $env:__COMPAT_LAYER = 'RunAsInvoker'
    try {
        cargo run --quiet --locked --example $Name --manifest-path $manifest -- @args
        if ($LASTEXITCODE) { throw "examples\$Name.rs failed (exit $LASTEXITCODE)" }
    } finally {
        $env:__COMPAT_LAYER = $layer
    }
}

if ($App -eq 'flowapp') {
    # flowapp, from this checkout, at its own version; the build number is
    # the commit count plus build_offset, so the tree must be the commit's.
    $changes = git -C $root status --porcelain
    if ($changes -and -not $AllowDirty) { throw 'The tree has changes: commit them first (or -AllowDirty for a test).' }
    $named = Select-String -LiteralPath $manifest -Pattern '^flowapp = "([^"]*)"$' | Select-Object -First 1
    if (-not $named) { throw "Cargo.toml names no flowapp version under [workspace.metadata.flow]" }
    $version = $named.Matches[0].Groups[1].Value
    if (-not $SkipBuild) { Invoke-Build -p flow-terminal --bin flowapp }
    Copy-Item -LiteralPath (Join-Path $root "target\$target\release\flowapp.exe") -Destination (Join-Path $stage 'bin')
    Invoke-Example third_party_notices --target $target --binary 'flowapp' --out (Join-Path $stage 'THIRD-PARTY-NOTICES.txt')
} else {
    # The SDK, from the tag Flow takes it by.
    $named = Select-String -LiteralPath $manifest -Pattern '^flow-sdk = \{ git = "[^"]*", tag = "([^"]*)" \}$' | Select-Object -First 1
    if (-not $named) { throw 'Cargo.toml names no tag for flow-sdk' }
    $tag = $named.Matches[0].Groups[1].Value
    $version = $tag.TrimStart('v')
    $sdk = Join-Path $root "target\flow-sdk-$tag"
    if (-not (Test-Path -LiteralPath $sdk)) {
        git clone --quiet --depth 1 --branch $tag https://github.com/arcanii/Flow-SDK $sdk
        if ($LASTEXITCODE) { throw "git clone of Flow-SDK's $tag failed" }
    }
    # A checkout from before is used only where it's the tag, unchanged. Git
    # takes a checkout on a network share for someone else's and won't read
    # it, so these two commands trust this one, which the script cloned
    # itself, without touching the user's git config.
    $head = git -c safe.directory=* -C $sdk rev-parse HEAD
    $tagged = git -c safe.directory=* -C $sdk rev-parse "$tag^{commit}"
    $changed = git -c safe.directory=* -C $sdk status --porcelain
    if ($LASTEXITCODE -or -not $head -or "$head".Trim() -ne "$tagged".Trim() -or $changed) { throw "$sdk isn't Flow-SDK's $tag as it was tagged. Delete it to clone it again." }
    $built = Join-Path $root 'target\flow-sdk-build'
    if (-not $SkipBuild) { Invoke-Build --manifest-path (Join-Path $sdk 'Cargo.toml') -p flow-sdk-c -p flow-demos --target-dir $built }
    $rel = Join-Path $built "$target\release"
    Copy-Item -LiteralPath (Join-Path $rel 'flow-demo.exe') -Destination (Join-Path $stage 'bin')
    foreach ($d in 'lib', 'include', 'python') { New-Item -ItemType Directory -Path (Join-Path $stage $d) | Out-Null }
    # The DLL, its import library (to link against the DLL) and the static library.
    Copy-Item -LiteralPath (Join-Path $rel 'flow_sdk_c.dll'), (Join-Path $rel 'flow_sdk_c.dll.lib'), (Join-Path $rel 'flow_sdk_c.lib') -Destination (Join-Path $stage 'lib')
    Copy-Item -LiteralPath (Join-Path $sdk 'c\include\flow_sdk.h') -Destination (Join-Path $stage 'include')
    Copy-Item -LiteralPath (Join-Path $sdk 'python\flow_sdk') -Destination (Join-Path $stage 'python') -Recurse
    Get-ChildItem -LiteralPath (Join-Path $stage 'python') -Recurse -Directory -Filter '__pycache__' | Remove-Item -Recurse -Force
    # Python looks for the library in its own folder.
    Copy-Item -LiteralPath (Join-Path $rel 'flow_sdk_c.dll') -Destination (Join-Path $stage 'python\flow_sdk')
    Copy-Item -LiteralPath (Join-Path $sdk 'LICENSE-MIT'), (Join-Path $sdk 'LICENSE-APACHE') -Destination $stage
    Invoke-Example third_party_notices --manifest-path (Join-Path $sdk 'Cargo.toml') --target $target `
        --package flow-sdk-c --package flow-demos --binary "the Flow SDK's library and flow-demo" --out (Join-Path $stage 'THIRD-PARTY-NOTICES.txt')
}
Write-Host "$App $version for $platform"

# The download: what's staged, at the tarball's top. Windows' own tar
# (bsdtar), which flowapp unpacks with too, not a GNU one first on the PATH;
# it writes no owner's name.
$file = "$App-$version-$platform.tar.gz"
$tarball = Join-Path $out $file
if (Test-Path -LiteralPath $tarball) { Remove-Item -LiteralPath $tarball -Force }
$tar = Join-Path $env:SystemRoot 'System32\tar.exe'
& $tar -czf $tarball -C $stage .
if ($LASTEXITCODE) { throw "tar failed (exit $LASTEXITCODE)" }

# Unpacked as flowapp unpacks it, each program run, before anything is signed.
$tried = Join-Path ([System.IO.Path]::GetTempPath()) "flow-app-$App-$PID"
if (Test-Path -LiteralPath $tried) { Remove-Item -LiteralPath $tried -Recurse -Force }
New-Item -ItemType Directory -Path $tried | Out-Null
try {
    & $tar -xzf $tarball -C $tried
    if ($LASTEXITCODE) { throw "tar couldn't unpack $tarball (exit $LASTEXITCODE)" }
    foreach ($p in $programs) {
        $said = & (Join-Path $tried "bin\$p.exe") --version
        if ($LASTEXITCODE -or "$said" -notlike "$p *") { throw "bin\$p.exe --version printed '$said'" }
        Write-Host "  $said"
    }
    # The package loads its own copy of the DLL, whatever Cargo has built.
    $python = Get-Command python3, python -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not (Test-Path -LiteralPath (Join-Path $tried 'python'))) {
        # No Python package in it (flowapp's).
    } elseif ($python) {
        $given = $env:FLOW_SDK_LIB, $env:CARGO_TARGET_DIR
        $env:FLOW_SDK_LIB = $null
        $env:CARGO_TARGET_DIR = $null
        try {
            $loaded = & $python.Source -I -B -c "import sys; sys.path.insert(0, sys.argv[1]); from flow_sdk import _native as n; n.lib(); print(next(p for p in n.candidates() if p.is_file()), n.ABI)" (Join-Path $tried 'python')
            if ($LASTEXITCODE) { throw "The Python package didn't load its DLL" }
        } finally {
            $env:FLOW_SDK_LIB, $env:CARGO_TARGET_DIR = $given
        }
        if ("$loaded" -notlike '*\python\flow_sdk\flow_sdk_c.dll 1') { throw "The Python package loaded '$loaded', not its own DLL" }
        Write-Host "  Python's flow_sdk loads its own flow_sdk_c.dll"
    } else {
        Write-Warning 'No Python here, so the Python package went untried.'
    }
} finally {
    Remove-Item -LiteralPath $tried -Recurse -Force -ErrorAction SilentlyContinue
}

# Signed with Flow's key, as the appcasts' downloads are.
$sig = Invoke-Example update_key sign $tarball $KeyFile
$sig = "$sig".Trim()
if (-not $sig) { throw "update_key printed no signature for $tarball" }

# Its entry, into the catalog, begun from the published one: a catalog
# begun empty would drop the other systems' downloads when it's published.
if (-not (Test-Path -LiteralPath $Catalog)) {
    try {
        Invoke-WebRequest -Uri $catalogUrl -OutFile $Catalog
    } catch {
        Remove-Item -LiteralPath $Catalog -Force -ErrorAction SilentlyContinue
        throw "Couldn't read the published catalog ($catalogUrl): $_. Pass -Catalog with a copy of it."
    }
}
$url = "https://github.com/$releases/releases/download/$App-v$version/$file"
$extra = if ($Flow) { @('--flow', $Flow) } else { @() }
if ($bundled) { $extra += @('--bundled', $bundled) }
Invoke-Example app_catalog --catalog $Catalog --app $App --version $version --summary $summary --programs ($programs -join ',') @extra `
    --platform $platform --url $url --file $tarball --signature $sig

$size = (Get-Item -LiteralPath $tarball).Length
Write-Host ''
Write-Host ('Made {0} ({1:N1} MB).' -f $tarball, ($size / 1MB))
Write-Host 'Next:'
Write-Host "  1. Where the release $App-v$version is (the Mac's download of it):"
Write-Host "        gh release upload $App-v$version --repo $releases `"$tarball`" `"$tarball.sig`""
Write-Host '     else:'
Write-Host "        gh release create $App-v$version --repo $releases --title `"$App $version`" --latest=false --notes `"…`" `"$tarball`" `"$tarball.sig`""
Write-Host "  2. Commit $Catalog as apps.json on main in $releases."
