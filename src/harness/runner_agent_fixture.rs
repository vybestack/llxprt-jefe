use std::path::PathBuf;

use crate::harness::TmuxStartRequest;

pub(super) fn install_llxprt_probe_fixture(config_dir: &std::path::Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let bin_dir = config_dir.join("fixture-bin");
    std::fs::create_dir_all(&bin_dir)
        .unwrap_or_else(|error| panic!("create fixture bin directory: {error}"));
    let executable = bin_dir.join("llxprt");
    std::fs::write(
        &executable,
        "#!/bin/sh\ncase \"$1\" in\n  --version) printf 'llxprt 1.0.0\\n' ;;\n  --help) printf '%s\\n' '--prompt-interactive --profile-load --sandbox --sandbox-engine --yolo --approval-mode --continue' ;;\n  *) sleep 300 ;;\nesac\n",
    )
    .unwrap_or_else(|error| panic!("write llxprt fixture: {error}"));
    let mut permissions = std::fs::metadata(&executable)
        .unwrap_or_else(|error| panic!("read llxprt fixture metadata: {error}"))
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&executable, permissions)
        .unwrap_or_else(|error| panic!("make llxprt fixture executable: {error}"));
    bin_dir
}

pub(super) fn prepend_fixture_path(request: &mut TmuxStartRequest, bin_dir: &std::path::Path) {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![bin_dir.to_path_buf()];
    paths.extend(std::env::split_paths(&existing));
    let path =
        std::env::join_paths(paths).unwrap_or_else(|error| panic!("compose fixture PATH: {error}"));
    request
        .command
        .insert(0, format!("PATH={}", path.to_string_lossy()));
    request.command.insert(0, "env".to_owned());
}
