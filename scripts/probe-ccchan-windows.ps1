param(
  [string]$DevExePath = "D:\my-project\cc-pane\target\debug\cc-panes.exe",
  [string]$ProtocolScheme = "ccpanes",
  [string]$DevConfigPath = "$env:USERPROFILE\.cc-panes-dev\config.toml",
  [switch]$VerifyDrag,
  [switch]$VerifyTrayToggle,
  [string]$TrayTooltip = "CC-Panes [DEV]"
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
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT lpPoint);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
  [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  public const int GWL_EXSTYLE = -20;
  public const long WS_EX_TOPMOST = 0x00000008L;
  public const int SW_RESTORE = 9;
  public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
  public const uint MOUSEEVENTF_LEFTUP = 0x0004;
  public const uint MOUSEEVENTF_RIGHTDOWN = 0x0008;
  public const uint MOUSEEVENTF_RIGHTUP = 0x0010;
}

[StructLayout(LayoutKind.Sequential)]
public struct RECT {
  public int Left;
  public int Top;
  public int Right;
  public int Bottom;
}

[StructLayout(LayoutKind.Sequential)]
public struct POINT {
  public int X;
  public int Y;
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
      hwnd = $hwnd
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

function Get-MainWindowForProcess {
  param(
    [int]$ProcessId,
    [string]$Title = "CC-Panes [DEV]"
  )

  $windows = Get-VisibleWindowsForProcess -ProcessId $ProcessId
  $main = $windows |
    Where-Object { $_.title -eq $Title -and -not $_.topMost } |
    Select-Object -First 1

  if (-not $main) {
    $main = $windows |
      Where-Object { $_.width -gt 300 -and $_.height -gt 300 -and -not $_.topMost } |
      Select-Object -First 1
  }

  return [pscustomobject]@{
    main = $main
    windows = $windows
  }
}

function Restore-MainWindowForProcess {
  param(
    [int]$ProcessId,
    [string]$Title = "CC-Panes [DEV]"
  )

  $probe = Get-MainWindowForProcess -ProcessId $ProcessId -Title $Title
  $main = $probe.main
  if (-not $main) {
    return $probe
  }

  if ($main.width -le 300 -or $main.height -le 300) {
    [void][CCChanWin32Probe]::ShowWindowAsync($main.hwnd, [CCChanWin32Probe]::SW_RESTORE)
    Start-Sleep -Milliseconds 500
  }

  $probe = Get-MainWindowForProcess -ProcessId $ProcessId -Title $Title
  if ($probe.main) {
    [void][CCChanWin32Probe]::SetForegroundWindow($probe.main.hwnd)
    Start-Sleep -Milliseconds 150
    $probe = Get-MainWindowForProcess -ProcessId $ProcessId -Title $Title
  }

  return $probe
}

function Get-MascotWindow {
  param([int]$ProcessId)

  $windows = Get-VisibleWindowsForProcess -ProcessId $ProcessId
  $mascot = $windows | Where-Object { $_.width -eq 120 -and $_.height -eq 120 -and $_.topMost } | Select-Object -First 1
  return [pscustomobject]@{
    mascot = $mascot
    windows = $windows
  }
}

function Read-CCChanConfigPosition {
  param([string]$Path)

  if (-not (Test-Path $Path)) {
    return $null
  }
  $content = Get-Content $Path
  $xLine = $content | Select-String -Pattern '^\s*windowX\s*=' | Select-Object -Last 1
  $yLine = $content | Select-String -Pattern '^\s*windowY\s*=' | Select-Object -Last 1
  if (-not $xLine -or -not $yLine) {
    return $null
  }
  $xText = ($xLine.Line -replace '^\s*windowX\s*=\s*', '').Trim()
  $yText = ($yLine.Line -replace '^\s*windowY\s*=\s*', '').Trim()
  return [pscustomobject]@{
    x = [double]::Parse($xText, [System.Globalization.CultureInfo]::InvariantCulture)
    y = [double]::Parse($yText, [System.Globalization.CultureInfo]::InvariantCulture)
  }
}

function Read-CCChanConfigVisible {
  param([string]$Path)

  if (-not (Test-Path $Path)) {
    return $null
  }
  $line = Get-Content $Path |
    Select-String -Pattern '^\s*windowVisible\s*=' |
    Select-Object -Last 1
  if (-not $line) {
    return $null
  }
  $value = ($line.Line -replace '^\s*windowVisible\s*=\s*', '').Trim().ToLowerInvariant()
  if ($value -eq "true") {
    return $true
  }
  if ($value -eq "false") {
    return $false
  }
  return $null
}

function Test-MascotInsideScreens {
  param(
    [object]$Mascot,
    [object[]]$Screens
  )

  if (-not $Mascot) {
    return $false
  }

  foreach ($screen in $Screens) {
    $screenRight = $screen.left + $screen.width
    $screenBottom = $screen.top + $screen.height
    if (
      $Mascot.left -ge $screen.left -and
      $Mascot.top -ge $screen.top -and
      ($Mascot.left + $Mascot.width) -le $screenRight -and
      ($Mascot.top + $Mascot.height) -le $screenBottom
    ) {
      return $true
    }
  }
  return $false
}

function Wait-CCChanMascotState {
  param(
    [int]$ProcessId,
    [bool]$Visible,
    [int]$TimeoutMs = 4000
  )

  $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
  do {
    $probe = Get-MascotWindow -ProcessId $ProcessId
    $hasMascot = [bool]$probe.mascot
    if ($hasMascot -eq $Visible) {
      return $probe
    }
    Start-Sleep -Milliseconds 150
  } while ((Get-Date) -lt $deadline)

  return Get-MascotWindow -ProcessId $ProcessId
}

function Initialize-UIAutomation {
  if (-not ("System.Windows.Automation.AutomationElement" -as [type])) {
    Add-Type -AssemblyName UIAutomationTypes
    Add-Type -AssemblyName UIAutomationClient
  }
}

function Get-AutomationRootElements {
  Initialize-UIAutomation
  $root = [System.Windows.Automation.AutomationElement]::RootElement
  return $root.FindAll(
    [System.Windows.Automation.TreeScope]::Children,
    [System.Windows.Automation.Condition]::TrueCondition
  )
}

function Find-AutomationElementsByNamePart {
  param(
    [string]$NamePart,
    [string[]]$RootClassNames = @()
  )

  Initialize-UIAutomation
  $matches = New-Object System.Collections.Generic.List[object]
  foreach ($rootElement in Get-AutomationRootElements) {
    if ($RootClassNames.Count -gt 0 -and $RootClassNames -notcontains $rootElement.Current.ClassName) {
      continue
    }

    $elements = $rootElement.FindAll(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.Condition]::TrueCondition
    )
    foreach ($element in $elements) {
      $name = $element.Current.Name
      if ($name -and $name.IndexOf($NamePart, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
        $rect = $element.Current.BoundingRectangle
        if (-not $element.Current.IsOffscreen -and $rect.Width -gt 0 -and $rect.Height -gt 0) {
          $matches.Add($element) | Out-Null
        }
      }
    }
  }
  return $matches.ToArray()
}

function Find-TrayOverflowButtonByLayout {
  Initialize-UIAutomation
  foreach ($rootElement in Get-AutomationRootElements) {
    if ($rootElement.Current.ClassName -ne "Shell_TrayWnd") {
      continue
    }

    $elements = $rootElement.FindAll(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.Condition]::TrueCondition
    )
    $trayNotify = $null
    foreach ($element in $elements) {
      if ($element.Current.ClassName -eq "TrayNotifyWnd" -and -not $element.Current.IsOffscreen) {
        $trayNotify = $element
        break
      }
    }
    if (-not $trayNotify) {
      continue
    }

    $trayRect = $trayNotify.Current.BoundingRectangle
    foreach ($element in $elements) {
      if ($element.Current.ClassName -ne "SystemTray.NormalButton" -or $element.Current.IsOffscreen) {
        continue
      }

      $rect = $element.Current.BoundingRectangle
      $nearTrayStart = $rect.Left -ge ($trayRect.Left - 2) -and $rect.Left -le ($trayRect.Left + 48)
      if ($nearTrayStart -and $rect.Width -gt 0 -and $rect.Height -gt 0) {
        return $element
      }
    }
  }

