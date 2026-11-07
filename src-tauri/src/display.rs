//! The video outputs (monitors): which one is the main display, and whether several screens are
//! extended or duplicated.
//!
//! Games and Lumen open on the main (primary) display, so choosing the main display chooses where
//! they appear. The screen mode is Windows' own "Project" setting. Like `audio.rs`, this drives the
//! Windows API from PowerShell with C# compiled on the fly, so the project needs no extra crates.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    /// The Windows device name, like `\\.\DISPLAY1`.
    pub id: String,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoState {
    pub displays: Vec<Display>,
    /// `extend`, `clone` (duplicate), `internal` (PC screen only), `external` (second screen only) or `unknown`.
    pub mode: String,
}

/// The screen modes Lumen can switch to, as `DisplaySwitch.exe` knows them.
const MODES: &[&str] = &["extend", "clone", "internal", "external"];

#[cfg(windows)]
const CSHARP: &str = r#"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class Video {
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  struct DISPLAY_DEVICE {
    public int cb;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string DeviceName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceString;
    public int StateFlags;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceID;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceKey;
  }
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  struct DEVMODE {
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
    public short dmSpecVersion, dmDriverVersion, dmSize, dmDriverExtra;
    public int dmFields;
    public int dmPositionX, dmPositionY, dmDisplayOrientation, dmDisplayFixedOutput;
    public short dmColor, dmDuplex, dmYResolution, dmTTOption, dmCollate;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
    public short dmLogPixels;
    public int dmBitsPerPel, dmPelsWidth, dmPelsHeight, dmDisplayFlags, dmDisplayFrequency, dmICMMethod, dmICMIntent, dmMediaType, dmDitherType, dmReserved1, dmReserved2, dmPanningWidth, dmPanningHeight;
  }
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern bool EnumDisplayDevices(string device, uint index, ref DISPLAY_DEVICE dd, uint flags);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern bool EnumDisplaySettings(string device, int mode, ref DEVMODE dm);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int ChangeDisplaySettingsEx(string device, ref DEVMODE dm, IntPtr hwnd, uint flags, IntPtr lp);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int ChangeDisplaySettingsEx(string device, IntPtr dm, IntPtr hwnd, uint flags, IntPtr lp);
  [DllImport("user32.dll")] static extern int GetDisplayConfigBufferSizes(uint flags, out uint paths, out uint modes);
  [DllImport("user32.dll")] static extern int QueryDisplayConfig(uint flags, ref uint paths, byte[] pathInfo, ref uint modes, byte[] modeInfo, out uint topology);

  const int ATTACHED = 0x1, PRIMARY = 0x4;
  const uint SET_PRIMARY = 0x10, UPDATE_REGISTRY = 0x1, NO_RESET = 0x10000000;

  static DEVMODE Current(string name) {
    var dm = new DEVMODE();
    dm.dmSize = (short)Marshal.SizeOf(typeof(DEVMODE));
    EnumDisplaySettings(name, -1, ref dm);
    return dm;
  }

  static List<DISPLAY_DEVICE> Attached() {
    var list = new List<DISPLAY_DEVICE>();
    for (uint i = 0; ; i++) {
      var dd = new DISPLAY_DEVICE(); dd.cb = Marshal.SizeOf(typeof(DISPLAY_DEVICE));
      if (!EnumDisplayDevices(null, i, ref dd, 0)) break;
      if ((dd.StateFlags & ATTACHED) != 0) list.Add(dd);
    }
    return list;
  }

  // One display per line: name <TAB> monitor code <TAB> adapter text <TAB> width <TAB> height <TAB> 1 if primary.
  public static string List() {
    var sb = new StringBuilder();
    foreach (var d in Attached()) {
      var mon = new DISPLAY_DEVICE(); mon.cb = Marshal.SizeOf(typeof(DISPLAY_DEVICE));
      string code = "";
      if (EnumDisplayDevices(d.DeviceName, 0, ref mon, 0)) {
        var p = (mon.DeviceID ?? "").Split('\\');
        if (p.Length > 1) code = p[1];
      }
      var dm = Current(d.DeviceName);
      sb.Append(d.DeviceName).Append('\t').Append(code).Append('\t').Append(d.DeviceString).Append('\t')
        .Append(dm.dmPelsWidth).Append('\t').Append(dm.dmPelsHeight).Append('\t').Append((d.StateFlags & PRIMARY) != 0 ? "1" : "0").Append('\n');
    }
    return sb.ToString();
  }

  // 1 internal, 2 clone (duplicate), 4 extend, 8 external; 0 if unknown.
  public static int Mode() {
    uint np, nm;
    if (GetDisplayConfigBufferSizes(4, out np, out nm) != 0) return 0;
    var paths = new byte[np * 72]; var modes = new byte[nm * 64]; uint topo;
    if (QueryDisplayConfig(4, ref np, paths, ref nm, modes, out topo) != 0) return 0;
    return (int)topo;
  }

  // Makes `name` the main display. Windows wants the main display at 0,0, so every active display
  // moves by the same offset, which keeps their arrangement.
  public static int SetPrimary(string name) {
    var all = Attached();
    DISPLAY_DEVICE? target = null;
    foreach (var d in all) if (d.DeviceName == name) target = d;
    if (target == null) return -100;
    var t = Current(name);
    int ox = t.dmPositionX, oy = t.dmPositionY;
    foreach (var d in all) {
      if (d.DeviceName == name) continue;
      var dm = Current(d.DeviceName);
      dm.dmPositionX -= ox; dm.dmPositionY -= oy;
      int r = ChangeDisplaySettingsEx(d.DeviceName, ref dm, IntPtr.Zero, UPDATE_REGISTRY | NO_RESET, IntPtr.Zero);
      if (r != 0) return r;
    }
    t.dmPositionX = 0; t.dmPositionY = 0;
    int rt = ChangeDisplaySettingsEx(name, ref t, IntPtr.Zero, SET_PRIMARY | UPDATE_REGISTRY | NO_RESET, IntPtr.Zero);
    if (rt != 0) return rt;
    return ChangeDisplaySettingsEx(null, IntPtr.Zero, IntPtr.Zero, 0, IntPtr.Zero);
  }
}
"#;

