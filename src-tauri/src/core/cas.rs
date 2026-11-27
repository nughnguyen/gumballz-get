//! Content-addressed store: the same video in three qualities, or the same
//! PDF in four courses, takes up space once.
//!
//! The real file lives in the store under its content hash, and each place it
//! appears is a hardlink. Deleting one copy does not delete the others — the
//! kernel only frees the space when the last link goes away.
//!
//! Origin: restic and borg. Their content-defined chunking stays out: here the
//! whole file is the unit, which loses partial dedup but needs no own format
//! and keeps the files readable by any program.
//!
//! **Known limit:** a hardlink does not cross filesystems. The app lets the
//! user pick the output folder, so copying is the normal path, not the
//! exception.

use std::path::{Path, PathBuf};

/// Path of a content inside the store, sharded by the first two characters
/// of the hash.
///
/// Sharding exists because a single directory with tens of thousands of
/// entries degrades listing on any filesystem.
///
/// Rejects a hash that is not hexadecimal SHA-256: the value becomes a path
/// component, and accepting arbitrary input here would be directory traversal.
pub fn object_path(store_root: &Path, sha256: &str) -> Option<PathBuf> {
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let lower = sha256.to_lowercase();
    let (prefix, rest) = lower.split_at(2);
    Some(store_root.join(prefix).join(rest))
}

/// How the file was materialized at the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkOutcome {
    /// Hardlink: space shared with the store.
    Linked,
    /// Copy: destination on another filesystem, or hardlink refused.
    Copied,
    /// Destination already pointed at the same content.
    AlreadyPresent,
}

/// Puts `sha256` into the store, from `source`, without duplicating if it already exists.
pub fn ingest(store_root: &Path, source: &Path, sha256: &str) -> std::io::Result<PathBuf> {
    let object = object_path(store_root, sha256)
        .ok_or_else(|| std::io::Error::other(format!("invalid sha256: {sha256:?}")))?;

    if object.exists() {
        return Ok(object);
    }
    if let Some(parent) = object.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Hardlink first: ingesting must not duplicate the space while the
    // source file still exists.
    match std::fs::hard_link(source, &object) {
        Ok(()) => Ok(object),
        Err(_) => {
            std::fs::copy(source, &object)?;
            Ok(object)
        }
    }
}

/// Materializa o objeto do store em `dest`, preferindo hardlink.
pub fn materialize(object: &Path, dest: &Path) -> std::io::Result<LinkOutcome> {
    if dest.exists() && same_file(object, dest) {
        return Ok(LinkOutcome::AlreadyPresent);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    match std::fs::hard_link(object, dest) {
        Ok(()) => Ok(LinkOutcome::Linked),
        Err(_) => {
            // A different volume is the normal case, not the exception: the
            // output folder is picked by the user and is usually on another disk.
            std::fs::copy(object, dest)?;
            Ok(LinkOutcome::Copied)
        }
    }
}

/// Do two paths point at the same inode?
pub fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        // Without `dev`/`ino` on stable Windows (`volume_serial_number` and
        // `file_index` are still unstable), the conservative answer is "don't
        // know, treat as different". Practical effect: `materialize` over a
        // destination that is already the same file re-links instead of
        // returning `AlreadyPresent`. Idempotent and correct, just a bit more
        // expensive.
        let _ = (a, b);
        false
    }
}

