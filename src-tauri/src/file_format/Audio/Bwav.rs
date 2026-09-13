use super::{Bfwav, Bfwav::DecodedAudio};
use crate::parser::binary::{BinaryPatcher, BinaryReader, BinaryWriter};

const INFO_SIZE: usize = 0x4c;

fn u16_at(data: &[u8], at: usize) -> Result<u16, String> {
    BinaryReader::new(data)
        .read_u16_at(at)
        .map_err(|_| "truncated BWAV".into())
}

fn i16_at(data: &[u8], at: usize) -> Result<i16, String> {
    Ok(u16_at(data, at)? as i16)
}

fn u32_at(data: &[u8], at: usize) -> Result<u32, String> {
    BinaryReader::new(data)
        .read_u32_at(at)
        .map_err(|_| "truncated BWAV".into())
}

fn put_u16(data: &mut [u8], at: usize, value: u16) -> Result<(), String> {
    BinaryPatcher::new(data)
        .write_u16_at(at, value)
        .map_err(|_| "truncated BWAV output".into())
}

fn put_u32(data: &mut [u8], at: usize, value: u32) -> Result<(), String> {
    BinaryPatcher::new(data)
        .write_u32_at(at, value)
        .map_err(|_| "truncated BWAV output".into())
}

fn put_bytes(data: &mut [u8], at: usize, value: &[u8]) -> Result<(), String> {
    BinaryPatcher::new(data)
        .write_bytes_at(at, value)
        .map_err(|_| "truncated BWAV output".into())
}

fn align(value: usize, boundary: usize) -> usize {
    (value + boundary - 1) & !(boundary - 1)
}

