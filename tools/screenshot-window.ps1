param(
    [Parameter(Mandatory = $true)][string]$Title,
    [Parameter(Mandatory = $true)][string]$Out
)
# Saves a PNG of the top-level window whose title is exactly $Title (physical pixels,
# copied from the screen, so the window must be visible and not covered).
$ErrorActionPreference = 'Stop'

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class Win32Shot {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindowW(string lpClassName, string lpWindowName);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
}
'@
Add-Type -AssemblyName System.Drawing

# Per-monitor-v2 awareness (-4) so GetWindowRect and CopyFromScreen both use physical px.
[void][Win32Shot]::SetProcessDpiAwarenessContext([IntPtr]::new(-4))

$hwnd = [Win32Shot]::FindWindowW([NullString]::Value, $Title)
if ($hwnd -eq [IntPtr]::Zero) {
    [Console]::Error.WriteLine("No window titled '$Title'")
    exit 1
}
$r = New-Object Win32Shot+RECT
if (-not [Win32Shot]::GetWindowRect($hwnd, [ref]$r)) {
    [Console]::Error.WriteLine("GetWindowRect failed for '$Title'")
    exit 1
}
$w = $r.Right - $r.Left
$h = $r.Bottom - $r.Top
if ($w -le 0 -or $h -le 0) {
    [Console]::Error.WriteLine("Window '$Title' has an empty rect")
    exit 1
}

$bmp = New-Object System.Drawing.Bitmap $w, $h
try {
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    try {
        $g.CopyFromScreen($r.Left, $r.Top, 0, 0, (New-Object System.Drawing.Size $w, $h))
    } finally { $g.Dispose() }
    $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
} finally { $bmp.Dispose() }
Write-Output "Saved ${w}x${h} to $Out"
