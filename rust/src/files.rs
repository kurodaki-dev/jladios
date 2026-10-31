//! Accès aux fichiers de l'appareil pour `/unlock` (stockage partagé + dossier de l'app).
//! Nécessite l'autorisation Android « Accès à tous les fichiers ».

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const ROOT: &str = "/storage/emulated/0";
const READ_CHUNK: usize = 6000;

fn resolve(p: &str) -> PathBuf {
    let p = p.trim().trim_matches('"');
    if p.is_empty() || p == "~" {
        PathBuf::from(ROOT)
    } else if let Some(r) = p.strip_prefix("~/") {
        Path::new(ROOT).join(r)
    } else if p.starts_with('/') {
        PathBuf::from(p)
    } else {
        Path::new(ROOT).join(p)
    }
}

fn err(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "ERREUR : permission refusée (autorise « Accès à tous les fichiers » pour Jladios dans les réglages Android)".into()
    } else {
        format!("ERREUR : {e}")
    }
}

fn human(n: u64) -> String {
    match n {
        0..=1023 => format!("{n} o"),
        1024..=1_048_575 => format!("{:.1} Ko", n as f64 / 1024.0),
        1_048_576..=1_073_741_823 => format!("{:.1} Mo", n as f64 / 1_048_576.0),
        _ => format!("{:.2} Go", n as f64 / 1_073_741_824.0),
    }
}

/// Refuse de supprimer la racine du stockage ou un dossier système.
fn protected(p: &Path) -> bool {
    p == Path::new(ROOT) || p.components().count() < 3
}

/// Exécute une commande `files_xxx <chemin>[\ncontenu]`.
pub fn run(action: &str) -> String {
    let (head, body) = action.split_once('\n').unwrap_or((action, ""));
    let (verb, rest) = head
        .split_once(char::is_whitespace)
        .map(|(v, r)| (v, r.trim()))
        .unwrap_or((head, ""));
    match verb {
        "files_ls" => ls(rest),
        "files_read" => read(rest),
        "files_write" => write(rest, body, false),
        "files_append" => write(rest, body, true),
        "files_mkdir" => fs::create_dir_all(resolve(rest)).map(|_| "ok".to_string()).unwrap_or_else(err),
        "files_rm" => rm(rest),
        "files_mv" => mv(rest),
        "files_cp" => cp(rest),
        "files_find" => find(rest),
        "files_info" => info(rest),
        _ => format!("ERREUR : commande de fichiers inconnue '{verb}'"),
    }
}

fn ls(path: &str) -> String {
    let p = resolve(path);
    let rd = match fs::read_dir(&p) {
        Ok(r) => r,
        Err(e) => return err(e),
    };
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        match e.metadata() {
            Ok(m) if m.is_dir() => dirs.push(format!("{name}/")),
            Ok(m) => files.push(format!("{name} ({})", human(m.len()))),
            Err(_) => files.push(name),
        }
    }
    dirs.sort_by_key(|s| s.to_lowercase());
    files.sort_by_key(|s| s.to_lowercase());
    let total = dirs.len() + files.len();
    let mut out = format!("{} : {total} éléments\n", p.display());
    for l in dirs.iter().chain(files.iter()).take(150) {
        out.push_str(l);
        out.push('\n');
    }
    if total > 150 {
        out.push_str(&format!("... ({} de plus, précise le dossier ou utilise files_find)\n", total - 150));
    }
    out
}

fn read(arg: &str) -> String {
    let (path, off) = arg.split_once(" | ").map(|(p, o)| (p, o.trim().parse::<u64>().unwrap_or(0))).unwrap_or((arg, 0));
    let p = resolve(path);
    let mut f = match File::open(&p) {
        Ok(f) => f,
        Err(e) => return err(e),
    };
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    if off > 0 && f.seek(SeekFrom::Start(off)).is_err() {
        return "ERREUR : position invalide".into();
    }
    let mut buf = vec![0u8; READ_CHUNK];
    let n = f.read(&mut buf).unwrap_or(0);
    buf.truncate(n);
    if buf.iter().filter(|b| **b == 0).count() > 4 {
        return format!("fichier binaire de {} (non lisible comme texte)", human(size));
    }
    let mut out = String::from_utf8_lossy(&buf).into_owned();
    let next = off + n as u64;
    if next < size {
        out.push_str(&format!("\n[... suite : files_read {} | {next}  (taille totale {})]", p.display(), human(size)));
    }
    out
}

fn write(path: &str, body: &str, append: bool) -> String {
    if path.is_empty() {
        return "ERREUR : usage files_write <chemin> puis le contenu à la ligne suivante".into();
    }
    let p = resolve(path);
    if let Some(parent) = p.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return err(e);
        }
    }
    let res = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&p)
        .and_then(|mut f| f.write_all(body.as_bytes()));
    match res {
        Ok(()) => format!("ok : {} octets écrits dans {}", body.len(), p.display()),
        Err(e) => err(e),
    }
}

