#![warn(clippy::pedantic)]

//! The third-party notices every release archive carries, held to what
//! `licenses/README.md` records of them.
//!
//! The release workflow copies `licenses/` into each package straight from the
//! checkout, so these bytes are the bytes a user receives. The README records
//! the SHA-256 of each file, the crate version and package it was copied from,
//! and that package's checksum. A notice edited, truncated or rewritten by a
//! checkout — a Windows checkout converting its line endings, which
//! `.gitattributes` exempts the directory from — fails its checksum here, and a
//! `spacewasm` pin that moves without the notices being copied again fails the
//! version rows. Only tracked files are read: nothing comes from the cargo
//! registry, so the checks run offline and on every platform.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The repository checkout this crate sits in.
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The directory every release archive carries as `licenses/`.
fn licenses() -> PathBuf {
    repository().join("licenses")
}

/// `licenses/README.md`, as text.
fn readme() -> String {
    std::fs::read_to_string(licenses().join("README.md"))
        .expect("licenses/README.md must be readable")
}

/// The body rows of the Markdown table in `readme` whose header row is
/// `header`, each cell trimmed of spaces and of the backticks around it.
fn table(readme: &str, header: &str) -> Vec<Vec<String>> {
    let mut lines = readme.lines().skip_while(|line| *line != header);
    assert!(
        lines.next().is_some(),
        "licenses/README.md must carry the table headed `{header}`"
    );
    let separator = lines.next().unwrap_or_default();
    assert!(
        separator.starts_with("|---"),
        "the table headed `{header}` must have its separator row, got `{separator}`"
    );
    lines
        .take_while(|line| line.starts_with('|'))
        .map(|line| {
            line.trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().trim_matches('`').to_string())
                .collect()
        })
        .collect()
}

/// The README's crate table: each crate's name, its version, and the files it
/// lists for that crate.
fn crates(readme: &str) -> Vec<(String, String, Vec<String>)> {
    table(readme, "| Crate | Version | License | Files |")
        .into_iter()
        .map(|row| {
            let [name, version, _, files] = row.as_slice() else {
                panic!("a crate row has four cells, got {row:?}");
            };
            let files = files
                .split(',')
                .map(|file| file.trim().trim_matches('`').to_string())
                .collect();
            (name.clone(), version.clone(), files)
        })
        .collect()
}

/// The README's provenance table: each file, its SHA-256, the package it was
/// copied from, and that package's SHA-256.
fn provenance(readme: &str) -> Vec<[String; 4]> {
    table(readme, "| File | SHA-256 | From the package | Package SHA-256 |")
        .into_iter()
        .map(|row| {
            <[String; 4]>::try_from(row.clone())
                .unwrap_or_else(|_| panic!("a provenance row has four cells, got {row:?}"))
        })
        .collect()
}

/// The version the README records for `name`.
fn version_of(readme: &str, name: &str) -> String {
    let Some((_, version, _)) = crates(readme)
        .into_iter()
        .find(|(crate_name, _, _)| crate_name == name)
    else {
        panic!("licenses/README.md must record a version for `{name}`");
    };
    version
}

/// Every file under `dir` but `README.md` at its top, as a `/`-separated name
/// relative to `dir` — the spelling the README's tables use — sorted.
fn notice_files(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", dir.display()))
        {
            let path = entry.expect("a directory entry").path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a notice's name is UTF-8");
            let member = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            if path.is_dir() {
                walk(&path, &member, found);
            } else if member != "README.md" {
                found.push(member);
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, "", &mut found);
    found.sort();
    found
}

/// The path of the `/`-separated name `member` under `dir`.
fn member_path(dir: &Path, member: &str) -> PathBuf {
    member
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// The lowercase-hex SHA-256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The `[[package]]` tables of the tracked lockfile the Rocq discharge lane
/// builds with, which is the one lockfile the repository tracks.
fn locked_packages() -> Vec<toml::Table> {
    let text = std::fs::read_to_string(repository().join("ci").join("rocq-discharge.cargo-lock"))
        .expect("ci/rocq-discharge.cargo-lock must be readable");
    let lock: toml::Table = toml::from_str(&text).expect("the lockfile is TOML");
    lock.get("package")
        .and_then(toml::Value::as_array)
        .expect("the lockfile lists its packages")
        .iter()
        .map(|package| package.as_table().expect("a package is a table").clone())
        .collect()
}

/// The string field `key` of a lockfile package.
fn field<'p>(package: &'p toml::Table, key: &str) -> &'p str {
    package
        .get(key)
        .and_then(toml::Value::as_str)
        .unwrap_or_else(|| panic!("a locked package has a string `{key}`: {package:?}"))
}

/// The locked package `name` at `version`.
fn locked<'p>(packages: &'p [toml::Table], name: &str, version: &str) -> &'p toml::Table {
    packages
        .iter()
        .find(|package| field(package, "name") == name && field(package, "version") == version)
        .unwrap_or_else(|| {
            panic!("ci/rocq-discharge.cargo-lock must lock `{name}` {version}")
        })
}