fn crc32(parts: &[Vec<u8>]) -> u32 {
    let mut crc = u32::MAX;
    for byte in parts.iter().flat_map(|part| part.iter().copied()) {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn header(data: &[u8]) -> Result<(usize, usize), String> {
    if !crate::Settings::Magic::is_bwav(data) || data.len() < 0x10 {
        return Err("not a BWAV file".into());
    }
    if data.get(4..6) != Some(b"\xff\xfe") {
        return Err("unsupported BWAV byte order".into());
    }
    let channels = u16_at(data, 0x0e)? as usize;
    if channels == 0 || channels > 32 || 0x10 + channels * INFO_SIZE > data.len() {
        return Err(format!("invalid BWAV channel count {channels}"));
    }
    Ok((channels, u32_at(data, 0x1c)? as usize))
}

fn coefficients(data: &[u8], channel: usize) -> Result<[i16; 16], String> {
    let at = 0x10 + channel * INFO_SIZE + 0x10;
    let mut result = [0i16; 16];
    for (index, value) in result.iter_mut().enumerate() {
        *value = i16_at(data, at + index * 2)?;
    }
    Ok(result)
}

fn decode_channel(
    encoded: &[u8],
    samples: usize,
    coefs: &[i16; 16],
    mut hist1: i32,
    mut hist2: i32,
) -> Vec<i16> {
    let mut output = Vec::with_capacity(samples);
    for frame in encoded.chunks_exact(8) {
        let predictor = (frame[0] >> 4) as usize;
        let scale = 1i32 << (frame[0] & 0xf);
        if predictor >= 8 {
            break;
        }
        for index in 0..14 {
            let packed = frame[1 + index / 2];
            let nibble = if index % 2 == 0 {
                packed >> 4
            } else {
                packed & 0xf
            };
            let signed = if nibble >= 8 {
                nibble as i32 - 16
            } else {
                nibble as i32
            };
            let value = ((signed * scale * 2048
                + coefs[predictor * 2] as i32 * hist1
                + coefs[predictor * 2 + 1] as i32 * hist2
                + 1024)
                >> 11)
                .clamp(i16::MIN as i32, i16::MAX as i32);
            hist2 = hist1;
            hist1 = value;
            output.push(value as i16);
            if output.len() == samples {
                return output;
            }
        }
    }
    output
}

pub fn decode(data: &[u8]) -> Result<DecodedAudio, String> {
    let (channel_count, sample_count) = header(data)?;
    let sample_rate = u32_at(data, 0x14)?;
    let looping = u32_at(data, 0x10 + 0x38)? != 0;
    let loop_start = u32_at(data, 0x10 + 0x40)?;
    let mut channels = Vec::with_capacity(channel_count);
    for channel in 0..channel_count {
        let info = 0x10 + channel * INFO_SIZE;
        if sample_count == 0 {
            channels.push(Vec::new());
            continue;
        }
        let offset = u32_at(data, info + 0x34)? as usize;
        let codec = u16_at(data, info)?;
        let encoded_size = match codec {
            0 => sample_count * 2,
            1 => ((sample_count + 13) / 14) * 8,
            _ => return Err(format!("unsupported BWAV codec {codec}")),
        };
        let encoded = data
            .get(offset..offset + encoded_size)
            .ok_or_else(|| format!("truncated BWAV channel {channel}"))?;
        let values = if codec == 0 {
            let mut reader = BinaryReader::new(encoded);
            let mut values = Vec::with_capacity(sample_count);
            while reader.remaining() >= 2 {
                values.push(
                    reader
                        .read_i16()
                        .map_err(|_| format!("truncated BWAV channel {channel}"))?,
                );
            }
            values
        } else {
            decode_channel(
                encoded,
                sample_count,
                &coefficients(data, channel)?,
                i16_at(data, info + 0x46)? as i32,
                i16_at(data, info + 0x48)? as i32,
            )
        };
        if values.len() != sample_count {
            return Err(format!(
                "BWAV channel {channel} decoded only {} of {sample_count} samples",
                values.len()
            ));
        }
        channels.push(values);
    }
    Ok(DecodedAudio {
        channels,
        sample_rate,
        looping,
        loop_start,
    })
}

pub fn to_wav(data: &[u8]) -> Result<Vec<u8>, String> {
    Bfwav::pcm_to_wav(&decode(data)?)
}

fn encode_channel(samples: &[i16], coefs: &[i16; 16]) -> Vec<u8> {
    let mut output = Vec::with_capacity(((samples.len() + 13) / 14) * 8);
    let mut hist1 = 0i32;
    let mut hist2 = 0i32;
    for source in samples.chunks(14) {
        let mut best = (i64::MAX, 0u8, [0i8; 14], hist1, hist2);
        for predictor in 0..8usize {
            for exponent in 0..=12u8 {
                let scale = 1i32 << exponent;
                let mut h1 = hist1;
                let mut h2 = hist2;
                let mut error = 0i64;
                let mut nibbles = [0i8; 14];
                for (index, &wanted) in source.iter().enumerate() {
                    let predicted = (coefs[predictor * 2] as i32 * h1
                        + coefs[predictor * 2 + 1] as i32 * h2
                        + 1024)
                        >> 11;
                    let nibble = (((wanted as i32 - predicted) as f64 / scale as f64).round()
                        as i32)
                        .clamp(-8, 7);
                    let reconstructed =
                        (predicted + nibble * scale).clamp(i16::MIN as i32, i16::MAX as i32);
                    let delta = wanted as i64 - reconstructed as i64;
                    error = error.saturating_add(delta.saturating_mul(delta));
                    nibbles[index] = nibble as i8;
                    h2 = h1;
                    h1 = reconstructed;
                }
                if error < best.0 {
                    best = (error, ((predictor as u8) << 4) | exponent, nibbles, h1, h2);
                }
            }
        }
        output.push(best.1);
        for pair in 0..7 {
            output.push(((best.2[pair * 2] as u8 & 0xf) << 4) | (best.2[pair * 2 + 1] as u8 & 0xf));
        }
        hist1 = best.3;
        hist2 = best.4;
    }
    output
}

pub fn encode_like(target: &[u8], source: &DecodedAudio) -> Result<Vec<u8>, String> {
    let (target_channels, _) = header(target)?;
    if source.channels.len() != target_channels {
        return Err(format!(
            "replacement has {} channels but target BWAV has {target_channels}",
            source.channels.len()
        ));
    }
    let sample_count = source
        .channels
        .first()
        .map(Vec::len)
        .ok_or("replacement has no channels")?;
    if source.channels.iter().any(|v| v.len() != sample_count) {
        return Err("replacement channels have different lengths".into());
    }
    let mut encoded = Vec::with_capacity(target_channels);
    for channel in 0..target_channels {
        let codec = u16_at(target, 0x10 + channel * INFO_SIZE)?;
        encoded.push(match codec {
            0 => {
                let mut writer = BinaryWriter::new();
                for value in &source.channels[channel] {
                    writer.write_i16(*value);
                }
                writer.into_inner()
            }
            1 => encode_channel(&source.channels[channel], &coefficients(target, channel)?),
            _ => return Err(format!("unsupported BWAV codec {codec}")),
        });
    }
    let encoded_len = encoded
        .first()
        .map(Vec::len)
        .ok_or("replacement encoded no channels")?;
    if encoded.iter().any(|v| v.len() != encoded_len) {
        return Err("BWAV channels use incompatible codecs".into());
    }
    let header_size = align(0x10 + target_channels * INFO_SIZE, 0x40);
    let channel_stride = align(encoded_len, 0x40);
    let total = header_size + channel_stride * target_channels;
    let mut output = vec![0u8; total];
    let header_len = 0x10 + target_channels * INFO_SIZE;
    let target_header = BinaryReader::new(target)
        .read_bytes_at(0, header_len)
        .map_err(|_| "truncated BWAV")?;
    put_bytes(&mut output, 0, target_header)?;
    put_u32(&mut output, 8, crc32(&encoded))?;
    put_u16(&mut output, 0x0c, 0)?;
    for channel in 0..target_channels {
        let info = 0x10 + channel * INFO_SIZE;
        put_u32(&mut output, info + 4, source.sample_rate)?;
        put_u32(&mut output, info + 8, sample_count as u32)?;
        put_u32(&mut output, info + 0x0c, sample_count as u32)?;
        let offset = header_size + channel * channel_stride;
        put_u32(&mut output, info + 0x30, offset as u32)?;
        put_u32(&mut output, info + 0x34, offset as u32)?;
        put_u32(&mut output, info + 0x38, 0)?;
        put_u32(&mut output, info + 0x3c, u32::MAX)?;
        put_u32(&mut output, info + 0x40, 0)?;
        put_u16(&mut output, info + 0x44, 0)?;
        put_u16(&mut output, info + 0x46, 0)?;
        put_u16(&mut output, info + 0x48, 0)?;
        put_bytes(&mut output, offset, &encoded[channel])?;
    }
    Ok(output)
}