/// PowerShell that prints the state as `OK:<json>`; monitor models come from WMI, matched by their code.
#[cfg(windows)]
const LIST_SCRIPT: &str = r#"
$names = @{}
Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorID -ErrorAction SilentlyContinue | ForEach-Object {
  $code = ($_.InstanceName -split '\\')[1]
  $n = -join ($_.UserFriendlyName | Where-Object { $_ } | ForEach-Object { [char]$_ })
  if ($code -and $n) { $names[$code] = $n }
}
$i = 0
$rows = @([Video]::List() -split "`n" | Where-Object { $_ } | ForEach-Object {
  $i++
  $dev, $code, $text, $w, $h, $pri = $_ -split "`t"
  $name = if ($names[$code]) { $names[$code] } elseif ($text -and $text -notmatch 'Generic PnP|^Default') { $text } else { '' }
  [pscustomobject]@{ id = $dev; name = $name; width = [int]$w; height = [int]$h; primary = ($pri -eq '1') }
})
$mode = switch ([Video]::Mode()) { 1 { 'internal' } 2 { 'clone' } 4 { 'extend' } 8 { 'external' } default { 'unknown' } }
[Console]::Out.Write('OK:' + (ConvertTo-Json -InputObject ([pscustomobject]@{ displays = $rows; mode = $mode }) -Compress -Depth 4))
"#;

#[cfg(windows)]
fn run(body: &str) -> Result<String, String> {
    let script = format!("$ErrorActionPreference='Stop'\nAdd-Type -TypeDefinition @'\n{CSHARP}\n'@\ntry {{\n{body}\n}} catch {{ [Console]::Out.Write('ERR:' + $_) }}");
    let out = crate::platform::powershell(&script)?;
    let out = out.trim().to_owned();
    match out.strip_prefix("OK") {
        Some(ok) => Ok(ok.trim_start_matches(':').to_owned()),
        None => {
            let e = out.strip_prefix("ERR:").unwrap_or(&out).trim().to_owned();
            Err(crate::t!(format!("Could not use the video outputs: {e}"), format!("Nu am putut folosi ieșirile video: {e}")))
        }
    }
}