fn rm(arg: &str) -> String {
    let (recursive, path) = match arg.strip_prefix("-r ") {
        Some(p) => (true, p),
        None => (false, arg),
    };
    let p = resolve(path);
    if protected(&p) {
        return "ERREUR : suppression refusée (dossier protégé)".into();
    }
    let res = match fs::metadata(&p) {
        Ok(m) if m.is_dir() => {
            if recursive { fs::remove_dir_all(&p) } else { fs::remove_dir(&p) }
        }
        Ok(_) => fs::remove_file(&p),
        Err(e) => return err(e),
    };
    res.map(|_| format!("ok : {} supprimé", p.display())).unwrap_or_else(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied || e.to_string().contains("not empty") {
            format!("ERREUR : {e} (pour un dossier non vide : files_rm -r <dossier>)")
        } else {
            err(e)
        }
    })
}

fn two_paths(arg: &str) -> Option<(PathBuf, PathBuf)> {
    arg.split_once(" -> ").map(|(a, b)| (resolve(a), resolve(b)))
}

fn mv(arg: &str) -> String {
    let Some((a, b)) = two_paths(arg) else { return "ERREUR : usage files_mv <source> -> <destination>".into() };
    if protected(&a) {
        return "ERREUR : déplacement refusé (dossier protégé)".into();
    }
    let b = if b.is_dir() { b.join(a.file_name().unwrap_or_default()) } else { b };
    if let Some(parent) = b.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if fs::rename(&a, &b).is_ok() {
        return format!("ok : déplacé vers {}", b.display());
    }
    // autre volume : copie puis suppression
    match copy_any(&a, &b).and_then(|_| if a.is_dir() { fs::remove_dir_all(&a) } else { fs::remove_file(&a) }) {
        Ok(()) => format!("ok : déplacé vers {}", b.display()),
        Err(e) => err(e),
    }
}

fn copy_any(a: &Path, b: &Path) -> std::io::Result<()> {
    if a.is_dir() {
        fs::create_dir_all(b)?;
        for e in fs::read_dir(a)? {
            let e = e?;
            copy_any(&e.path(), &b.join(e.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(a, b).map(|_| ())
    }
}

fn cp(arg: &str) -> String {
    let Some((a, b)) = two_paths(arg) else { return "ERREUR : usage files_cp <source> -> <destination>".into() };
    let b = if b.is_dir() { b.join(a.file_name().unwrap_or_default()) } else { b };
    if let Some(parent) = b.parent() {
        let _ = fs::create_dir_all(parent);
    }
    copy_any(&a, &b).map(|_| format!("ok : copié vers {}", b.display())).unwrap_or_else(err)
}

fn walk(dir: &Path, needle: &str, depth: usize, out: &mut Vec<String>) {
    if depth > 8 || out.len() >= 60 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        let path = e.path();
        let is_dir = e.metadata().map(|m| m.is_dir()).unwrap_or(false);
        if name.contains(needle) {
            out.push(format!("{}{}", path.display(), if is_dir { "/" } else { "" }));
        }
        if is_dir {
            walk(&path, needle, depth + 1, out);
        }
        if out.len() >= 60 {
            return;
        }
    }
}

fn find(arg: &str) -> String {
    let (dir, pat) = arg.split_once(" | ").unwrap_or(("~", arg));
    let needle = pat.trim().to_lowercase();
    if needle.is_empty() {
        return "ERREUR : usage files_find <dossier> | <morceau du nom>".into();
    }
    let mut out = Vec::new();
    walk(&resolve(dir), &needle, 0, &mut out);
    if out.is_empty() {
        format!("aucun fichier dont le nom contient '{}'", pat.trim())
    } else {
        let cap = if out.len() >= 60 { " (60 premiers résultats)" } else { "" };
        format!("{}{cap}\n", out.join("\n"))
    }
}

fn info(path: &str) -> String {
    let p = resolve(path);
    match fs::metadata(&p) {
        Ok(m) => {
            let age = m
                .modified()
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .map(|d| format!("modifié il y a {} min", d.as_secs() / 60))
                .unwrap_or_default();
            format!(
                "{} : {} {} {} {}",
                p.display(),
                if m.is_dir() { "dossier" } else { "fichier" },
                human(m.len()),
                if m.permissions().readonly() { "lecture seule" } else { "modifiable" },
                age
            )
        }
        Err(e) => err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let d = std::env::temp_dir().join(format!("jl-files-{}", std::process::id()));
        let f = d.join("a/b.txt");
        let fs = f.to_string_lossy().into_owned();
        assert!(run(&format!("files_write {fs}\nsalut")).starts_with("ok"));
        assert!(run(&format!("files_append {fs}\n !")).starts_with("ok"));
        assert_eq!(run(&format!("files_read {fs}")), "salut !");
        assert!(run(&format!("files_find {} | B.TX", d.display())).contains("b.txt"));
        assert!(run(&format!("files_mv {fs} -> {}/c.txt", d.display())).starts_with("ok"));
        assert!(run(&format!("files_ls {}", d.display())).contains("c.txt"));
        assert!(run(&format!("files_rm -r {}", d.display())).starts_with("ok"));
        assert!(run("files_rm /").starts_with("ERREUR"));
    }
}
