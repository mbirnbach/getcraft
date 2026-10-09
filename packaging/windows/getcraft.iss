; Inno Setup script for the GetCraft installer (getcraft-<version>-windows-setup.exe).
;
; Built by the release workflow from the two portable folders it stages:
;   iscc /DAppVersion=0.2.0 /DStage=..\..\dist packaging\windows\getcraft.iss
; One installer serves x64 and ARM64 PCs and picks the matching GetCraft.exe.
;
; Installs per user (no admin rights, no UAC prompt) into %LOCALAPPDATA%\Programs\GetCraft, the
; folder GetCraft's self-updater and app installer already use. GetCraft has no runtime
; dependencies: the C runtime is linked in (see .cargo/config.toml).

#ifndef AppVersion
  #error Pass /DAppVersion=<version>
#endif
#ifndef Stage
  #define Stage "..\..\dist"
#endif

[Setup]
; Never change AppId: Windows uses it to recognise upgrades and the uninstaller.
AppId={{7CF358B6-E104-44DA-AC2E-E883DB1AA9F1}
AppName=GetCraft
AppVersion={#AppVersion}
AppVerName=GetCraft {#AppVersion}
AppPublisher=GetCraft (community project)
AppPublisherURL=https://github.com/mbirnbach/getcraft
AppSupportURL=https://github.com/mbirnbach/getcraft/issues
AppUpdatesURL=https://github.com/mbirnbach/getcraft/releases
AppCopyright=Copyright (c) 2026 the GetCraft contributors
VersionInfoVersion={#AppVersion}
VersionInfoProductName=GetCraft
VersionInfoDescription=GetCraft Setup
DefaultDirName={localappdata}\Programs\GetCraft
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible arm64
ArchitecturesInstallIn64BitMode=x64compatible arm64
MinVersion=10.0.17763
OutputDir={#Stage}
OutputBaseFilename=getcraft-{#AppVersion}-windows-setup
SetupIconFile=..\..\assets\getcraft.ico
UninstallDisplayIcon={app}\GetCraft.exe
UninstallDisplayName=GetCraft
WizardStyle=modern
WizardSizePercent=110
Compression=lzma2/max
SolidCompression=yes
; GetCraft hides to the tray instead of closing, so it has to be stopped for upgrades/uninstall.
CloseApplications=force
RestartApplications=no
ShowLanguageDialog=auto

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "it"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "nl"; MessagesFile: "compiler:Languages\Dutch.isl"
Name: "pl"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "ptbr"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "ja"; MessagesFile: "compiler:Languages\Japanese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Stage}\x64\GetCraft.exe"; DestDir: "{app}"; Check: not IsArm64; Flags: ignoreversion
Source: "{#Stage}\arm64\GetCraft.exe"; DestDir: "{app}"; Check: IsArm64; Flags: ignoreversion
Source: "{#Stage}\x64\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\x64\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\x64\NOTICE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\x64\ATTRIBUTION.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\x64\LICENSE-icon.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\GetCraft"; Filename: "{app}\GetCraft.exe"
Name: "{autodesktop}\GetCraft"; Filename: "{app}\GetCraft.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\GetCraft.exe"; Description: "{cm:LaunchProgram,GetCraft}"; Flags: nowait postinstall skipifsilent

[Registry]
; Remove the "start at login" entry GetCraft may have created (only if it exists).
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "GetCraft"; Flags: uninsdeletevalue dontcreatekey

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/IM GetCraft.exe /F"; Flags: runhidden; RunOnceId: "StopGetCraft"

[UninstallDelete]
; Leftovers of self-updates (the previous exe can't be deleted while running).
Type: files; Name: "{app}\*.getcraft-old"
; Apps installed with GetCraft live in subfolders of {app} and are deliberately kept.
