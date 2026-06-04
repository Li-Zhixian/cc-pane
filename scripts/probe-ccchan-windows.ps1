param(
  [string]$DevExePath = "D:\my-project\cc-pane\target\debug\cc-panes.exe",
  [string]$ProtocolScheme = "ccpanes",
  [string]$DevConfigPath = "$env:USERPROFILE\.cc-panes-dev\config.toml"
)

$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public class CCChanWin32Probe {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
  [DllImport("user32.dll")] public static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);
  public const int GWL_EXSTYLE = -20;
  public const long WS_EX_TOPMOST = 0x00000008L;
}

[StructLayout(LayoutKind.Sequential)]
public struct RECT {
  public int Left;
  public int Top;
  public int Right;
  public int Bottom;
}
"@

Add-Type -AssemblyName System.Windows.Forms

function Get-VisibleWindowsForProcess {
  param([int]$ProcessId)

  $windows = New-Object System.Collections.Generic.List[object]
  $callback = [CCChanWin32Probe+EnumWindowsProc]{
    param([IntPtr]$hwnd, [IntPtr]$lparam)

    [uint32]$windowPid = 0
    [void][CCChanWin32Probe]::GetWindowThreadProcessId($hwnd, [ref]$windowPid)
    if ($windowPid -ne [uint32]$ProcessId -or -not [CCChanWin32Probe]::IsWindowVisible($hwnd)) {
      return $true
    }

    $length = [CCChanWin32Probe]::GetWindowTextLength($hwnd)
    $title = New-Object System.Text.StringBuilder ([Math]::Max(1, $length + 1))
    [void][CCChanWin32Probe]::GetWindowText($hwnd, $title, $title.Capacity)

    $rect = New-Object RECT
    [void][CCChanWin32Probe]::GetWindowRect($hwnd, [ref]$rect)
    $exStyle = [CCChanWin32Probe]::GetWindowLongPtr($hwnd, [CCChanWin32Probe]::GWL_EXSTYLE).ToInt64()

    $windows.Add([pscustomobject]@{
      handle = ("0x{0:X}" -f $hwnd.ToInt64())
      title = $title.ToString()
      left = $rect.Left
      top = $rect.Top
      width = $rect.Right - $rect.Left
      height = $rect.Bottom - $rect.Top
      topMost = (($exStyle -band [CCChanWin32Probe]::WS_EX_TOPMOST) -ne 0)
    }) | Out-Null

    return $true
  }

  [void][CCChanWin32Probe]::EnumWindows($callback, [IntPtr]::Zero)
  return $windows.ToArray()
}

$devProcess = Get-Process cc-panes -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -eq $DevExePath } |
  Sort-Object StartTime -Descending |
  Select-Object -First 1

if (-not $devProcess) {
  throw "Dev cc-panes process not found at '$DevExePath'. Start 'npm run tauri:dev' first."
}

$windows = Get-VisibleWindowsForProcess -ProcessId $devProcess.Id
$mascot = $windows | Where-Object { $_.width -eq 120 -and $_.height -eq 120 -and $_.topMost } | Select-Object -First 1
$mainWindow = $windows | Where-Object { $_.width -gt 300 -and $_.height -gt 300 -and -not $_.topMost } | Select-Object -First 1

$protocolPath = "HKCU:\Software\Classes\$ProtocolScheme\shell\open\command"
$protocolCommand = $null
if (Test-Path $protocolPath) {
  $protocolItem = Get-ItemProperty -Path $protocolPath -Name "(default)" -ErrorAction SilentlyContinue
  $protocolCommand = $protocolItem."(default)"
}
$expectedProtocolCommand = '"' + $DevExePath + '" "%1"'
$protocolMatchesDevExe = ($protocolCommand -eq $expectedProtocolCommand)

$configValues = @()
if (Test-Path $DevConfigPath) {
  $configValues = Get-Content $DevConfigPath |
    Select-String -Pattern "windowVisible|windowX|windowY" |
    ForEach-Object { $_.Line }
}

$screens = [System.Windows.Forms.Screen]::AllScreens | ForEach-Object {
  [pscustomobject]@{
    primary = $_.Primary
    left = $_.Bounds.Left
    top = $_.Bounds.Top
    width = $_.Bounds.Width
    height = $_.Bounds.Height
    workingLeft = $_.WorkingArea.Left
    workingTop = $_.WorkingArea.Top
    workingWidth = $_.WorkingArea.Width
    workingHeight = $_.WorkingArea.Height
  }
}

$mascotInsideScreen = $false
if ($mascot) {
  foreach ($screen in $screens) {
    $screenRight = $screen.left + $screen.width
    $screenBottom = $screen.top + $screen.height
    if (
      $mascot.left -ge $screen.left -and
      $mascot.top -ge $screen.top -and
      ($mascot.left + $mascot.width) -le $screenRight -and
      ($mascot.top + $mascot.height) -le $screenBottom
    ) {
      $mascotInsideScreen = $true
      break
    }
  }
}

$result = [pscustomobject]@{
  ok = [bool]($mascot -and $mainWindow -and $protocolMatchesDevExe -and $mascotInsideScreen)
  devPid = $devProcess.Id
  devPath = $devProcess.Path
  mascotWindow = $mascot
  mascotInsideScreen = $mascotInsideScreen
  mainWindow = $mainWindow
  visibleWindows = $windows
  protocolCommand = $protocolCommand
  expectedProtocolCommand = $expectedProtocolCommand
  protocolMatchesDevExe = $protocolMatchesDevExe
  configPath = $DevConfigPath
  configValues = $configValues
  screenCount = @($screens).Count
  screens = $screens
}

$result | ConvertTo-Json -Depth 6

if (-not $result.ok) {
  exit 1
}
