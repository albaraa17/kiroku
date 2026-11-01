use anyhow::{Context, Result, bail};
use log::info;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

fn clean_windows_path(p: &Path) -> String {
    let raw = p.to_string_lossy();
    if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{}", unc)
    } else if let Some(stripped) = raw.strip_prefix(r"\\?\") {
        stripped.to_string()
    } else {
        raw.into_owned()
    }
}

fn escape_ps_single_quoted(s: &str) -> String {
    s.replace('\'', "''")
}

/// Injects `LunaHook32.dll` or `LunaHook64.dll` into the target process using
/// `LunaSubProcess32.exe` / `LunaSubProcess64.exe` (which also handles WolfRPG WinVerifyTrust bypass).
pub fn inject_lunahook_dll(lunahook_dir: &Path, pid: u32, is_64bit: bool) -> Result<()> {
    let helper_name = if is_64bit {
        "LunaSubProcess64.exe"
    } else {
        "LunaSubProcess32.exe"
    };
    let dll_name = if is_64bit {
        "LunaHook64.dll"
    } else {
        "LunaHook32.dll"
    };

    let helper_path = lunahook_dir.join(helper_name);
    let dll_path = lunahook_dir.join(dll_name);

    if !helper_path.exists() {
        bail!("Injector helper not found: {}", helper_path.display());
    }
    if !dll_path.exists() {
        bail!("Hook DLL not found: {}", dll_path.display());
    }

    let helper_abs = std::fs::canonicalize(&helper_path).unwrap_or(helper_path);
    let dll_abs = std::fs::canonicalize(&dll_path).unwrap_or(dll_path);

    let helper_str = clean_windows_path(&helper_abs);
    let dll_str = clean_windows_path(&dll_abs);

    info!(
        "Injecting {} into PID {} using {}",
        dll_str, pid, helper_str
    );

    let status = Command::new(&helper_str)
        .arg("dllinject")
        .arg(pid.to_string())
        .arg(&dll_str)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .with_context(|| format!("Failed to execute {}", helper_str))?;

    // dllinjectwmain returns 1 on success
    if status.code() == Some(1) {
        return Ok(());
    }

    info!(
        "Standard injection returned {:?} for PID {}; attempting elevated UAC injection...",
        status.code(),
        pid
    );

    let ps_helper = escape_ps_single_quoted(&helper_str);
    let ps_dll = escape_ps_single_quoted(&dll_str.replace('"', ""));
    let ps_cmd = format!(
        "$p = Start-Process -FilePath '{}' -ArgumentList 'dllinject {} \"{}\"' -Verb RunAs -WindowStyle Hidden -PassThru -Wait; if ($p.ExitCode -eq 1) {{ exit 0 }} else {{ exit 1 }}",
        ps_helper, pid, ps_dll
    );

    let elev_status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &ps_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .context("Failed to launch elevated injector via PowerShell")?;

    if !elev_status.success() {
        bail!(
            "Failed to inject {} into PID {} (target may be protected or UAC was declined)",
            dll_name,
            pid
        );
    }

    Ok(())
}
