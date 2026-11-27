//! Keeps the previous versions of a managed binary, so the user can roll back in
//! one click when an update breaks a site.
//!
//! Today `check_ytdlp_freshness` re-downloads by 2-day mtime and the previous
//! version is simply overwritten: if the new one breaks, there is nowhere to go
//! back to and the user is left without downloads until upstream fixes it.
//!
//! It fits into `replace_managed_binary`, which is already atomic — this module
//! only resolves the names and the pruning, which is the decidable and testable
//! part without a network.
//!
//! Origin: Nix and pnpm, the idea of keeping the previous generation alive.

use std::path::{Path, PathBuf};

/// How many previous versions are kept. Three covers the real case (today's
/// update broke, roll back to yesterday's) without becoming 100 MB of clutter.
pub const KEEP_VERSIONS: usize = 3;

const SUFFIX: &str = ".gumballz-get-prev.";

/// Stamped archive path for `binary`.
///
/// The stamp goes at the end, not in the middle of the name, so `find_tool`
/// keeps seeing only the active binary: it looks up by exact name, and an
/// archived `yt-dlp` as `yt-dlp.gumballz-get-prev.123` does not collide.
pub fn archive_path(binary: &Path, stamp_ms: u128) -> PathBuf {
    let name = binary
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("binary");
    binary.with_file_name(format!("{name}{SUFFIX}{stamp_ms}"))
}

/// `Some(stamp)` when the file is an archive of `binary_name`.
pub fn parse_archive(file_name: &str, binary_name: &str) -> Option<u128> {
    let rest = file_name.strip_prefix(binary_name)?;
    let stamp = rest.strip_prefix(SUFFIX)?;
    stamp.parse().ok()
}

/// Archived versions of `binary`, newest first.
pub fn list_archived(binary: &Path) -> Vec<(u128, PathBuf)> {
    let (Some(dir), Some(name)) = (binary.parent(), binary.file_name().and_then(|n| n.to_str()))
    else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut out: Vec<(u128, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let fname = e.file_name();
            let fname = fname.to_str()?;
            parse_archive(fname, name).map(|stamp| (stamp, e.path()))
        })
        .collect();
    out.sort_by_key(|(stamp, _)| std::cmp::Reverse(*stamp));
    out
}

/// Which archives should be deleted to respect `KEEP_VERSIONS`.
///
/// Kept separate from the deletion on purpose: deciding what to delete is
/// testable without touching disk, and deleting a file is the part you do not
/// want to get wrong.
pub fn prune_list(archived: &[(u128, PathBuf)], keep: usize) -> Vec<PathBuf> {
    archived.iter().skip(keep).map(|(_, p)| p.clone()).collect()
}

/// Moves the current binary to the stamped archive, before installing the new one.
pub fn archive_current(binary: &Path, stamp_ms: u128) -> std::io::Result<Option<PathBuf>> {
    if !binary.exists() {
        return Ok(None);
    }
    let dest = archive_path(binary, stamp_ms);
    std::fs::rename(binary, &dest)?;

    for old in prune_list(&list_archived(binary), KEEP_VERSIONS) {
        let _ = std::fs::remove_file(old);
    }
    Ok(Some(dest))
}

