//! Lists, pairs and removes Bluetooth devices through the Windows Runtime device APIs
//! (`Windows.Devices.Enumeration`), driven from PowerShell so the project needs no extra crates.
//!
//! Paired devices are read from the system's cache, so listing is instant. Looking for new devices
//! runs a real radio inquiry and takes several seconds, so callers keep it off the main thread.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BtDevice {
    /// The Windows device id, used to pair or remove the device.
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub paired: bool,
}

/// Shared prelude: loads the WinRT types and an `Await` helper for `IAsyncOperation`.
const PRELUDE: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null = [Windows.Devices.Enumeration.DeviceInformation, Windows.Devices.Enumeration, ContentType = WindowsRuntime]
$null = [Windows.Devices.Enumeration.DeviceInformationCollection, Windows.Devices.Enumeration, ContentType = WindowsRuntime]
$null = [Windows.Devices.Enumeration.DevicePairingResult, Windows.Devices.Enumeration, ContentType = WindowsRuntime]
$null = [Windows.Devices.Enumeration.DeviceUnpairingResult, Windows.Devices.Enumeration, ContentType = WindowsRuntime]
$null = [Windows.Devices.Bluetooth.BluetoothDevice, Windows.Devices.Bluetooth, ContentType = WindowsRuntime]
$null = [Windows.Devices.Bluetooth.BluetoothLEDevice, Windows.Devices.Bluetooth, ContentType = WindowsRuntime]
$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]
function Await($op, $type, $ms) {
  $t = $asTask.MakeGenericMethod($type).Invoke($null, @($op))
  if (-not $t.Wait($ms)) { throw 'timeout' }
  $t.Result
}
$kind = [Windows.Devices.Enumeration.DeviceInformationKind]::AssociationEndpoint
$props = [string[]]@('System.Devices.Aep.IsConnected', 'System.Devices.Aep.IsPaired')
# Bluetooth Classic and Bluetooth LE paired devices. The dedicated selectors answer at once; a plain
# protocol-id query over all association endpoints can hang for good on some PCs.
$selectors = @([Windows.Devices.Bluetooth.BluetoothDevice]::GetDeviceSelectorFromPairingState($true), [Windows.Devices.Bluetooth.BluetoothLEDevice]::GetDeviceSelectorFromPairingState($true))
function Find($ms) {
  foreach ($s in $selectors) {
    $c = Await ([Windows.Devices.Enumeration.DeviceInformation]::FindAllAsync($s, $props)) ([Windows.Devices.Enumeration.DeviceInformationCollection]) $ms
    foreach ($d in $c) { $d }
  }
}
# The Aep.IsConnected property comes back empty for these selectors; the device object knows.
function Conn($d) {
  try {
    if ($d.Id -like 'BluetoothLE#*') { $o = Await ([Windows.Devices.Bluetooth.BluetoothLEDevice]::FromIdAsync($d.Id)) ([Windows.Devices.Bluetooth.BluetoothLEDevice]) 5000 }
    else { $o = Await ([Windows.Devices.Bluetooth.BluetoothDevice]::FromIdAsync($d.Id)) ([Windows.Devices.Bluetooth.BluetoothDevice]) 5000 }
    if ($null -eq $o) { throw 'no device object' }
    $o.ConnectionStatus -eq 'Connected'
  } catch { [bool]$d.Properties['System.Devices.Aep.IsConnected'] }
}
function Emit($devices) {
  $rows = @($devices | Where-Object { $_.Name } | ForEach-Object {
    [pscustomobject]@{ id = $_.Id; name = $_.Name; connected = (Conn $_); paired = [bool]$_.Pairing.IsPaired }
  })
  [Console]::Out.Write('OK:' + (ConvertTo-Json -InputObject $rows -Compress))
}
"#;

fn run(body: &str) -> Result<String, String> {
    let out = crate::platform::powershell(&format!("{PRELUDE}\ntry {{\n{body}\n}} catch {{ [Console]::Out.Write('ERR:' + $_.Exception.Message) }}"))?;
    let out = out.trim().to_owned();
    match out.strip_prefix("OK:") {
        Some(ok) => Ok(ok.to_owned()),
        None => {
            let why = out.strip_prefix("ERR:").unwrap_or("").trim().to_owned();
            Err(crate::t!(
                format!("Bluetooth is not available: {}", if why.is_empty() { "no answer from Windows".into() } else { why }),
                format!("Bluetooth nu este disponibil: {}", if why.is_empty() { "Windows nu a răspuns".into() } else { why })
            ))
        }
    }
}

