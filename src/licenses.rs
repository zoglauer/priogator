//! The licenses of the libraries built into Priogator, collected by
//! `scripts/licenses.sh` into `assets/third-party-licenses.txt`.

use std::sync::OnceLock;

const THIRD_PARTY: &str = include_str!("../assets/third-party-licenses.txt");

/// Priogator's own license.
pub const OWN: &str = include_str!("../LICENSE");

pub struct Library {
    pub name: String,
    pub version: String,
    pub repository: String,
}

/// One license text and the libraries that use it.
pub struct License {
    pub name: String,
    pub libraries: Vec<Library>,
    pub text: String,
}

/// All third-party licenses, Priogator itself left out.
pub fn third_party() -> &'static [License] {
    static LICENSES: OnceLock<Vec<License>> = OnceLock::new();
    LICENSES.get_or_init(parse)
}

fn parse() -> Vec<License> {
    let mut licenses: Vec<License> = Vec::new();
    for line in THIRD_PARTY.lines() {
        if let Some(name) = line.strip_prefix("@@license ") {
            licenses.push(License {
                name: name.to_owned(),
                libraries: Vec::new(),
                text: String::new(),
            });
        } else if let Some(license) = licenses.last_mut() {
            if let Some(library) = line.strip_prefix("@@crate ") {
                let mut parts = library.split(' ');
                let name = parts.next().unwrap_or_default();
                if name != env!("CARGO_PKG_NAME") {
                    license.libraries.push(Library {
                        name: name.to_owned(),
                        version: parts.next().unwrap_or_default().to_owned(),
                        repository: parts.next().unwrap_or_default().to_owned(),
                    });
                }
            } else {
                license.text.push_str(line);
                license.text.push('\n');
            }
        }
    }
    for license in &mut licenses {
        license.text = license.text.trim_matches('\n').to_owned();
    }
    licenses.retain(|l| !l.libraries.is_empty());
    licenses
}

/// The licenses grouped by name, most used first. Libraries often ship their
/// own copy of a license (with their own copyright line), so one name can
/// have several texts.
pub fn by_name(licenses: &[License]) -> Vec<(&str, Vec<&License>)> {
    let mut groups: Vec<(&str, Vec<&License>)> = Vec::new();
    for license in licenses {
        match groups.iter_mut().find(|(name, _)| *name == license.name) {
            Some((_, texts)) => texts.push(license),
            None => groups.push((&license.name, vec![license])),
        }
    }
    let count = |texts: &[&License]| texts.iter().map(|l| l.libraries.len()).sum::<usize>();
    groups.sort_by_key(|(_, texts)| std::cmp::Reverse(count(texts)));
    groups
}

/// How many different libraries (by name) are built in.
pub fn library_count(licenses: &[License]) -> usize {
    let mut names: Vec<&str> = licenses
        .iter()
        .flat_map(|l| l.libraries.iter().map(|lib| lib.name.as_str()))
        .collect();
    names.sort_unstable();
    names.dedup();
    names.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_license_has_its_libraries_and_text() {
        let licenses = third_party();
        assert!(licenses.len() > 10);
        for l in licenses {
            assert!(!l.libraries.is_empty(), "{}", l.name);
            assert!(l.text.len() > 100, "{}", l.name);
        }
        let names: Vec<&str> = licenses
            .iter()
            .flat_map(|l| l.libraries.iter().map(|lib| lib.name.as_str()))
            .collect();
        assert!(names.contains(&"egui"));
        assert!(names.contains(&"jiff"));
        assert!(!names.contains(&"priogator"));
        assert!(library_count(licenses) > 100);
        let groups = by_name(licenses);
        assert_eq!(groups[0].0, "Apache License 2.0");
        let mut names: Vec<&str> = groups.iter().map(|(n, _)| *n).collect();
        names.dedup();
        assert_eq!(names.len(), groups.len(), "each license once");
    }

    /// Every library in Cargo.lock that ships with the app must be listed;
    /// run scripts/licenses.sh after changing dependencies.
    #[test]
    fn list_is_up_to_date() {
        let listed: Vec<String> = third_party()
            .iter()
            .flat_map(|l| {
                l.libraries
                    .iter()
                    .map(|lib| format!("{} {}", lib.name, lib.version))
            })
            .collect();
        for direct in [
            "eframe",
            "jiff",
            "serde",
            "uuid",
            "rfd",
            "fontique",
            "directories",
        ] {
            let lock = include_str!("../Cargo.lock");
            let version = lock
                .split("[[package]]")
                .find_map(|p| {
                    p.contains(&format!("name = \"{direct}\"\n"))
                        .then(|| p.split("version = \"").nth(1)?.split('"').next())
                        .flatten()
                })
                .unwrap();
            assert!(
                listed.contains(&format!("{direct} {version}")),
                "{direct} {version} missing: run scripts/licenses.sh"
            );
        }
    }
}
