; Built from the verified released Windows payload, not an arbitrary folder.
[Setup]
AppId={{663D2FE2-8DF9-4DDA-A7F5-607083D786B3}
AppName=Tayriva
AppVersion={#TayrivaVersion}
AppPublisher=Tayriva
AppPublisherURL=https://tayriva.lhemerly.chatgpt.site/
DefaultDirName={localappdata}\Programs\Tayriva
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\tayriva.exe
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0
OutputBaseFilename=tayriva-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Files]
Source: "{#Payload}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\Tayriva"; Filename: "{app}\tayriva.exe"; WorkingDir: "{app}"

[Run]
Filename: "{app}\tayriva.exe"; Description: "{cm:LaunchProgram,Tayriva}"; Flags: nowait postinstall skipifsilent

; No UninstallDelete or home-directory cleanup. Always preserve files that
; were not installed by Setup, including workbooks saved in the app directory.
