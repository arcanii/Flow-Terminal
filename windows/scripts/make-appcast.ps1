#Requires -Version 7
# Write the WinSparkle appcast for a Flow release. The appcast and the installers
# are published in the public arcanii/Flow-releases repo, apart from the source:
# commit the file this writes as appcast.xml on that repo's main branch.
#
#   pwsh windows\scripts\make-appcast.ps1 -Version 0.1.0.12 -Tag v0.1.0 -SetupExe target\dist\Flow-0.1.0-setup.exe
#
# Without -Signature it signs the installer here with Flow's update key
# (-KeyFile, by default ~\.flow-signing\flow-update.key). With -Signature it uses
# one made on the Mac by windows/scripts/sign-release.sh. Either way it checks first that
# the installer is the build named by -Version and that the signature verifies
# against the updater's public key, because either mistake would only show up as
# failed updates on users' machines.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Version,    # marketing.build, e.g. 0.1.0.12 (what WinSparkle compares)
    [Parameter(Mandatory)][string]$Tag,        # the release tag, e.g. v0.1.0, or v0.1.0.1 for a patch release; not v<Version>
    [Parameter(Mandatory)][string]$SetupExe,   # the installer users download
    [string]$Signature,                        # sparkle:edSignature from the Mac; omit to sign here
    [string]$KeyFile = (Join-Path $env:USERPROFILE '.flow-signing\flow-update.key'),
    [string]$Repo = 'arcanii/Flow-releases',
    [string]$Notes,                            # release-notes URL; defaults to the release page
    [string]$Out                               # defaults to target\dist\appcast.xml
)
$ErrorActionPreference = 'Stop'
# This script is in windows\scripts; the repo root is two levels up.
$root = Split-Path -Path (Split-Path -Path $PSScriptRoot -Parent) -Parent
if (-not (Test-Path -LiteralPath $SetupExe)) { throw "Setup exe not found: $SetupExe" }
$SetupExe = (Resolve-Path -LiteralPath $SetupExe).Path
if (-not $Out) { $Out = Join-Path $root 'target\dist\appcast.xml' }

# Inno Setup pads its version strings with spaces.
$stamped = (Get-Item -LiteralPath $SetupExe).VersionInfo.FileVersion.Trim()
if ($stamped -ne $Version) { throw "$SetupExe is version $stamped, not $Version." }
if ($Tag -notmatch '^v\d+\.\d+\.\d+(\.\d+)?$') { throw "-Tag should look like v0.1.0 (or v0.1.0.1 for a patch release), not $Tag." }
# The version people see is the tag's; its first three numbers start -Version.
$shown = $Tag.Substring(1)
if (-not $Version.StartsWith((($shown -split '\.')[0..2] -join '.') + '.')) { throw "-Tag $Tag doesn't match -Version $Version." }

function Invoke-UpdateKey {
    # With installer detection on, Windows takes an exe that has no manifest and
    # "update" in its name for an installer, and won't start it without
    # elevation. RunAsInvoker starts it with the user's own rights instead.
    $layer = $env:__COMPAT_LAYER
    $env:__COMPAT_LAYER = 'RunAsInvoker'
    try {
        cargo run --quiet --locked --example update_key --manifest-path (Join-Path $root 'Cargo.toml') -- @args
    } finally {
        $env:__COMPAT_LAYER = $layer
    }
}
if (-not $Signature) {
    if (-not (Test-Path -LiteralPath $KeyFile)) { throw "No -Signature given, and no key file at $KeyFile to sign with." }
    $Signature = Invoke-UpdateKey sign $SetupExe $KeyFile
    if ($LASTEXITCODE) { throw "Signing with $KeyFile failed." }
    Write-Host "Signed with $KeyFile"
}
$Signature = $Signature.Trim()
Invoke-UpdateKey verify $SetupExe $Signature
if ($LASTEXITCODE) { throw "The signature check failed. Sign this exact file with Flow's update key." }

$len = (Get-Item -LiteralPath $SetupExe).Length
$file = Split-Path -Path $SetupExe -Leaf
$url = "https://github.com/$Repo/releases/download/$Tag/$file"
if (-not $Notes) { $Notes = "https://github.com/$Repo/releases/tag/$Tag" }
$pub = (Get-Date).ToUniversalTime().ToString(
    'ddd, dd MMM yyyy HH:mm:ss +0000', [System.Globalization.CultureInfo]::InvariantCulture)
$xml = @"
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Flow (Windows) Updates</title>
    <link>https://raw.githubusercontent.com/$Repo/main/appcast.xml</link>
    <description>Most recent updates to Flow for Windows</description>
    <language>en</language>
    <item>
      <title>Version $Version</title>
      <sparkle:releaseNotesLink>$Notes</sparkle:releaseNotesLink>
      <pubDate>$pub</pubDate>
      <enclosure url="$url"
                 sparkle:version="$Version"
                 sparkle:shortVersionString="$shown"
                 sparkle:edSignature="$Signature"
                 length="$len"
                 type="application/octet-stream"/>
    </item>
  </channel>
</rss>
"@
[System.IO.File]::WriteAllText($Out, $xml + "`n", [System.Text.UTF8Encoding]::new($false))
Write-Host "Wrote $Out"
Write-Host "  version=$Version  length=$len"
Write-Host "  url=$url"
Write-Host "Next: upload $file to the $Tag release in $Repo, then commit this file there as appcast.xml on main."
