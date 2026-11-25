; Inno Setup script for Flow (Windows x64). Built by windows\scripts\release.ps1,
; which passes the version and stages the files in target\dist\stage first:
;   ISCC /DMyVer=0.1.0 /DMyShown=0.1.0.1 /DMyBuild=42 windows\packaging\installer.iss
; MyShown is the version people see, a patch release's number and all (MyVer
; when there's none); MyVer.MyBuild is the one installs compare.
; Produces target\dist\Flow-<shown>-setup.exe — the artifact WinSparkle installs.
; Relative paths start from this file's folder, so ..\.. is the repo root.

#ifndef MyVer
  #define MyVer "0.0.0"
#endif
#ifndef MyShown
  #define MyShown MyVer
#endif
#ifndef MyBuild
  #define MyBuild "0"
#endif
; A test build of work toward a version shows it as 0.1.40.DEV, but Windows'
; product version takes numbers alone: those there, the words in its text.
#define MyShownNumbers StringChange(MyShown, ".DEV", "")
#define MyApp "Flow"
#define Stage "..\..\target\dist\stage"

[Setup]
AppId={{D8EEDC06-A196-44EA-9EDC-A9B4CB6FCAFB}
AppName={#MyApp}
AppVersion={#MyVer}.{#MyBuild}
AppVerName={#MyApp} {#MyShown} (build {#MyBuild})
AppPublisher=arcanii
AppPublisherURL=https://github.com/arcanii/Flow-releases
VersionInfoVersion={#MyVer}.{#MyBuild}
VersionInfoProductVersion={#MyShownNumbers}
VersionInfoProductTextVersion={#MyShown}
DefaultDirName={autopf}\{#MyApp}
DefaultGroupName={#MyApp}
DisableProgramGroupPage=yes
SetupIconFile=..\..\assets\icon\flow.ico
UninstallDisplayIcon={app}\flow.exe
OutputDir=..\..\target\dist
OutputBaseFilename={#MyApp}-{#MyShown}-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
; Lets windows\scripts\release.ps1 smoke-test a per-user install without UAC (/CURRENTUSER).
PrivilegesRequiredOverridesAllowed=commandline dialog
; Updates: Restart Manager closes a running Flow before overwriting files and
; relaunches it. AppMutex matches the mutex Flow holds while running.
AppMutex=Flow.Running
CloseApplications=yes
RestartApplications=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Stage}\flow.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\WinSparkle.dll"; DestDir: "{app}"; Flags: ignoreversion
; The built-in bash (BusyBox), used when the machine has no Git Bash, MSYS2 or WSL.
Source: "{#Stage}\busybox.exe"; DestDir: "{app}"; Flags: ignoreversion
; flow-help, flow-settings, flowapp, flow-mcp and the flow command (flow-command.exe,
; as flow.exe), in the folder Flow puts at the end of its tabs' PATH.
Source: "{#Stage}\games\flow-help.exe"; DestDir: "{app}\games"; Flags: ignoreversion
Source: "{#Stage}\games\flow-settings.exe"; DestDir: "{app}\games"; Flags: ignoreversion
Source: "{#Stage}\games\flowapp.exe"; DestDir: "{app}\games"; Flags: ignoreversion
Source: "{#Stage}\games\flow-mcp.exe"; DestDir: "{app}\games"; Flags: ignoreversion
Source: "{#Stage}\games\flow.exe"; DestDir: "{app}\games"; Flags: ignoreversion
Source: "{#Stage}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
; The manual and the pages beside it, which the README's Documentation links.
Source: "{#Stage}\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#Stage}\THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion

[InstallDelete]
; Flow carried the games and the demos itself until 0.1.33; flowapp installs
; them now (in Flow's own folder, apps\bin), so an update takes an older
; Flow's copies away.
Type: files; Name: "{app}\games\flow-rogue.exe"
Type: files; Name: "{app}\games\flow-mines.exe"
Type: files; Name: "{app}\games\flow-demo.exe"

[Icons]
Name: "{group}\{#MyApp}"; Filename: "{app}\flow.exe"
Name: "{group}\Uninstall {#MyApp}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyApp}"; Filename: "{app}\flow.exe"; Tasks: desktopicon

[Registry]
; Flow registers the name its toasts go under for each user who runs it
; (windows\src\notify.rs); uninstalling takes the registration away, and
; installing makes none.
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\com.arcanii.Flow"; Flags: uninsdeletekey dontcreatekey

[Run]
Filename: "{app}\flow.exe"; Description: "Launch {#MyApp}"; Flags: nowait postinstall skipifsilent
