; ADBHelper installer (Inno Setup 6). Built by scripts/Build-Release.ps1:
;   ISCC.exe /DAppVersion=0.4.11 installer\adbhelper.iss
; Installs per user without administrator rights. The app's own updater keeps
; replacing ADBHelper.exe in the install folder after this first installation.

#ifndef AppVersion
  #error AppVersion is missing. Pass /DAppVersion=<x.y.z>.
#endif

[Setup]
; Keep this AppId forever: Windows uses it to find the installed app for updates and uninstall.
AppId={{3E7F0FBB-556D-4708-9254-3F8B13E7F30F}
AppName=ADBHelper
AppVersion={#AppVersion}
AppVerName=ADBHelper {#AppVersion}
AppPublisher=KaelanTesseract
AppPublisherURL=https://github.com/KaelanTesseract/ADBHelper
AppSupportURL=https://github.com/KaelanTesseract/ADBHelper/issues
AppUpdatesURL=https://github.com/KaelanTesseract/ADBHelper/releases
; Per-user install: {autopf} is %LOCALAPPDATA%\Programs without administrator rights.
PrivilegesRequired=lowest
DefaultDirName={autopf}\ADBHelper
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
LicenseFile=..\LICENSE
OutputDir=..\dist
OutputBaseFilename=ADBHelper-Setup-x64
; adbhelper.ico is the logo from assets/adbhelper-logo-v2.png, as build.rs embeds it in the EXE.
SetupIconFile=adbhelper.ico
UninstallDisplayIcon={app}\ADBHelper.exe
UninstallDisplayName=ADBHelper
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
; Close a running ADBHelper before files are replaced; do not restart it afterwards.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\adbhelper.exe"; DestDir: "{app}"; DestName: "ADBHelper.exe"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\ADBHelper"; Filename: "{app}\ADBHelper.exe"
Name: "{autodesktop}\ADBHelper"; Filename: "{app}\ADBHelper.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\ADBHelper.exe"; Description: "{cm:LaunchProgram,ADBHelper}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Leftovers of an interrupted in-app update. Settings in %APPDATA%\ADBHelper stay.
Type: filesandordirs; Name: "{app}\.adbhelper-update-*"
