//! Header dependencies reported by the compiler.
//!
//! Every compile asks the compiler to write the files it read: a Make-style
//! depfile from Clang and GCC (`-MD -MF`), a JSON file from MSVC
//! (`/sourceDependencies`). The headers in it feed incremental rebuild
//! detection and cache keys, so a header edit rebuilds every translation
//! unit that includes it.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

/// BMI extensions a depfile can mention. Imported BMIs are tracked as build
/// graph edges, not as headers, so they are dropped here. `c++m` is the
/// phony module target GCC writes.
const BMI_EXTENSIONS: &[&str] = &["pcm", "gcm", "ifc", "c++m"];

/// Path of the dependency file written alongside `obj_output`.
///
/// `ext` is `"d"` for Make-style depfiles and `"json"` for MSVC.
pub fn depfile_path(obj_output: &Path, ext: &str) -> PathBuf {
    let mut path = obj_output.as_os_str().to_owned();
    path.push(".");
    path.push(ext);
    PathBuf::from(path)
}

/// Parse a Make-style depfile and return every prerequisite that is not a
/// BMI, in order of first appearance.
///
/// Handles what Clang and GCC emit: `\`-newline continuations, `\ ` and
/// `\#` escapes, `$$`, several rules per file, and the extra rules GCC
/// writes in module mode (`.PHONY`, order-only `:|` rules,
/// `CXX_IMPORTS +=` assignments).
pub fn parse_make_depfile(content: &str) -> Vec<PathBuf> {
    let joined = content.replace("\\\r\n", " ").replace("\\\n", " ");

    let mut seen = HashSet::new();
    let mut prereqs = Vec::new();
    for line in joined.lines() {
        let tokens = tokenize(line);
        let Some(sep) = tokens.iter().position(|t| t.ends_with(':') || t == ":|") else {
            // Variable assignments (`CXX_IMPORTS += …`) and blank lines.
            continue;
        };
        if tokens[sep].ends_with(":|") || tokens[sep] == ".PHONY:" {
            continue;
        }
        for token in &tokens[sep + 1..] {
            if token == "|" {
                // Order-only prerequisites follow; they are not inputs.
                break;
            }
            if is_bmi(token) {
                continue;
            }
            if seen.insert(token.clone()) {
                prereqs.push(PathBuf::from(token));
            }
        }
    }
    prereqs
}

/// Split one logical depfile line on unescaped whitespace, undoing Make
/// escapes.
fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if matches!(chars.peek(), Some(' ') | Some('#')) => {
                current.push(chars.next().unwrap_or(' '));
            }
            '$' if chars.peek() == Some(&'$') => {
                chars.next();
                current.push('$');
            }
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn is_bmi(token: &str) -> bool {
    Path::new(token)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| BMI_EXTENSIONS.contains(&e))
}

/// Parse the JSON MSVC writes for `/sourceDependencies` and return its
/// `Data.Includes` list. `None` when the content is not that format.
pub fn parse_msvc_source_dependencies(content: &str) -> Option<Vec<PathBuf>> {
    let value: serde_json::Value = serde_json::from_str(content).ok()?;
    let includes = value.get("Data")?.get("Includes")?.as_array()?;
    Some(
        includes
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|s| !is_bmi(s))
            .map(PathBuf::from)
            .collect(),
    )
}

