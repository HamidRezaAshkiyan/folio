#ifndef AppVersion
  #define AppVersion "1.5.0"
#endif

[Setup]
AppId={{9C2E3F40-6B4A-4A51-9D31-5E0E6F2F4C01}
AppName=Folio Bullet Journal
AppVersion={#AppVersion}
AppPublisher=HamidRezaAshkiyan
AppPublisherURL=https://github.com/HamidRezaAshkiyan/folio
DefaultDirName={localappdata}\Programs\Folio
DefaultGroupName=Folio
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\..\dist
OutputBaseFilename=folio-windows-x64-setup
Compression=lzma
SolidCompression=yes
WizardStyle=modern
ChangesEnvironment=yes

[Files]
Source: "..\..\target\x86_64-pc-windows-msvc\release\folio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Folio (TUI)"; Filename: "{app}\folio.exe"; Parameters: "--tui"
Name: "{group}\Folio (Web GUI)"; Filename: "{app}\folio.exe"; Parameters: "--gui"
Name: "{group}\Uninstall Folio"; Filename: "{uninstallexe}"

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Check: NeedsAddPath(ExpandConstant('{app}'))

[Code]
function NeedsAddPath(Param: string): boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + UpperCase(Param) + ';', ';' + UpperCase(OrigPath) + ';') = 0;
end;
