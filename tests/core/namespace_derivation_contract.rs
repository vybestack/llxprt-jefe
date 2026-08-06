//! The multiplexer namespace must never be keyed on machine identity (#547, V1).
//!
//! The namespace that isolates jefe's psmux server was once derived from the
//! hostname joined to the account name. Both are properties of the machine
//! rather than of the installation, and that produced two failures at once.
//!
//! Because the material is global to the box, every jefe running on it
//! collapsed into a single namespace: six working trees spanning two unrelated
//! projects were observed sharing one psmux server, so unrelated agents
//! appeared in each other's session lists. And because the material can change
//! while the installation does not, renaming the machine, renaming the account,
//! or merely receiving a different casing for it moved the namespace and
//! silently orphaned every running session. The agents kept running; jefe lost
//! the address.
//!
//! The derivation is now a pure function of the resolved state path. That is a
//! property jefe controls and persists, so it survives machine renames while
//! still separating genuinely distinct installations.
//!
//! This contract keeps machine identity from creeping back in. Deleting the old
//! helpers is not enough on its own: the failure mode is quiet and only shows up
//! as vanished agents on someone else's machine, long after the change that
//! caused it. So the ban is enforced on the source rather than trusted to
//! review.

use std::path::{Path, PathBuf};

/// Where the namespace is derived. Scoped deliberately: `whoami` is legitimate
/// elsewhere in the tree (`src/jsp_host/launch.rs` needs the real account), so a
/// repo-wide ban would be wrong. What matters is that *namespace derivation*
/// cannot see it.
const DERIVATION_MODULE: &str = "src/runtime/namespace.rs";

/// Sources of machine identity, and why each one must not reach the namespace.
const MACHINE_IDENTITY_SOURCES: &[(&str, &str)] = &[
    (
        "whoami",
        "the crate that supplied the original hostname and account material",
    ),
    (
        "COMPUTERNAME",
        "the Windows hostname variable; moves on machine rename",
    ),
    (
        "USERNAME",
        "the Windows account variable; casing alone has been observed to differ",
    ),
    ("LOGNAME", "the Unix login-name variable"),
    ("gethostname", "a direct hostname syscall wrapper"),
    ("hostname", "any hostname lookup"),
];