/// Turn raw depfile entries into the header list for `source`: relative
/// entries resolved against `cwd` (the directory the compiler ran in),
/// the source itself removed, duplicates dropped.
pub fn header_list(source: &Path, entries: Vec<PathBuf>, cwd: &Path) -> Vec<PathBuf> {
    let source = normalize(&absolute(source, cwd));
    let mut seen = HashSet::new();
    entries
        .into_iter()
        .map(|p| normalize(&absolute(&p, cwd)))
        .filter(|p| *p != source)
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

fn absolute(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

/// Drop `.` components. `..` stays: folding it lexically is wrong when the
/// directory before it is a symlink, and the OS resolves it on open anyway.
fn normalize(path: &Path) -> PathBuf {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depfile_path() {
        assert_eq!(
            depfile_path(Path::new("build/obj/foo.o"), "d"),
            PathBuf::from("build/obj/foo.o.d")
        );
        assert_eq!(
            depfile_path(Path::new("build/obj/foo.obj"), "json"),
            PathBuf::from("build/obj/foo.obj.json")
        );
    }

    #[test]
    fn test_parse_clang_depfile() {
        let content = "t/main.o: src/main.cpp \\\n  /usr/include/stdio.h \\\n  include/value.h\n";
        assert_eq!(
            parse_make_depfile(content),
            vec![
                PathBuf::from("src/main.cpp"),
                PathBuf::from("/usr/include/stdio.h"),
                PathBuf::from("include/value.h"),
            ]
        );
    }

    #[test]
    fn test_parse_clang_precompile_depfile() {
        let content = "t/lib.pcm: src/lib.cppm include/value.h\n";
        assert_eq!(
            parse_make_depfile(content),
            vec![
                PathBuf::from("src/lib.cppm"),
                PathBuf::from("include/value.h")
            ]
        );
    }

    /// Verbatim shape of `g++ -fmodules-ts -MD` for a module interface.
    #[test]
    fn test_parse_gcc_module_interface_depfile() {
        let content = "lib.o \\\n /b/lib.gcm: \\\n ../src/lib.cppm /usr/include/stdc-predef.h ../include/value.h\n\
local.hdr.c++m: \\\n /b/lib.gcm\n\
.PHONY: local.hdr.c++m\n\
/b/lib.gcm:| \\\n lib.o\n";
        assert_eq!(
            parse_make_depfile(content),
            vec![
                PathBuf::from("../src/lib.cppm"),
                PathBuf::from("/usr/include/stdc-predef.h"),
                PathBuf::from("../include/value.h"),
            ]
        );
    }

    /// Verbatim shape of `g++ -fmodules-ts -MD` for a module importer.
    #[test]
    fn test_parse_gcc_module_importer_depfile() {
        let content =
            "main.o: ../src/main.cpp /usr/include/stdc-predef.h \\\n /usr/include/c++/13/cstdio\n\
main.o: local.hdr.c++m\n\
CXX_IMPORTS += local.hdr.c++m\n";
        assert_eq!(
            parse_make_depfile(content),
            vec![
                PathBuf::from("../src/main.cpp"),
                PathBuf::from("/usr/include/stdc-predef.h"),
                PathBuf::from("/usr/include/c++/13/cstdio"),
            ]
        );
    }

    #[test]
    fn test_parse_depfile_escapes() {
        let content = "s.o: s.cpp sp\\ ace/x.h hash\\#dir/y.h cost$$/z.h\n";
        assert_eq!(
            parse_make_depfile(content),
            vec![
                PathBuf::from("s.cpp"),
                PathBuf::from("sp ace/x.h"),
                PathBuf::from("hash#dir/y.h"),
                PathBuf::from("cost$/z.h"),
            ]
        );
    }

    #[test]
    fn test_parse_depfile_crlf_and_windows_paths() {
        let content = "C:\\b\\main.o: C:\\src\\main.cpp \\\r\n  C:\\inc\\value.h\r\n";
        assert_eq!(
            parse_make_depfile(content),
            vec![
                PathBuf::from("C:\\src\\main.cpp"),
                PathBuf::from("C:\\inc\\value.h"),
            ]
        );
    }

    #[test]
    fn test_parse_depfile_skips_order_only_and_dedupes() {
        let content = "a.o: a.cpp x.h | order.h\nb.o: x.h y.h\n";
        assert_eq!(
            parse_make_depfile(content),
            vec![
                PathBuf::from("a.cpp"),
                PathBuf::from("x.h"),
                PathBuf::from("y.h")
            ]
        );
    }

    #[test]
    fn test_parse_depfile_drops_bmis() {
        let content = "m.o: m.cpp dep.pcm other.gcm win.ifc local.hdr.c++m h.h\n";
        assert_eq!(
            parse_make_depfile(content),
            vec![PathBuf::from("m.cpp"), PathBuf::from("h.h")]
        );
    }

    #[test]
    fn test_parse_empty_depfile() {
        assert!(parse_make_depfile("").is_empty());
        assert!(parse_make_depfile("\n\n").is_empty());
    }

    #[test]
    fn test_parse_msvc_source_dependencies() {
        let content = r#"{
            "Version": "1.2",
            "Data": {
                "Source": "c:\\p\\src\\lib.cppm",
                "ProvidedModule": "local.hdr",
                "Includes": ["c:\\p\\include\\value.h", "c:\\vc\\include\\cstdio"],
                "ImportedModules": [],
                "ImportedHeaderUnits": []
            }
        }"#;
        assert_eq!(
            parse_msvc_source_dependencies(content),
            Some(vec![
                PathBuf::from("c:\\p\\include\\value.h"),
                PathBuf::from("c:\\vc\\include\\cstdio"),
            ])
        );
    }

    #[test]
    fn test_parse_msvc_source_dependencies_rejects_other_json() {
        assert_eq!(parse_msvc_source_dependencies("{}"), None);
        assert_eq!(parse_msvc_source_dependencies("not json"), None);
    }

    #[test]
    fn test_header_list_resolves_and_removes_source() {
        let cwd = Path::new("/proj");
        let entries = vec![
            PathBuf::from("src/main.cpp"),
            PathBuf::from("include/value.h"),
            PathBuf::from("/usr/lib/gcc/x/13/../../../../include/stdio.h"),
            PathBuf::from("./include/value.h"),
            PathBuf::from("./src/main.cpp"),
        ];
        assert_eq!(
            header_list(Path::new("/proj/src/main.cpp"), entries, cwd),
            vec![
                PathBuf::from("/proj/include/value.h"),
                PathBuf::from("/usr/lib/gcc/x/13/../../../../include/stdio.h"),
            ]
        );
    }
}