/// Each notice is the file whose SHA-256 the README records, and the README
/// records every notice: a file in the directory with no row, or a row with no
/// file, is as much a failure as bytes that moved.
///
/// Fails if a notice is edited, truncated or appended to, if a checkout
/// converts its line endings (`.gitattributes` exempts `licenses/` from that),
/// or if a file is added to or removed from the directory without its row.
#[test]
fn every_notice_is_the_file_the_readme_records() {
    let readme = readme();
    let rows = provenance(&readme);
    let mut recorded: Vec<String> = rows.iter().map(|[file, ..]| file.clone()).collect();
    recorded.sort();
    assert_eq!(
        notice_files(&licenses()),
        recorded,
        "licenses/ must hold exactly the files licenses/README.md records"
    );

    for [file, sha256, ..] in &rows {
        let bytes = std::fs::read(member_path(&licenses(), file))
            .unwrap_or_else(|e| panic!("licenses/{file} must be readable: {e}"));
        assert_eq!(
            sha256_hex(&bytes),
            *sha256,
            "licenses/{file} ({} bytes, {} CRLF line endings) is not the byte-for-byte copy \
             licenses/README.md records: an edit changes it, and so does a checkout that \
             converts line endings",
            bytes.len(),
            bytes.windows(2).filter(|pair| pair == b"\r\n").count()
        );
    }

    for (name, _, mut listed) in crates(&readme) {
        listed.sort();
        let under_crate: Vec<String> = recorded
            .iter()
            .filter(|file| file.starts_with(&format!("{name}/")))
            .cloned()
            .collect();
        assert_eq!(
            listed, under_crate,
            "the crate table must list every notice of `{name}` and no other"
        );
    }
}

/// The notices are the ones of the crates the workspace links: the README's
/// `spacewasm` version is the workspace's `=`-pin, its `libm` version is the
/// one that `spacewasm` resolves to in the tracked lockfile, and each package
/// the provenance table names is that crate at that version, with the checksum
/// the lockfile records for it.
///
/// Fails when the `spacewasm` pin moves, or the lockfile resolves another
/// `libm`, without the notices being copied again from the new packages and
/// both tables updated.
#[test]
fn the_notices_come_from_the_packages_the_workspace_pins() {
    let readme = readme();
    let spacewasm = version_of(&readme, "spacewasm");
    let libm = version_of(&readme, "libm");

    let manifest: toml::Table = toml::from_str(
        &std::fs::read_to_string(repository().join("Cargo.toml"))
            .expect("the workspace Cargo.toml must be readable"),
    )
    .expect("the workspace Cargo.toml is TOML");
    let pin = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(|dependencies| dependencies.get("spacewasm"))
        .and_then(|spacewasm| spacewasm.get("version"))
        .and_then(toml::Value::as_str)
        .expect("the workspace pins `spacewasm`");
    assert_eq!(
        pin,
        format!("={spacewasm}"),
        "licenses/README.md records `spacewasm` {spacewasm}; copy the notices again from the \
         package the workspace pins"
    );

    let packages = locked_packages();
    let locked_spacewasm = locked(&packages, "spacewasm", &spacewasm);
    let libm_dependency = locked_spacewasm
        .get("dependencies")
        .and_then(toml::Value::as_array)
        .and_then(|dependencies| {
            dependencies
                .iter()
                .filter_map(toml::Value::as_str)
                .find(|dependency| dependency.split(' ').next() == Some("libm"))
        })
        .expect("the locked `spacewasm` depends on `libm`");
    let resolved_libm = if let Some(version) = libm_dependency.split(' ').nth(1) {
        version.to_string()
    } else {
        let versions: Vec<&str> = packages
            .iter()
            .filter(|package| field(package, "name") == "libm")
            .map(|package| field(package, "version"))
            .collect();
        let [version] = versions.as_slice() else {
            panic!("an unversioned dependency names the one locked `libm`, got {versions:?}");
        };
        (*version).to_string()
    };
    assert_eq!(
        libm, resolved_libm,
        "licenses/README.md records `libm` {libm}; copy its notice again from the package the \
         locked `spacewasm` {spacewasm} resolves to"
    );

    for [file, _, package, package_sha256] in provenance(&readme) {
        let name = file
            .split('/')
            .next()
            .expect("a notice sits under its crate's directory");
        let version = version_of(&readme, name);
        assert_eq!(
            package,
            format!("{name}-{version}.crate"),
            "licenses/{file} must come from the package of the version the crate table records"
        );
        assert_eq!(
            field(locked(&packages, name, &version), "checksum"),
            package_sha256,
            "the checksum licenses/README.md records for {package} must be the lockfile's"
        );
    }
}