  return $null
}

function Click-AutomationElement {
  param(
    [object]$Element,
    [ValidateSet("Left", "Right")]
    [string]$Button = "Left"
  )

  $rect = $Element.Current.BoundingRectangle
  if ($rect.Width -le 0 -or $rect.Height -le 0) {
    throw "Automation element has no clickable bounds."
  }

  $x = [int]($rect.Left + [Math]::Floor($rect.Width / 2))
  $y = [int]($rect.Top + [Math]::Floor($rect.Height / 2))
  [void][CCChanWin32Probe]::SetCursorPos($x, $y)
  Start-Sleep -Milliseconds 120

  if ($Button -eq "Right") {
    [CCChanWin32Probe]::mouse_event([CCChanWin32Probe]::MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [CCChanWin32Probe]::mouse_event([CCChanWin32Probe]::MOUSEEVENTF_RIGHTUP, 0, 0, 0, [UIntPtr]::Zero)
  } else {
    [CCChanWin32Probe]::mouse_event([CCChanWin32Probe]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [CCChanWin32Probe]::mouse_event([CCChanWin32Probe]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
  }
}

function Open-TrayOverflowIfAvailable {
  $layoutButton = Find-TrayOverflowButtonByLayout
  if ($layoutButton) {
    Click-AutomationElement -Element $layoutButton -Button "Left"
    Start-Sleep -Milliseconds 500
    return $true
  }

  $labelZhWithDe = -join ([char[]](0x663E, 0x793A, 0x9690, 0x85CF, 0x7684, 0x56FE, 0x6807))
  $labelZhWithoutDe = -join ([char[]](0x663E, 0x793A, 0x9690, 0x85CF, 0x56FE, 0x6807))
  $labels = @("Show hidden icons", $labelZhWithDe, $labelZhWithoutDe)
  foreach ($label in $labels) {
    $button = Find-AutomationElementsByNamePart -NamePart $label -RootClassNames @("Shell_TrayWnd") |
      Select-Object -First 1
    if ($button) {
      Click-AutomationElement -Element $button -Button "Left"
      Start-Sleep -Milliseconds 500
      return $true
    }
  }
  return $false
}

function Find-CCPanesTrayIcon {
  param([string]$Tooltip)

  $rootClasses = @("Shell_TrayWnd", "NotifyIconOverflowWindow")
  $nameParts = @($Tooltip, "CC-Panes", "cc-panes") |
    Where-Object { $_ } |
    Select-Object -Unique

  foreach ($namePart in $nameParts) {
    $icon = Find-AutomationElementsByNamePart -NamePart $namePart -RootClassNames $rootClasses |
      Select-Object -First 1
    if ($icon) {
      return $icon
    }
  }

  [void](Open-TrayOverflowIfAvailable)
  foreach ($namePart in $nameParts) {
    $icon = Find-AutomationElementsByNamePart -NamePart $namePart -RootClassNames $rootClasses |
      Select-Object -First 1
    if ($icon) {
      return $icon
    }
  }

  return $null
}

function Invoke-CCChanTrayMenuToggle {
  param([string]$Tooltip)

  $icon = Find-CCPanesTrayIcon -Tooltip $Tooltip
  if (-not $icon) {
    throw "Could not find a visible tray icon with tooltip containing '$Tooltip'. Pin the icon or open the overflow menu before using -VerifyTrayToggle."
  }

  Click-AutomationElement -Element $icon -Button "Right"
  Start-Sleep -Milliseconds 500

  $menuItemName = "Show/Hide cc" + [char]0x9171
  $menuItem = Find-AutomationElementsByNamePart -NamePart $menuItemName |
    Select-Object -First 1
  if (-not $menuItem) {
    throw "Could not find tray menu item '$menuItemName' after opening the tray menu."
  }

  Click-AutomationElement -Element $menuItem -Button "Left"
  Start-Sleep -Milliseconds 500
}

function Test-CCChanTrayToggle {
  param(
    [int]$ProcessId,
    [object]$InitialMascot,
    [string]$ConfigPath,
    [string]$Tooltip
  )

  if (-not $InitialMascot) {
    throw "Cannot verify tray toggle without a visible mascot window."
  }

  $originalCursor = New-Object POINT
  [void][CCChanWin32Probe]::GetCursorPos([ref]$originalCursor)

  try {
    Invoke-CCChanTrayMenuToggle -Tooltip $Tooltip
    $afterHideProbe = Wait-CCChanMascotState -ProcessId $ProcessId -Visible $false
    $configAfterHide = Read-CCChanConfigVisible -Path $ConfigPath
    $hideOk = [bool]((-not $afterHideProbe.mascot) -and $configAfterHide -eq $false)

    Invoke-CCChanTrayMenuToggle -Tooltip $Tooltip
    $afterShowProbe = Wait-CCChanMascotState -ProcessId $ProcessId -Visible $true
    $configAfterShow = Read-CCChanConfigVisible -Path $ConfigPath
    $showOk = [bool]($afterShowProbe.mascot -and $configAfterShow -eq $true)

    return [pscustomobject]@{
      attempted = $true
      tooltip = $Tooltip
      hideOk = $hideOk
      showOk = $showOk
      before = $InitialMascot
      afterHide = $afterHideProbe.mascot
      afterShow = $afterShowProbe.mascot
      configAfterHide = $configAfterHide
      configAfterShow = $configAfterShow
    }
  } finally {
    [CCChanWin32Probe]::mouse_event([CCChanWin32Probe]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    [CCChanWin32Probe]::mouse_event([CCChanWin32Probe]::MOUSEEVENTF_RIGHTUP, 0, 0, 0, [UIntPtr]::Zero)
    [void][CCChanWin32Probe]::SetCursorPos($originalCursor.X, $originalCursor.Y)
  }
}

function Test-CCChanDragPersistence {
  param(
    [int]$ProcessId,
    [object]$InitialMascot,
    [string]$ConfigPath
  )

  if (-not $InitialMascot) {
    throw "Cannot verify drag without a mascot window."
  }

  $originalCursor = New-Object POINT
  [void][CCChanWin32Probe]::GetCursorPos([ref]$originalCursor)

  $startX = [int]($InitialMascot.left + [Math]::Floor($InitialMascot.width / 2))
  $startY = [int]($InitialMascot.top + [Math]::Floor($InitialMascot.height / 2))
  $targetX = $startX + 48
  $targetY = $startY + 36

  try {
    [void][CCChanWin32Probe]::SetCursorPos($startX, $startY)
    Start-Sleep -Milliseconds 100
    [CCChanWin32Probe]::mouse_event([CCChanWin32Probe]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    foreach ($step in 1..8) {
      $nextX = [int]($startX + (($targetX - $startX) * $step / 8))
      $nextY = [int]($startY + (($targetY - $startY) * $step / 8))
      [void][CCChanWin32Probe]::SetCursorPos($nextX, $nextY)
      Start-Sleep -Milliseconds 40
    }
    Start-Sleep -Milliseconds 120
    [CCChanWin32Probe]::mouse_event([CCChanWin32Probe]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 650
  } finally {
    [CCChanWin32Probe]::mouse_event([CCChanWin32Probe]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    [void][CCChanWin32Probe]::SetCursorPos($originalCursor.X, $originalCursor.Y)
  }

  $afterProbe = Get-MascotWindow -ProcessId $ProcessId
  $after = $afterProbe.mascot
  $configPosition = Read-CCChanConfigPosition -Path $ConfigPath
  $moved = [bool]($after -and ([Math]::Abs($after.left - $InitialMascot.left) -ge 12 -or [Math]::Abs($after.top - $InitialMascot.top) -ge 12))
  $configMoved = [bool]($configPosition -and ([Math]::Abs($configPosition.x - $InitialMascot.left) -ge 12 -or [Math]::Abs($configPosition.y - $InitialMascot.top) -ge 12))

  return [pscustomobject]@{
    attempted = $true
    moved = $moved
    configMoved = $configMoved
    before = $InitialMascot
    after = $after
    configPosition = $configPosition
  }
}

$devProcess = Get-Process cc-panes -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -eq $DevExePath } |
  Sort-Object StartTime -Descending |
  Select-Object -First 1

if (-not $devProcess) {
  throw "Dev cc-panes process not found at '$DevExePath'. Start 'npm run tauri:dev' first."
}

$windowProbe = Get-MascotWindow -ProcessId $devProcess.Id
$windows = $windowProbe.windows
$mascot = $windowProbe.mascot
$mainProbe = Restore-MainWindowForProcess -ProcessId $devProcess.Id
$windows = Get-VisibleWindowsForProcess -ProcessId $devProcess.Id
$mainWindow = $mainProbe.main

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

$mascotInsideScreen = Test-MascotInsideScreens -Mascot $mascot -Screens $screens

$dragResult = $null
if ($VerifyDrag) {
  $dragResult = Test-CCChanDragPersistence -ProcessId $devProcess.Id -InitialMascot $mascot -ConfigPath $DevConfigPath
  $windowProbe = Get-MascotWindow -ProcessId $devProcess.Id
  $windows = $windowProbe.windows
  $mascot = $windowProbe.mascot
  $mainProbe = Restore-MainWindowForProcess -ProcessId $devProcess.Id
  $windows = Get-VisibleWindowsForProcess -ProcessId $devProcess.Id
  $mainWindow = $mainProbe.main
  if (Test-Path $DevConfigPath) {
    $configValues = Get-Content $DevConfigPath |
      Select-String -Pattern "windowVisible|windowX|windowY" |
      ForEach-Object { $_.Line }
  }
  $mascotInsideScreen = Test-MascotInsideScreens -Mascot $mascot -Screens $screens
}

$trayToggleResult = $null
if ($VerifyTrayToggle) {
  $trayToggleResult = Test-CCChanTrayToggle -ProcessId $devProcess.Id -InitialMascot $mascot -ConfigPath $DevConfigPath -Tooltip $TrayTooltip
  $windowProbe = Get-MascotWindow -ProcessId $devProcess.Id
  $windows = $windowProbe.windows
  $mascot = $windowProbe.mascot
  $mainProbe = Restore-MainWindowForProcess -ProcessId $devProcess.Id
  $windows = Get-VisibleWindowsForProcess -ProcessId $devProcess.Id
  $mainWindow = $mainProbe.main
  if (Test-Path $DevConfigPath) {
    $configValues = Get-Content $DevConfigPath |
      Select-String -Pattern "windowVisible|windowX|windowY" |
      ForEach-Object { $_.Line }
  }
  $mascotInsideScreen = Test-MascotInsideScreens -Mascot $mascot -Screens $screens
}

$dragOk = (-not $VerifyDrag) -or ($dragResult -and $dragResult.moved -and $dragResult.configMoved)
$trayToggleOk = (-not $VerifyTrayToggle) -or ($trayToggleResult -and $trayToggleResult.hideOk -and $trayToggleResult.showOk)

$result = [pscustomobject]@{
  ok = [bool]($mascot -and $mainWindow -and $mainWindow.width -gt 300 -and $mainWindow.height -gt 300 -and $protocolMatchesDevExe -and $mascotInsideScreen -and $dragOk -and $trayToggleOk)
  devPid = $devProcess.Id
  devPath = $devProcess.Path
  mascotWindow = $mascot
  mascotInsideScreen = $mascotInsideScreen
  dragVerification = $dragResult
  trayToggleVerification = $trayToggleResult
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