/// How many links point at this object. `1` means only the store holds it
/// and deleting frees real space.
#[cfg(unix)]
pub fn link_count(object: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(object).ok().map(|m| m.nlink())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "gumballz-get-cas-{nome}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const H: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn o_caminho_e_fatiado_pelos_dois_primeiros_caracteres() {
        let p = object_path(Path::new("/store"), H).unwrap();
        assert_eq!(p, Path::new("/store/e3").join(&H[2..]));
    }

    #[test]
    fn hash_invalido_nao_vira_caminho() {
        // The hash is a path component: accepting arbitrary input would be
        // directory traversal.
        for ruim in [
            "../../etc/passwd",
            "e3b0",
            "",
            &"z".repeat(64),
            &format!("{}/x", &H[..62]),
        ] {
            assert!(
                object_path(Path::new("/store"), ruim).is_none(),
                "accepted {ruim:?}"
            );
        }
    }

    #[test]
    fn caixa_do_hash_nao_cria_dois_objetos() {
        let a = object_path(Path::new("/s"), H).unwrap();
        let b = object_path(Path::new("/s"), &H.to_uppercase()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn ingerir_duas_vezes_nao_duplica() {
        let dir = tmp("ingest");
        let store = dir.join("store");
        let src = dir.join("video.mp4");
        std::fs::write(&src, "conteudo").unwrap();

        let o1 = ingest(&store, &src, H).unwrap();
        let o2 = ingest(&store, &src, H).unwrap();
        assert_eq!(o1, o2);
        assert!(o1.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn materializar_no_mesmo_volume_compartilha_o_inode() {
        // The point of the feature: three qualities of the same video take up
        // space once. If this becomes a copy, the feature does not exist.
        let dir = tmp("link");
        let store = dir.join("store");
        let src = dir.join("original.mp4");
        std::fs::write(&src, "bytes do video").unwrap();
        let object = ingest(&store, &src, H).unwrap();

        let d1 = dir.join("curso-a/aula.mp4");
        let d2 = dir.join("curso-b/aula.mp4");
        assert_eq!(materialize(&object, &d1).unwrap(), LinkOutcome::Linked);
        assert_eq!(materialize(&object, &d2).unwrap(), LinkOutcome::Linked);

        // Behavioural check, not by inode: writing through one link and reading
        // through the other only works if both point at the same file.
        // `same_file` cannot answer that on stable Windows (the volume/index
        // APIs are still unstable), so asserting inode identity would fail
        // the test somewhere the feature works.
        std::fs::write(&d1, "editado por d1").unwrap();
        assert_eq!(
            std::fs::read_to_string(&d2).unwrap(),
            "editado por d1",
            "d1 and d2 should have been the same file"
        );

        #[cfg(unix)]
        {
            assert!(same_file(&d1, &d2));
            // object + src + d1 + d2
            assert!(
                link_count(&object).unwrap() >= 3,
                "expected shared links"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apagar_uma_copia_nao_afeta_as_outras() {
        let dir = tmp("unlink");
        let store = dir.join("store");
        let src = dir.join("o.bin");
        std::fs::write(&src, "dados").unwrap();
        let object = ingest(&store, &src, H).unwrap();

        let d1 = dir.join("a/x.bin");
        let d2 = dir.join("b/x.bin");
        materialize(&object, &d1).unwrap();
        materialize(&object, &d2).unwrap();

        std::fs::remove_file(&d1).unwrap();
        assert!(!d1.exists());
        assert_eq!(std::fs::read_to_string(&d2).unwrap(), "dados");
        assert!(object.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn materializar_por_cima_do_mesmo_conteudo_e_no_op() {
        let dir = tmp("noop");
        let store = dir.join("store");
        let src = dir.join("o.bin");
        std::fs::write(&src, "dados").unwrap();
        let object = ingest(&store, &src, H).unwrap();
        let dest = dir.join("x.bin");

        assert_eq!(materialize(&object, &dest).unwrap(), LinkOutcome::Linked);
        #[cfg(unix)]
        assert_eq!(
            materialize(&object, &dest).unwrap(),
            LinkOutcome::AlreadyPresent
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn materializar_substitui_arquivo_diferente_que_estava_no_destino() {
        let dir = tmp("replace");
        let store = dir.join("store");
        let src = dir.join("o.bin");
        std::fs::write(&src, "novo").unwrap();
        let object = ingest(&store, &src, H).unwrap();

        let dest = dir.join("x.bin");
        std::fs::write(&dest, "antigo e diferente").unwrap();
        materialize(&object, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "novo");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