/// Fills in a name for displays whose model is unknown: `Display 1`, `Display 2`...
fn name_displays(state: &mut VideoState) {
    for (i, d) in state.displays.iter_mut().enumerate() {
        if d.name.trim().is_empty() {
            d.name = crate::t!(format!("Display {}", i + 1), format!("Ecran {}", i + 1));
        }
    }
}

/// The active displays and the current screen mode.
#[cfg(windows)]
pub fn state() -> Result<VideoState, String> {
    let json = run(LIST_SCRIPT)?;
    let mut st: VideoState = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    name_displays(&mut st);
    Ok(st)
}

/// `\\.\DISPLAY<number>`: the only form accepted, since the id ends up in a script.
fn valid_id(id: &str) -> bool {
    id.strip_prefix(r"\\.\DISPLAY").is_some_and(|n| !n.is_empty() && n.len() < 4 && n.chars().all(|c| c.is_ascii_digit()))
}

/// Makes a display the main one, so Lumen and the games open on it.
#[cfg(windows)]
pub fn set_primary(id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err(crate::t!("Invalid display.", "Ecran nevalid.").into());
    }
    run(&format!("$r = [Video]::SetPrimary('{id}'); if ($r -eq 0) {{ [Console]::Out.Write('OK') }} else {{ [Console]::Out.Write('ERR:code ' + $r) }}")).map(|_| ())
}

/// Switches how several screens are used: `extend` or `clone` (duplicate), also `internal` / `external`.
#[cfg(windows)]
pub fn set_mode(mode: &str) -> Result<(), String> {
    if !MODES.contains(&mode) {
        return Err(crate::t!("Invalid screen mode.", "Mod de ecran nevalid.").into());
    }
    let exe = std::path::Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join("System32").join("DisplaySwitch.exe");
    let status = crate::platform::hidden(&exe.to_string_lossy())
        .arg(format!("/{mode}"))
        .status()
        .map_err(|e| crate::t!(format!("Could not change the screen mode: {e}"), format!("Nu am putut schimba modul ecranelor: {e}")))?;
    let _ = status; // DisplaySwitch returns at once; the change lands a moment later.
    Ok(())
}

#[cfg(not(windows))]
pub fn state() -> Result<VideoState, String> {
    Ok(VideoState { displays: Vec::new(), mode: "unknown".into() })
}

#[cfg(not(windows))]
pub fn set_primary(_id: &str) -> Result<(), String> {
    Err(crate::t!("Only available on Windows.", "Disponibil doar pe Windows.").into())
}

#[cfg(not(windows))]
pub fn set_mode(_mode: &str) -> Result<(), String> {
    Err(crate::t!("Only available on Windows.", "Disponibil doar pe Windows.").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_display_device_names_are_accepted() {
        assert!(valid_id(r"\\.\DISPLAY1"));
        assert!(valid_id(r"\\.\DISPLAY12"));
        assert!(!valid_id(r"\\.\DISPLAY"));
        assert!(!valid_id(r"\\.\DISPLAY1'; calc #"));
        assert!(!valid_id("DISPLAY1"));
    }

    #[test]
    fn unnamed_displays_get_numbered_names() {
        let mut st = VideoState {
            displays: vec![
                Display { id: r"\\.\DISPLAY1".into(), name: "".into(), width: 1920, height: 1080, primary: true },
                Display { id: r"\\.\DISPLAY2".into(), name: "LG TV".into(), width: 3840, height: 2160, primary: false },
            ],
            mode: "extend".into(),
        };
        name_displays(&mut st);
        assert_eq!(st.displays[0].name, "Display 1");
        assert_eq!(st.displays[1].name, "LG TV");
    }
}

#[cfg(all(test, windows))]
mod live {
    /// Manual check on a real PC: `cargo test live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn state() {
        println!("{:#?}", super::state());
    }
}
