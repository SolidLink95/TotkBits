use crate::Zstd::TotkFileType;
use std::{fs::File, io::Read, path::Path};

#[derive(Clone, Debug, Default)]
pub struct Magic {
    pub data: Vec<u8>,
}

impl Magic {
    pub fn format_name(data: &[u8]) -> Option<&'static str> {
        if Self::is_smo_save(data) {
            Some("SMO SAVE")
        } else if Self::is_byml(data) {
            Some("BYML")
        } else if Self::is_sarc(data) {
            Some("SARC")
        } else if Self::is_bphyssb(data) {
            Some("BPHYSSB")
        } else if Self::is_aamp(data) {
            Some("AAMP")
        } else if Self::is_msbt(data) {
            Some("MSBT")
        } else if Self::is_ainb(data) {
            Some("AINB")
        } else if Self::is_xlink(data) {
            Some("Xlink")
        } else if Self::is_asb(data) {
            Some("ASB")
        } else if Self::is_evfl(data) {
            Some("BFEVFL")
        } else if Self::is_restbl(data) {
            Some("RSTB")
        } else if Self::is_bfres(data) {
            Some("BFRES")
        } else if Self::is_bphcl(data) {
            Some("BPHCL")
        } else if Self::is_hkrg(data) {
            Some("HKRG")
        } else if Self::is_hkcl(data) {
            Some("HKCL")
        } else if Self::is_bntx(data) {
            Some("BNTX")
        } else if Self::is_dds(data) {
            Some("DDS")
        } else if Self::is_png(data) {
            Some("PNG")
        } else if Self::is_g1t(data) {
            Some("G1T")
        } else if Self::is_g1m(data) {
            Some("G1M")
        } else if Self::is_fbx(data) {
            Some("FBX")
        } else if Self::is_zip(data) {
            Some("ZIP")
        } else if Self::is_seven_zip(data) {
            Some("7-Zip")
        } else if Self::is_rar(data) {
            Some("RAR")
        } else if Self::is_bars(data) {
            Some("BARS")
        } else if Self::is_rfl_db(data) {
            Some("RFL_DB")
        } else if Self::is_bwav(data) {
            Some("BWAV")
        } else if Self::is_bfwav(data) {
            Some("BFWAV")
        } else if Self::is_amta(data) {
            Some("AMTA")
        } else if Self::is_riff(data) {
            Some("RIFF")
        } else if Self::is_yaz0(data) {
            Some("Yaz0")
        } else if Self::is_zstd(data) {
            Some("Zstandard")
        } else if Self::is_mcpk(data) {
            Some("MCPK")
        } else if Self::is_valid_utf8(data) {
            Some("TEXT")
        } else {
            None
        }
    }

    pub fn from_binary(data: &[u8]) -> TotkFileType {
        if Self::is_smo_save(data) {
            TotkFileType::SmoSaveFile
        } else if Self::is_ainb(data) {
            TotkFileType::AINB
        } else if Self::is_asb(data) {
            TotkFileType::ASB
        } else if Self::is_restbl(data) {
            TotkFileType::Restbl
        } else if Self::is_sarc(data) {
            TotkFileType::Sarc
        } else if Self::is_byml(data) {
            TotkFileType::Byml
        } else if Self::is_bphyssb(data) {
            TotkFileType::Bphyssb
        } else if Self::is_aamp(data) {
            TotkFileType::Aamp
        } else if Self::is_bfres(data) {
            TotkFileType::Bfres
        } else if Self::is_bntx(data) {
            TotkFileType::Bntx
        } else if Self::is_dds(data) || Self::is_png(data) || Self::is_g1t(data) {
            TotkFileType::Image
        } else if Self::is_bphcl(data) {
            TotkFileType::Bphcl
        } else if Self::is_hkrg(data) {
            TotkFileType::Hkrg
        } else if Self::is_hkcl(data) {
            TotkFileType::Hkcl
        } else if Self::is_msbt(data) {
            TotkFileType::Msbt
        } else if Self::is_evfl(data) {
            TotkFileType::Evfl
        } else if Self::is_xlink(data) {
            TotkFileType::Xlink
        } else if Self::is_zip(data)
            || Self::is_seven_zip(data)
            || Self::is_rar(data)
            || Self::is_rfl_db(data)
        {
            TotkFileType::Archive
        } else if Self::is_bars(data) {
            TotkFileType::Bars
        } else if Self::is_compressed(data) {
            TotkFileType::Compressed
        } else if Self::is_g1m(data) {
            TotkFileType::G1M
        } else if Self::is_fbx(data) {
            TotkFileType::Fbx
        } else if Self::is_bwav(data) {
            TotkFileType::Bwav
        } else if Self::is_bfwav(data) {
            TotkFileType::Bfwav
        } else if Self::is_amta(data) {
            TotkFileType::Amta
        } else if Self::is_riff(data) {
            TotkFileType::Riff
        } else if Self::is_valid_utf8(data) {
            TotkFileType::Text
        } else {
            TotkFileType::None
        }
    }

    pub fn magic_to_str(data: &[u8]) -> String {
        let mut remaining = &data[..data.len().min(8)];
        let mut result = String::new();

        while !remaining.is_empty() {
            match std::str::from_utf8(remaining) {
                Ok(text) => {
                    result.push_str(text);
                    break;
                }
                Err(error) => {
                    let valid_len = error.valid_up_to();

                    // Append the valid UTF-8 prefix.
                    if valid_len > 0 {
                        if let Ok(prefix) = std::str::from_utf8(&remaining[..valid_len]) {
                            result.push_str(prefix);
                        }
                    }

                    // Format each invalid byte as 0xAA.
                    let invalid_len = error.error_len().unwrap_or(remaining.len() - valid_len);
                    for byte in &remaining[valid_len..valid_len + invalid_len] {
                        result.push_str(&format!("0x{byte:02X}"));
                    }

                    remaining = &remaining[valid_len + invalid_len..];
                }
            }
        }

        result
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> TotkFileType {
        if path.as_ref().is_dir() {
            return TotkFileType::Archive;
        }
        let Ok(mut file) = File::open(path) else {
            return TotkFileType::None;
        };
        let mut data = [0_u8; 18];
        let _ = file.read_exact(&mut data);
        Self::from_binary(&data)
    }

    pub fn is_valid_utf8(data: &[u8]) -> bool {
        const MAX_UTF8_CHECK_BYTES: usize = 1024 * 1024;
        !data.is_empty()
            && std::str::from_utf8(&data[..data.len().min(MAX_UTF8_CHECK_BYTES)]).is_ok()
    }

    #[inline]
    pub fn is_compressed(data: &[u8]) -> bool {
        Self::is_mcpk(data) || Self::is_zstd(data) || Self::is_yaz0(data)
    }
    #[inline]
    pub fn is_smo_save(data: &[u8]) -> bool {
        const SMO_HEADER_SIZE: usize = 16;
        const SMO_SAVE_FILE_SIZE: usize = 0x20000C;
        data.len() == SMO_SAVE_FILE_SIZE && Self::is_byml(&data[SMO_HEADER_SIZE..])
    }
    #[inline]
    pub fn is_byml(data: &[u8]) -> bool {
        data.starts_with(b"BY") || data.starts_with(b"YB")
    }
    #[inline]
    pub fn is_byml_little_endian(data: &[u8]) -> bool {
        data.starts_with(b"YB")
    }
    #[inline]
    pub fn is_byml_big_endian(data: &[u8]) -> bool {
        data.starts_with(b"BY")
    }
    #[inline]
    pub fn is_sarc(data: &[u8]) -> bool {
        data.starts_with(b"SARC")
    }
    #[inline]
    pub fn is_aamp(data: &[u8]) -> bool {
        data.starts_with(b"AAMP")
    }
    #[inline]
    pub fn is_msbt(data: &[u8]) -> bool {
        data.starts_with(b"MsgStd")
    }
    #[inline]
    pub fn is_ainb(data: &[u8]) -> bool {
        data.starts_with(b"AIB")
    }
    #[inline]
    pub fn is_xlink(data: &[u8]) -> bool {
        data.starts_with(b"XLNK")
    }
    #[inline]
    pub fn is_asb(data: &[u8]) -> bool {
        data.starts_with(b"ASB ")
    }
    #[inline]
    pub fn is_evfl(data: &[u8]) -> bool {
        data.starts_with(b"BFEVFL") && data.len() >= 14 && data.get(12..14) == Some(&[0xFF, 0xFE])
    }
    #[inline]
    pub fn is_restbl(data: &[u8]) -> bool {
        data.starts_with(b"RSTB") || data.starts_with(b"REST")
    }
    #[inline]
    pub fn is_restbl_dynamic(data: &[u8]) -> bool {
        data.starts_with(b"RESTBL")
    }
    #[inline]
    pub fn is_bfres(data: &[u8]) -> bool {
        data.starts_with(b"FRES")
    }
    #[inline]
    pub fn is_bntx(data: &[u8]) -> bool {
        data.starts_with(b"BNTX")
    }
    #[inline]
    pub fn is_bphcl(data: &[u8]) -> bool {
        data.starts_with(b"Phive\0")
    }
    #[inline]
    pub fn is_hkcl(data: &[u8]) -> bool {
        data.starts_with(&[0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10])
    }
    /// A Havok packfile whose class table names the ragdoll instance; the
    /// class-name section precedes the data, so the scan stays cheap.
    #[inline]
    pub fn is_hkrg(data: &[u8]) -> bool {
        const CLASS: &[u8] = b"hkaRagdollInstance\0";
        Self::is_hkcl(data)
            && data
                .windows(CLASS.len())
                .take(1 << 20)
                .any(|window| window == CLASS)
    }
    /// An AAMP archive whose parameter type is BotW's `physsb`.
    #[inline]
    pub fn is_bphyssb(data: &[u8]) -> bool {
        Self::is_aamp(data) && data.get(0x30..0x37) == Some(b"physsb\0")
    }
    #[inline]
    pub fn is_g1m(data: &[u8]) -> bool {
        data.starts_with(b"G1M_") || data.starts_with(b"_M1G")
    }
    #[inline]
    pub fn is_fbx(data: &[u8]) -> bool {
        data.starts_with(b"Kaydara FBX Binary")
    }
    #[inline]
    pub fn is_glb(data: &[u8]) -> bool {
        data.starts_with(b"glTF")
    }
    #[inline]
    pub fn is_dds(data: &[u8]) -> bool {
        data.starts_with(b"DDS ")
    }
    #[inline]
    pub fn is_png(data: &[u8]) -> bool {
        data.starts_with(b"\x89PNG\r\n\x1A\n")
    }
    #[inline]
    pub fn is_zip(data: &[u8]) -> bool {
        data.starts_with(b"PK\x03\x04")
            || data.starts_with(b"PK\x05\x06")
            || data.starts_with(b"PK\x07\x08")
    }
    #[inline]
    pub fn is_seven_zip(data: &[u8]) -> bool {
        data.starts_with(b"7z\xBC\xAF\x27\x1C")
    }
    #[inline]
    pub fn is_rar(data: &[u8]) -> bool {
        data.starts_with(b"Rar!\x1A\x07\x00") || data.starts_with(b"Rar!\x1A\x07\x01\x00")
    }
    #[inline]
    pub fn is_bars(data: &[u8]) -> bool {
        data.starts_with(b"BARS")
    }
    #[inline]
    pub fn is_rfl_db(data: &[u8]) -> bool {
        data.starts_with(b"RNOD")
    }
    #[inline]
    pub fn is_bwav(data: &[u8]) -> bool {
        data.starts_with(b"BWAV")
    }
    #[inline]
    pub fn is_bfwav(data: &[u8]) -> bool {
        data.starts_with(b"FWAV")
    }
    #[inline]
    pub fn is_amta(data: &[u8]) -> bool {
        data.starts_with(b"AMTA")
    }
    #[inline]
    pub fn is_riff(data: &[u8]) -> bool {
        data.starts_with(b"RIFF")
    }
    #[inline]
    pub fn is_g1t(data: &[u8]) -> bool {
        data.starts_with(b"GT1G") || data.starts_with(b"G1TG")
    }
    #[inline]
    pub fn is_mcpk(data: &[u8]) -> bool {
        data.starts_with(b"MCPK")
    }
    #[inline]
    pub fn is_zstd(data: &[u8]) -> bool {
        data.starts_with(&[0x28, 0xB5, 0x2F, 0xFD])
    }
    #[inline]
    pub fn is_yaz0(data: &[u8]) -> bool {
        data.starts_with(b"Yaz0")
    }
    #[inline]
    pub fn is_amta_or_bars(data: &[u8]) -> bool {
        Self::is_amta(data) || Self::is_bars(data)
    }
}

