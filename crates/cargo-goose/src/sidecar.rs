//! Editing a Goose sidecar (`foo.build`): the `dylibs_<platform>` entry
//! that maps a library name an extern block uses to the file to open.

use std::path::Path;

/// The entries of a list value `["a", "b"]`, as written.
fn parse_list(v: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut esc = false;
    for c in v.chars() {
        if in_str {
            if esc {
                cur.push(c);
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
                out.push(std::mem::take(&mut cur));
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_str = true;
        }
    }
    out
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Sets `name=spelling` in the sidecar's `dylibs_<platform>` list, keeping
/// everything else in the file as it was. Creates the file if there is none.
pub fn set_dylib(sidecar: &Path, platform: &str, name: &str, spelling: &str) -> Result<bool, String> {
    let key = format!("dylibs_{platform}");
    let text = std::fs::read_to_string(sidecar).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let entry = format!("{name}={spelling}");
    let mut found = false;
    for l in lines.iter_mut() {
        let t = l.trim_start();
        let Some(rest) = t.strip_prefix(&key) else { continue };
        let rest = rest.trim_start();
        let Some(value) = rest.strip_prefix('=') else { continue };
        // One entry per line is what this edits; a list spread over lines
        // is left alone and a new line would duplicate the key.
        let value = value.trim().trim_end_matches(';').trim();
        if !(value.starts_with('[') && value.ends_with(']')) {
            return Err(format!("{}: {key} is not a one-line list; add {entry} to it yourself", sidecar.display()));
        }
        let mut items = parse_list(value);
        items.retain(|i| !i.starts_with(&format!("{name}=")));
        items.push(entry.clone());
        let list: Vec<String> = items.iter().map(|i| quote(i)).collect();
        *l = format!("{key} = [{}];", list.join(", "));
        found = true;
    }
    if !found {
        lines.push(format!("{key} = [{}];   // written by cargo goose", quote(&entry)));
    }
    let mut new = lines.join("\n");
    new.push('\n');
    if new == text {
        return Ok(false);
    }
    std::fs::write(sidecar, new).map_err(|e| format!("cannot write {}: {e}", sidecar.display()))?;
    Ok(true)
}
