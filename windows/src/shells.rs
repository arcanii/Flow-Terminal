//! The Windows shells: cmd, PowerShell, and a bash from Git Bash, MSYS2 or
//! WSL, or else the BusyBox that ships with Flow.

use std::path::{Path, PathBuf};

use flow_core::shell::{first_existing, which, BashFlavor, ShellKind, ShellProfile};

/// All shells this machine has, in title-bar order.
pub fn shells() -> Vec<ShellProfile> {
    let mut out = Vec::new();
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let cmd = std::env::var("ComSpec")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&system_root).join(r"System32\cmd.exe"));
    out.push(ShellProfile {
        id: "cmd",
        label: "cmd".into(),
        kind: ShellKind::Cmd,
        program: cmd,
        flavor: BashFlavor::Native,
    });

    let pwsh = which("pwsh").or_else(|| {
        first_existing(&[PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe")])
    });
    let (ps, label) = match pwsh {
        Some(p) => (p, "pwsh"),
        None => (
            which("powershell").unwrap_or_else(|| {
                PathBuf::from(&system_root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
            }),
            "PowerShell",
        ),
    };
    out.push(ShellProfile {
        id: "powershell",
        label: label.into(),
        kind: ShellKind::PowerShell,
        program: ps,
        flavor: BashFlavor::Native,
    });

    // FLOW_SHELL=busybox tries the built-in bash even where a real one exists.
    let bash = if std::env::var("FLOW_SHELL").as_deref() == Ok("busybox") {
        bundled_busybox()
    } else {
        find_windows_bash()
    };
    if let Some(bash) = bash {
        out.push(bash);
    }
    out
}

fn find_windows_bash() -> Option<ShellProfile> {
    let mut git_candidates = vec![
        PathBuf::from(r"C:\Program Files\Git\bin\bash.exe"),
        PathBuf::from(r"C:\Program Files (x86)\Git\bin\bash.exe"),
    ];
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        git_candidates.push(PathBuf::from(local).join(r"Programs\Git\bin\bash.exe"));
    }
    if let Some(git) = which("git") {
        // ...\Git\cmd\git.exe -> ...\Git\bin\bash.exe
        if let Some(root) = git.parent().and_then(Path::parent) {
            git_candidates.push(root.join(r"bin\bash.exe"));
        }
    }
    if let Some(p) = first_existing(&git_candidates) {
        return Some(ShellProfile {
            id: "bash",
            label: "bash".into(),
            kind: ShellKind::Bash,
            program: p,
            flavor: BashFlavor::GitBash,
        });
    }
    if let Some(p) = first_existing(&[PathBuf::from(r"C:\msys64\usr\bin\bash.exe")]) {
        return Some(ShellProfile {
            id: "bash",
            label: "bash".into(),
            kind: ShellKind::Bash,
            program: p,
            flavor: BashFlavor::Msys2,
        });
    }
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let wsl = PathBuf::from(system_root).join(r"System32\wsl.exe");
    if wsl.is_file() && wsl_has_distro(&wsl) {
        return Some(ShellProfile {
            id: "bash",
            label: "bash (WSL)".into(),
            kind: ShellKind::Bash,
            program: wsl,
            flavor: BashFlavor::Wsl,
        });
    }
    bundled_busybox()
}

/// The BusyBox that ships next to flow.exe, or in dev builds the vendored copy.
pub fn bundled_busybox() -> Option<ShellProfile> {
    let installed = std::env::current_exe().ok().map(|exe| exe.with_file_name("busybox.exe"));
    let vendored = cfg!(debug_assertions)
        .then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(r"third_party\busybox\busybox64u.exe"));
    let program = [installed, vendored].into_iter().flatten().find(|p| p.is_file())?;
    Some(ShellProfile {
        id: "bash",
        label: "bash".into(),
        kind: ShellKind::Bash,
        program,
        flavor: BashFlavor::BusyBox,
    })
}

fn wsl_has_distro(wsl: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new(wsl)
        .args(["--list", "--quiet"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success() && o.stdout.iter().any(|b| b.is_ascii_alphanumeric()))
        .unwrap_or(false)
}