#[cfg(test)]
mod tests {
    use super::Magic;
    use crate::Zstd::TotkFileType;
    use std::{fs, path::Path, time::SystemTime};

    #[test]
    fn classifies_binary_and_zero_fills_short_files() {
        assert_eq!(Magic::from_binary(b"FRESpayload"), TotkFileType::Bfres);
        assert_eq!(
            Magic::from_binary(b"\x28\xB5\x2F\xFDpayload"),
            TotkFileType::Compressed
        );
        assert_eq!(Magic::from_binary(b"DDS payload"), TotkFileType::Image);
        assert_eq!(
            Magic::from_binary(b"PK\x03\x04payload"),
            TotkFileType::Archive
        );
        assert_eq!(Magic::from_binary(b"BARSpayload"), TotkFileType::Bars);
        assert_eq!(Magic::from_binary(b"RNODpayload"), TotkFileType::Archive);
        assert_eq!(Magic::format_name(b"RNODpayload"), Some("RFL_DB"));
        assert_eq!(Magic::from_binary(b"G1M_payload"), TotkFileType::G1M);
        assert_eq!(Magic::from_binary(b"unknown"), TotkFileType::Text);
        assert_eq!(Magic::from_binary(b"BWAVpayload"), TotkFileType::Bwav);
        assert_eq!(Magic::from_binary(b"FWAVpayload"), TotkFileType::Bfwav);
        assert_eq!(Magic::from_binary(b"AMTApayload"), TotkFileType::Amta);
        assert_eq!(Magic::from_binary(b"RIFFpayload"), TotkFileType::Riff);
        assert_eq!(Magic::from_binary(b"BNTXpayload"), TotkFileType::Bntx);

        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../tmp/magic-short-{}-{unique}.test",
            std::process::id()
        ));
        fs::write(&path, b"AIB").unwrap();
        assert_eq!(Magic::from_file(&path), TotkFileType::AINB);
        fs::remove_file(path).unwrap();
    }
}