#[test]
fn the_namespace_derivation_reads_no_machine_identity() {
    let source = read_source(DERIVATION_MODULE);
    let mut offenders = Vec::new();

    for (token, why) in MACHINE_IDENTITY_SOURCES {
        for (index, line) in source.iter().enumerate() {
            if mentions_in_code(line, token) {
                offenders.push(format!(
                    "{DERIVATION_MODULE} line {} names `{token}` ({why})",
                    index + 1
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "machine identity has reappeared in the namespace derivation:\n  {}\n\nThe namespace \
         must depend only on the resolved state path. Hostname and account material is global \
         to the machine, so it collapses unrelated jefe installations into one namespace, and \
         it changes without the installation changing, which silently orphans every running \
         session. Derive from the state path instead.",
        offenders.join("\n  ")
    );
}

#[test]
fn the_namespace_derivation_reads_no_environment() {
    let source = read_source(DERIVATION_MODULE);
    let mut offenders = Vec::new();

    for (index, line) in source.iter().enumerate() {
        if mentions_in_code(line, "env::var") || mentions_in_code(line, "env::var_os") {
            offenders.push(format!("{DERIVATION_MODULE} line {}", index + 1));
        }
    }

    assert!(
        offenders.is_empty(),
        "the namespace derivation reads the environment directly:\n  {}\n\nIt must be a pure \
         function of the state path it is handed. Reading the environment here would reintroduce \
         ambient inputs that callers cannot see or test, which is how the derivation drifted \
         away from the installation in the first place. Resolve paths at the boundary and pass \
         the result in.",
        offenders.join("\n  ")
    );
}

#[test]
fn the_stable_namespace_is_a_function_of_a_path_alone() {
    let source = read_source(DERIVATION_MODULE);

    for entry_point in ["for_state_path", "unique_for_state_path", "isolated_run"] {
        let signature = format!("pub fn {entry_point}(");
        let index = locate(&source, &signature).unwrap_or_else(|| {
            panic!(
                "{DERIVATION_MODULE} no longer declares `{signature}`, so this contract is \
                 guarding a function that does not exist and would pass vacuously. If the entry \
                 point was renamed, update this contract deliberately."
            )
        });

        let declaration = &source[index];
        let Some(parameters) = declaration
            .split_once('(')
            .and_then(|(_, rest)| rest.rsplit_once(')'))
            .map(|(params, _)| params.trim().to_owned())
        else {
            panic!("could not read the parameter list of `{entry_point}`: {declaration}")
        };

        assert_eq!(
            parameters, "state_path: &Path",
            "`{entry_point}` takes parameters other than the state path. Every additional input \
             is another way for the namespace to move without the installation moving, which is \
             the defect this issue exists to fix. Keep the derivation keyed on the path alone."
        );
    }
}

#[test]
fn the_raw_hash_primitive_is_not_reachable_outside_the_derivation() {
    const PRIMITIVE: &str = "hash_identity_material";

    let derivation = read_source(DERIVATION_MODULE);
    let definition = locate(&derivation, &format!("fn {PRIMITIVE}(")).unwrap_or_else(|| {
        panic!(
            "{DERIVATION_MODULE} no longer defines `{PRIMITIVE}`, so this contract is guarding a \
             function that does not exist and would pass vacuously. If the hash primitive was \
             renamed, update this contract deliberately."
        )
    });
    assert!(
        !derivation[definition].trim_start().starts_with("pub "),
        "`{PRIMITIVE}` is public. It accepts arbitrary material, so exposing it lets a caller key \
         the namespace on anything at all -- which is exactly how hostname and account material \
         got in. Keep it private and route callers through the path-based constructors."
    );

    let mut offenders = Vec::new();

    for file in rust_files(&repo_root().join("src")) {
        let shown = display_path(&file);
        if shown.ends_with(DERIVATION_MODULE) {
            continue;
        }
        for (index, line) in read_source_at(&file).iter().enumerate() {
            if mentions_in_code(line, PRIMITIVE) {
                offenders.push(format!("{shown} line {}", index + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the raw namespace hash is named outside the derivation module:\n  {}\n\nIt accepts \
         arbitrary material, so a caller elsewhere can key the namespace on anything at all -- \
         which is exactly how hostname and account material got in. Route new callers through \
         `InstallationId::for_state_path` so the state path stays the only input.",
        offenders.join("\n  ")
    );
}

#[test]
fn the_retired_machine_identity_helpers_stay_retired() {
    let retired = [
        "current_identity_material",
        "stable_current_user_namespace",
        "unique_current_user_namespace",
    ];
    let mut offenders = Vec::new();

    for file in rust_files(&repo_root().join("src")) {
        let shown = display_path(&file);
        for (index, line) in read_source_at(&file).iter().enumerate() {
            for name in retired {
                if mentions_in_code(line, name) {
                    offenders.push(format!("{shown} line {} names `{name}`", index + 1));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a retired machine-identity helper is back:\n  {}\n\nThese derived the namespace from \
         hostname plus account. They were removed because that made running agents disappear on \
         machine rename and merged unrelated installations into one psmux server.",
        offenders.join("\n  ")
    );
}

/// Index of the first line whose code (not comment) contains `needle`.
fn locate(source: &[String], needle: &str) -> Option<usize> {
    source
        .iter()
        .position(|line| mentions_in_code(line, needle))
}

/// Whether `line` contains `needle` as code rather than prose.
///
/// The doc comments in this area necessarily narrate the hostname history they
/// exist to warn about, and flagging those would make the contract impossible
/// to document.
fn mentions_in_code(line: &str, needle: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return false;
    }
    line.contains(needle)
}

fn read_source(relative: &str) -> Vec<String> {
    read_source_at(&repo_root().join(relative))
}

fn read_source_at(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()))
        .lines()
        .map(str::to_owned)
        .collect()
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(dir, &mut found);
    found
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn display_path(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}
