//! The Windows audio output devices and changing the default one.
//!
//! Windows has no public API for changing the default device; the only way (used by
//! Settings and by third-party utilities alike) is the undocumented COM interface
//! `IPolicyConfig`. We call it from a PowerShell script with C# compiled on the fly, so
//! we don't add crates or COM declarations to the project.

use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub current: bool,
}

#[cfg(windows)]
const CSHARP: &str = r#"
using System;
using System.Runtime.InteropServices;

[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class MMDeviceEnumeratorCom {}
[ComImport, Guid("870AF99C-171D-4F9E-AF0D-E63DF40C2BC9")] class PolicyConfigClient {}

[ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDeviceEnumerator {
  [PreserveSig] int EnumAudioEndpoints(int flow, int mask, out IMMDeviceCollection devices);
  [PreserveSig] int GetDefaultAudioEndpoint(int flow, int role, out IMMDevice device);
}
[ComImport, Guid("0BD7A1BE-7A1A-44DB-8397-CC5392387B5E"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDeviceCollection {
  [PreserveSig] int GetCount(out uint count);
  [PreserveSig] int Item(uint index, out IMMDevice device);
}
[ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDevice {
  [PreserveSig] int Activate(ref Guid iid, int ctx, IntPtr p, [MarshalAs(UnmanagedType.IUnknown)] out object o);
  [PreserveSig] int OpenPropertyStore(int access, out IPropertyStore store);
  [PreserveSig] int GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);
}
[ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IPropertyStore {
  [PreserveSig] int GetCount(out uint count);
  [PreserveSig] int GetAt(uint index, out PropKey key);
  [PreserveSig] int GetValue(ref PropKey key, out PropVariant value);
}
[ComImport, Guid("F8679F50-850A-41CF-9C72-430F290290C8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IPolicyConfig {
  [PreserveSig] int GetMixFormat(IntPtr a, IntPtr b);
  [PreserveSig] int GetDeviceFormat(IntPtr a, int b, IntPtr c);
  [PreserveSig] int ResetDeviceFormat(IntPtr a);
  [PreserveSig] int SetDeviceFormat(IntPtr a, IntPtr b, IntPtr c);
  [PreserveSig] int GetProcessingPeriod(IntPtr a, int b, IntPtr c, IntPtr d);
  [PreserveSig] int SetProcessingPeriod(IntPtr a, IntPtr b);
  [PreserveSig] int GetShareMode(IntPtr a, IntPtr b);
  [PreserveSig] int SetShareMode(IntPtr a, IntPtr b);
  [PreserveSig] int GetPropertyValue(IntPtr a, IntPtr b, IntPtr c);
  [PreserveSig] int SetPropertyValue(IntPtr a, IntPtr b, IntPtr c);
  [PreserveSig] int SetDefaultEndpoint([MarshalAs(UnmanagedType.LPWStr)] string id, int role);
  [PreserveSig] int SetEndpointVisibility(IntPtr a, int b);
}

[StructLayout(LayoutKind.Sequential)] struct PropKey { public Guid fmtid; public uint pid; }
[StructLayout(LayoutKind.Sequential)] struct PropVariant { public ushort vt, r1, r2, r3; public IntPtr p; public IntPtr q; }

public static class Audio {
  [DllImport("ole32.dll")] static extern int PropVariantClear(ref PropVariant v);

  static string Name(IMMDevice d) {
    IPropertyStore store;
    if (d.OpenPropertyStore(0, out store) != 0) return null;
    var key = new PropKey { fmtid = new Guid("a45c254e-df1c-4efd-8020-67d146a850e0"), pid = 14 };
    PropVariant v;
    if (store.GetValue(ref key, out v) != 0) return null;
    string s = v.vt == 31 ? Marshal.PtrToStringUni(v.p) : null;
    PropVariantClear(ref v);
    return s;
  }

  // One device per line: id <TAB> name <TAB> 1 if it is the default.
  public static string List() {
    var en = (IMMDeviceEnumerator)new MMDeviceEnumeratorCom();
    string cur = "";
    IMMDevice def;
    if (en.GetDefaultAudioEndpoint(0, 1, out def) == 0) def.GetId(out cur);
    IMMDeviceCollection col;
    if (en.EnumAudioEndpoints(0, 1, out col) != 0) return "";
    uint n; col.GetCount(out n);
    var sb = new System.Text.StringBuilder();
    for (uint i = 0; i < n; i++) {
      IMMDevice d; string id;
      if (col.Item(i, out d) != 0 || d.GetId(out id) != 0) continue;
      sb.Append(id).Append('\t').Append(Name(d) ?? id).Append('\t').Append(id == cur ? "1" : "0").Append('\n');
    }
    return sb.ToString();
  }

  // Console, Multimedia and Communications, so the whole system changes.
  public static int Set(string id) {
    var pc = (IPolicyConfig)new PolicyConfigClient();
    for (int role = 0; role < 3; role++) {
      int hr = pc.SetDefaultEndpoint(id, role);
      if (hr != 0) return hr;
    }
    return 0;
  }
}
"#;

#[cfg(windows)]
fn script(body: &str) -> String {
    format!("$ErrorActionPreference='Stop'\nAdd-Type -TypeDefinition @'\n{CSHARP}\n'@\n{body}")
}

/// The active playback devices, with the default one marked.
#[cfg(windows)]
pub fn outputs() -> Result<Vec<AudioDevice>, String> {
    let out = crate::platform::powershell(&script("try { [Console]::Out.Write([Audio]::List()) } catch { [Console]::Out.Write('ERR:' + $_) }"))?;
    if let Some(e) = out.trim().strip_prefix("ERR:") {
        return Err(crate::t!(format!("Could not read the audio devices: {e}"), format!("Nu am putut citi dispozitivele audio: {e}")));
    }
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut p = l.trim_end_matches('\r').split('\t');
            let (id, name, cur) = (p.next()?, p.next()?, p.next()?);
            Some(AudioDevice { id: id.into(), name: name.into(), current: cur == "1" })
        })
        .collect())
}

/// Makes `id` the default playback device for the whole system.
#[cfg(windows)]
pub fn set_output(id: &str) -> Result<(), String> {
    // The id ends up in a script, so we only accept the `{0.0.0.00000000}.{guid}` form.
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '{' | '}' | '.' | '-')) {
        return Err(crate::t!("Invalid audio device.", "Dispozitiv audio nevalid.").into());
    }
    let body = format!(
        "try {{ $r = [Audio]::Set('{id}'); if ($r -eq 0) {{ [Console]::Out.Write('OK') }} else {{ [Console]::Out.Write('ERR:0x{{0:X8}}' -f $r) }} }} catch {{ [Console]::Out.Write('ERR:' + $_) }}"
    );
    let out = crate::platform::powershell(&script(&body))?;
    match out.trim() {
        "OK" => Ok(()),
        e => {
            let e = e.trim_start_matches("ERR:");
            Err(crate::t!(format!("Could not change the audio output: {e}"), format!("Nu am putut schimba ieșirea audio: {e}")))
        }
    }
}

#[cfg(not(windows))]
pub fn outputs() -> Result<Vec<AudioDevice>, String> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub fn set_output(_id: &str) -> Result<(), String> {
    Err(crate::t!("Only available on Windows.", "Disponibil doar pe Windows.").into())
}