/// Windows ids look like `Bluetooth#Bluetooth<adapter>-<device>`; anything else would end up in a script.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 200 && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '#' | ':' | '-' | '_' | '.' | '&' | '{' | '}' | '\\'))
}

/// Parses the JSON the scripts print, sorted with the connected devices first.
fn parse(json: &str) -> Vec<BtDevice> {
    #[derive(serde::Deserialize)]
    struct Row {
        id: String,
        name: String,
        connected: bool,
        paired: bool,
    }
    let mut rows: Vec<Row> = serde_json::from_str(json).unwrap_or_default();
    // A device can answer both as Classic and as LE (same address, two ids): keep the Classic one.
    rows.sort_by_key(|r| r.id.starts_with("BluetoothLE#"));
    // The duplicate may be the only one that is actually connected (e.g. a gamepad linked over LE),
    // so the survivor is connected when any of its twins is.
    let addr = |id: &str| id.rsplit('-').next().unwrap_or(id).to_owned();
    let connected_addrs: std::collections::HashSet<String> = rows.iter().filter(|r| r.connected).map(|r| addr(&r.id)).collect();
    let mut seen = std::collections::HashSet::new();
    let mut v: Vec<BtDevice> = rows
        .into_iter()
        .filter(|r| seen.insert(addr(&r.id)))
        .map(|r| {
            let connected = r.connected || connected_addrs.contains(&addr(&r.id));
            BtDevice { id: r.id, name: r.name, connected, paired: r.paired }
        })
        .collect();
    v.sort_by(|a, b| b.connected.cmp(&a.connected).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

/// The devices already paired with this PC.
pub fn paired() -> Result<Vec<BtDevice>, String> {
    let json = run("Emit (Find 20000 | Where-Object { $_.Pairing.IsPaired })")?;
    Ok(parse(&json))
}

/// C# that runs `DeviceWatcher`s for a while and returns the unpaired devices it saw, one `id<TAB>name`
/// per line. PowerShell cannot subscribe to Windows Runtime events itself, so this is compiled through
/// CodeDom (plain `Add-Type` refuses the `.winmd` references) with the same compiler.
const SCAN_CSHARP: &str = r#"
using System;
using System.Collections.Generic;
using System.Threading;
using Windows.Devices.Enumeration;
public static class BtScan {
  public static string Run(int ms) {
    var found = new Dictionary<string, string>();
    var watchers = new List<DeviceWatcher>();
    var props = new string[] { "System.ItemNameDisplay", "System.Devices.Aep.IsPaired" };
    // The dedicated selectors for unpaired Bluetooth Classic and LE devices (a plain protocol-id query can hang).
    foreach (var sel in new[] { Windows.Devices.Bluetooth.BluetoothDevice.GetDeviceSelectorFromPairingState(false), Windows.Devices.Bluetooth.BluetoothLEDevice.GetDeviceSelectorFromPairingState(false) }) {
      var w = DeviceInformation.CreateWatcher(sel, props, DeviceInformationKind.AssociationEndpoint);
      // A device often shows up without a name and gets it in a later update.
      w.Added += (s, d) => { lock (found) if (!d.Pairing.IsPaired) found[d.Id] = d.Name ?? ""; };
      w.Updated += (s, u) => {
        object n;
        if (u.Properties.TryGetValue("System.ItemNameDisplay", out n) && n is string && (string)n != "") lock (found) found[u.Id] = (string)n;
      };
      w.Start();
      watchers.Add(w);
    }
    Thread.Sleep(ms);
    foreach (var w in watchers) { try { w.Stop(); } catch { } }
    var sb = new System.Text.StringBuilder();
    lock (found) foreach (var kv in found) if (kv.Value != "") sb.Append(kv.Key).Append('\t').Append(kv.Value).Append('\n');
    return sb.ToString();
  }
}
// Pairing needs the live device object, which only a watcher hands out (looking a device up by id, or
// FindAllAsync for unpaired ones, comes back empty or never returns). So it finds the device again and pairs it.
public static class BtPair {
  public static string Run(string id, int findMs) {
    DeviceInformation found = null;
    string lastId = null;
    var seen = new ManualResetEvent(false);
    // Matched by the device's own address (the id ends with it), on both the Classic and the LE selector.
    Func<string, string> addr = x => x.Substring(x.LastIndexOf('-') + 1).ToLowerInvariant();
    var want = addr(id);
    var watchers = new System.Collections.Generic.List<DeviceWatcher>();
    foreach (var sel in new[] { Windows.Devices.Bluetooth.BluetoothDevice.GetDeviceSelectorFromPairingState(false), Windows.Devices.Bluetooth.BluetoothLEDevice.GetDeviceSelectorFromPairingState(false) }) {
      var w = DeviceInformation.CreateWatcher(sel, new string[] { "System.ItemNameDisplay" }, DeviceInformationKind.AssociationEndpoint);
      w.Added += (s, d) => { if (found == null && addr(d.Id) == want) { found = d; seen.Set(); } };
      // The device sometimes only shows up in an update (e.g. when it starts advertising a name).
      w.Updated += (s, u) => { if (found == null && addr(u.Id) == want) { lastId = u.Id; seen.Set(); } };
      w.Start();
      watchers.Add(w);
    }
    bool ok = seen.WaitOne(findMs);
    foreach (var w in watchers) { try { w.Stop(); } catch { } }
    if (!ok) return "NotFound";
    if (found == null) {
      // Seen only through an update: with the watchers' discovery just done, the stack can hand out the device now.
      var get = DeviceInformation.CreateFromIdAsync(lastId, new string[] { "System.ItemNameDisplay" }, DeviceInformationKind.AssociationEndpoint);
      for (int i = 0; i < 100 && get.Status == Windows.Foundation.AsyncStatus.Started; i++) Thread.Sleep(100);
      if (get.Status != Windows.Foundation.AsyncStatus.Completed) return "NotFound";
      found = get.GetResults();
    }
    if (found.Pairing.IsPaired) return "Paired";
    // Custom pairing with a handler: the plain PairAsync() is refused by devices that ask for a confirmation.
    var custom = found.Pairing.Custom;
    custom.PairingRequested += (s, a) => {
      if (a.PairingKind == DevicePairingKinds.ProvidePin) a.Accept("0000"); else a.Accept();
    };
    var op = custom.PairAsync(DevicePairingKinds.ConfirmOnly | DevicePairingKinds.DisplayPin | DevicePairingKinds.ProvidePin | DevicePairingKinds.ConfirmPinMatch, DevicePairingProtectionLevel.Default);
    for (int i = 0; i < 600 && op.Status == Windows.Foundation.AsyncStatus.Started; i++) Thread.Sleep(100);
    if (op.Status != Windows.Foundation.AsyncStatus.Completed) return "Timeout" + op.Status;
    return op.GetResults().Status.ToString();
  }
}
"#;

/// Compiles [`SCAN_CSHARP`] (`__CSHARP__`); the caller appends what to run.
const COMPILE_SCRIPT: &str = r#"
$src = @'
__CSHARP__
'@
$fw = [System.Runtime.InteropServices.RuntimeEnvironment]::GetRuntimeDirectory()
$p = New-Object System.CodeDom.Compiler.CompilerParameters
$p.GenerateInMemory = $true
$refs = @(
  'C:\Windows\System32\WinMetadata\Windows.Devices.winmd', 'C:\Windows\System32\WinMetadata\Windows.Foundation.winmd',
  "$fw\mscorlib.dll", "$fw\System.Runtime.dll", "$fw\System.Runtime.WindowsRuntime.dll", "$fw\System.Runtime.InteropServices.WindowsRuntime.dll"
)
foreach ($r in $refs) { [void]$p.ReferencedAssemblies.Add($r) }
$c = (New-Object Microsoft.CSharp.CSharpCodeProvider).CompileAssemblyFromSource($p, $src)
if ($c.Errors.HasErrors) { throw ($c.Errors | Select-Object -First 1).ToString() }
"#;

/// How long the radio is searched for new devices.
const SCAN_MS: u32 = 8000;

/// Devices in range that are not paired yet. Runs a radio inquiry, so it takes several seconds.
pub fn scan() -> Result<Vec<BtDevice>, String> {
    let body = format!(
        "{}\n{}",
        COMPILE_SCRIPT.replace("__CSHARP__", SCAN_CSHARP),
        r#"$out = $c.CompiledAssembly.GetType('BtScan').GetMethod('Run').Invoke($null, @(__MS__))
$rows = @($out -split "`n" | Where-Object { $_ } | ForEach-Object {
  $id, $name = $_ -split "`t", 2
  [pscustomobject]@{ id = $id; name = $name; connected = $false; paired = $false }
})
[Console]::Out.Write('OK:' + (ConvertTo-Json -InputObject $rows -Compress))"#
            .replace("__MS__", &SCAN_MS.to_string())
    );
    Ok(parse(&run(&body)?))
}

/// Pairs with a device found by [`scan`].
pub fn pair(id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err(crate::t!("Invalid Bluetooth device.", "Dispozitiv Bluetooth nevalid.").into());
    }
    let body = format!(
        "{}\n$s = $c.CompiledAssembly.GetType('BtPair').GetMethod('Run').Invoke($null, @('{id}', 30000))\n[Console]::Out.Write('OK:' + $s)",
        COMPILE_SCRIPT.replace("__CSHARP__", SCAN_CSHARP)
    );
    let status = run(&body)?;
    match status.as_str() {
        "Paired" | "AlreadyPaired" => Ok(()),
        "NotFound" => Err(crate::t!(
            "The device was not found. Put it in pairing mode and try again.",
            "Dispozitivul nu a fost găsit. Pune-l în modul de asociere și încearcă din nou."
        )
        .into()),
        "ConnectionRejected" | "Failed" | "AuthenticationFailure" | "AuthenticationTimeout" | "RejectedByHandler" => Err(crate::t!(
            "The device refused to pair. Put it in pairing mode and try again.",
            "Dispozitivul a refuzat asocierea. Pune-l în modul de asociere și încearcă din nou."
        )
        .into()),
        "RequiredHandlerNotRegistered" | "AccessDenied" => Err(crate::t!(
            "This device needs a PIN or confirmation. Pair it from the Windows Bluetooth settings instead.",
            "Acest dispozitiv cere un PIN sau o confirmare. Asociază-l din setările Bluetooth ale Windows."
        )
        .into()),
        other => Err(crate::t!(format!("Pairing failed ({other})."), format!("Asocierea a eșuat ({other}).")).into()),
    }
}

/// Removes (unpairs) a paired device.
pub fn remove(id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err(crate::t!("Invalid Bluetooth device.", "Dispozitiv Bluetooth nevalid.").into());
    }
    let status = run(&format!(
        "$d = Await ([Windows.Devices.Enumeration.DeviceInformation]::CreateFromIdAsync('{id}', $props, $kind)) ([Windows.Devices.Enumeration.DeviceInformation]) 20000\n\
         $r = Await ($d.Pairing.UnpairAsync()) ([Windows.Devices.Enumeration.DeviceUnpairingResult]) 30000\n\
         [Console]::Out.Write('OK:' + $r.Status)"
    ))?;
    match status.as_str() {
        "Unpaired" | "AlreadyUnpaired" => Ok(()),
        other => Err(crate::t!(format!("The device could not be removed ({other})."), format!("Dispozitivul nu a putut fi eliminat ({other}).")).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_checked_before_reaching_a_script() {
        assert!(valid_id("Bluetooth#Bluetoothe8:fb:1c:b3:50:92-a0:fa:9c:ae:84:4d"));
        assert!(!valid_id(""));
        assert!(!valid_id("x'; Remove-Item C:\\ -Recurse #"));
        assert!(!valid_id("a b"));
    }

    #[test]
    fn rows_are_sorted_and_deduplicated() {
        let json = r#"[{"id":"b","name":"Zeta","connected":false,"paired":true},{"id":"a","name":"Alpha","connected":false,"paired":true},{"id":"c","name":"Pad","connected":true,"paired":true},{"id":"a","name":"Alpha","connected":false,"paired":true}]"#;
        let names: Vec<_> = parse(json).into_iter().map(|d| d.name).collect();
        assert_eq!(names, ["Pad", "Alpha", "Zeta"]);
    }

    #[test]
    fn a_single_row_still_parses() {
        // `ConvertTo-Json -InputObject @(x)` keeps the array; make sure an empty one is fine too.
        assert!(parse("[]").is_empty());
        assert_eq!(parse(r#"[{"id":"a","name":"A","connected":true,"paired":false}]"#).len(), 1);
    }
}

#[cfg(all(test, windows))]
mod live {
    /// Manual check on a real PC: `cargo test live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn paired_devices() {
        println!("{:#?}", super::paired());
    }

    /// Pairs the device whose id is in `LUMEN_BT_ID` (put it in pairing mode first).
    #[test]
    #[ignore]
    fn pair_device() {
        let id = std::env::var("LUMEN_BT_ID").expect("set LUMEN_BT_ID");
        println!("{:?}", super::pair(&id));
    }

    #[test]
    #[ignore]
    fn nearby_devices() {
        println!("{:#?}", super::scan());
    }
}