/// Restores an archive over the active binary.
///
/// Refuses a path that is not an archive of **this** binary: it is a path that
/// deletes and overwrites, and accepting an arbitrary path here would let any
/// caller replace a managed executable.
pub fn rollback_to(binary: &Path, archived: &Path) -> std::io::Result<()> {
    let name = binary
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| std::io::Error::other("binary has no file name"))?;
    let arch_name = archived
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| std::io::Error::other("archive has no file name"))?;

    if parse_archive(arch_name, name).is_none() {
        return Err(std::io::Error::other(format!(
            "{arch_name} is not an archive of {name}"
        )));
    }
    if archived.parent() != binary.parent() {
        return Err(std::io::Error::other(
            "archive must live beside the managed binary",
        ));
    }

    std::fs::rename(archived, binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "gumballz-get-bv-{nome}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn o_arquivamento_nao_colide_com_a_busca_pelo_binario_ativo() {
        // find_tool looks up by exact name; the stamp has to go at the end.
        let p = archive_path(Path::new("/bin/yt-dlp"), 1234);
        assert_eq!(
            p.file_name().unwrap().to_str().unwrap(),
            "yt-dlp.gumballz-get-prev.1234"
        );
        assert_ne!(p.file_name().unwrap(), "yt-dlp");
        assert_eq!(
            parse_archive("yt-dlp.gumballz-get-prev.1234", "yt-dlp"),
            Some(1234)
        );
        assert_eq!(parse_archive("yt-dlp", "yt-dlp"), None);
        assert_eq!(parse_archive("ffmpeg.gumballz-get-prev.1", "yt-dlp"), None);
    }

    #[test]
    fn arquivar_move_e_lista_da_mais_nova_para_a_mais_antiga() {
        let dir = tmp("lista");
        let bin = dir.join("yt-dlp");

        for stamp in [100u128, 300, 200] {
            std::fs::write(&bin, format!("versao {stamp}")).unwrap();
            archive_current(&bin, stamp).unwrap();
            assert!(!bin.exists(), "the active binary should have moved away");
        }

        let arch = list_archived(&bin);
        assert_eq!(
            arch.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![300, 200, 100]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_poda_mantem_exatamente_as_n_mais_novas() {
        let arch: Vec<(u128, PathBuf)> = (1..=6)
            .rev()
            .map(|i| (i as u128, PathBuf::from(format!("/x/b.gumballz-get-prev.{i}"))))
            .collect();
        let apagar = prune_list(&arch, 3);
        assert_eq!(apagar.len(), 3);
        // Deletes the oldest ones, never the newest.
        assert!(apagar.iter().all(|p| {
            let n: u128 = p
                .to_str()
                .unwrap()
                .rsplit('.')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            n <= 3
        }));
        assert!(prune_list(&arch, 10).is_empty());
    }

    #[test]
    fn arquivar_poda_sozinho_ao_passar_do_limite() {
        let dir = tmp("poda");
        let bin = dir.join("yt-dlp");
        for stamp in 1..=(KEEP_VERSIONS as u128 + 2) {
            std::fs::write(&bin, "x").unwrap();
            archive_current(&bin, stamp).unwrap();
        }
        assert_eq!(list_archived(&bin).len(), KEEP_VERSIONS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rollback_restaura_o_conteudo_da_versao_escolhida() {
        let dir = tmp("rollback");
        let bin = dir.join("yt-dlp");

        std::fs::write(&bin, "versao boa").unwrap();
        let arquivado = archive_current(&bin, 111).unwrap().unwrap();
        std::fs::write(&bin, "versao quebrada").unwrap();

        rollback_to(&bin, &arquivado).unwrap();
        assert_eq!(std::fs::read_to_string(&bin).unwrap(), "versao boa");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rollback_recusa_caminho_que_nao_e_arquivamento_deste_binario() {
        // A path that deletes and overwrites an executable: accepting an
        // arbitrary path here would let any caller swap a managed binary.
        let dir = tmp("guarda");
        let bin = dir.join("yt-dlp");
        std::fs::write(&bin, "ativo").unwrap();

        let intruso = dir.join("qualquer-coisa");
        std::fs::write(&intruso, "malicioso").unwrap();
        assert!(rollback_to(&bin, &intruso).is_err());

        let de_outro = dir.join("ffmpeg.gumballz-get-prev.1");
        std::fs::write(&de_outro, "outro binario").unwrap();
        assert!(rollback_to(&bin, &de_outro).is_err());

        let fora = std::env::temp_dir().join("yt-dlp.gumballz-get-prev.9");
        std::fs::write(&fora, "de fora").unwrap();
        assert!(
            rollback_to(&bin, &fora).is_err(),
            "an archive outside the managed directory must be refused"
        );

        assert_eq!(std::fs::read_to_string(&bin).unwrap(), "ativo");
        let _ = std::fs::remove_file(&fora);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arquivar_binario_inexistente_nao_e_erro() {
        let dir = tmp("ausente");
        let r = archive_current(&dir.join("nao-existe"), 1).unwrap();
        assert!(r.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
