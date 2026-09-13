use std::{fs::File, path::Path};

use crate::parser::binary::{BinaryReader, BinaryWriter, Endian};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions,
    io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};

fn truncated<T>(_: std::io::Error) -> Result<T, String> {
    Err("truncated BFWAV".into())
}

fn align(value: usize, boundary: usize) -> usize {
    (value + boundary - 1) & !(boundary - 1)
}

#[derive(Clone)]
pub struct DecodedAudio {
    pub channels: Vec<Vec<i16>>,
    pub sample_rate: u32,
    pub looping: bool,
    pub loop_start: u32,
}

pub fn decode(data: &[u8]) -> Result<DecodedAudio, String> {
    if !crate::Settings::Magic::is_bfwav(data) || data.len() < 0x20 {
        return Err("not a BFWAV file".into());
    }
    let endian = match BinaryReader::new(data)
        .read_bytes_at(4, 2)
        .or_else(truncated)?
    {
        b"\xfe\xff" => Endian::Big,
        b"\xff\xfe" => Endian::Little,
        _ => return Err("invalid BFWAV byte-order mark".into()),
    };
    let reader = BinaryReader::with_endian(data, endian);
    let u8_at = |at: usize| reader.read_u8_at(at).or_else(truncated);
    let u16_at = |at: usize| reader.read_u16_at(at).or_else(truncated);
    let i16_at = |at: usize| reader.read_i16_at(at).or_else(truncated);
    let u32_at = |at: usize| reader.read_u32_at(at).or_else(truncated);
    let bytes_at = |at: usize, len: usize| reader.read_bytes_at(at, len).or_else(truncated);

    let blocks = u16_at(0x10)? as usize;
    let mut info = None;
    let mut audio = None;
    for index in 0..blocks {
        let at = 0x14 + index * 12;
        let ty = u16_at(at)?;
        let offset = u32_at(at + 4)? as usize;
        match ty {
            0x7000 => info = Some(offset),
            0x7001 => audio = Some(offset),
            _ => {}
        }
    }
    let info = info.ok_or("BFWAV has no INFO block")?;
    let audio = audio.ok_or("BFWAV has no DATA block")?;
    if bytes_at(info, 4)? != b"INFO" || bytes_at(audio, 4)? != b"DATA" {
        return Err("invalid BFWAV blocks".into());
    }
    let stream = info + 8;
    let codec = u8_at(stream)?;
    let looping = u8_at(stream + 1)? != 0;
    let sample_rate = u32_at(stream + 4)?;
    let loop_start = u32_at(stream + 8)?;
    let sample_count = u32_at(stream + 12)? as usize;
    let table = stream + 20;
    let channel_count = u32_at(table)? as usize;
    if channel_count == 0 || channel_count > 32 {
        return Err(format!("invalid BFWAV channel count {channel_count}"));
    }
    let bytes_per_channel = match codec {
        0 => sample_count,
        1 => sample_count * 2,
        2 => ((sample_count + 13) / 14) * 8,
        _ => return Err(format!("unsupported BFWAV codec {codec}")),
    };
    let data_base = audio + 8;
    let mut channels = Vec::with_capacity(channel_count);
    for channel in 0..channel_count {
        let reference = table + 4 + channel * 8;
        if u16_at(reference)? != 0x7100 {
            return Err("invalid BFWAV channel reference".into());
        }
        let channel_info = table + u32_at(reference + 4)? as usize;
        let audio_offset = u32_at(channel_info + 4)? as usize;
        // Validated to hold `bytes_per_channel` bytes, which bounds every
        // allocation below.
        let encoded = bytes_at(data_base + audio_offset, bytes_per_channel)?;
        let samples = match codec {
            0 => encoded
                .iter()
                .take(sample_count)
                .map(|&v| (v as i8 as i16) << 8)
                .collect(),
            1 => {
                let mut pcm = BinaryReader::with_endian(encoded, endian);
                let mut samples = Vec::with_capacity(sample_count);
                while samples.len() < sample_count && pcm.remaining() >= 2 {
                    samples.push(pcm.read_i16().or_else(truncated)?);
                }
                samples
            }
            2 => {
                let adpcm_offset = u32_at(channel_info + 12)? as usize;
                let adpcm = channel_info + adpcm_offset;
                let mut coefs = [0i16; 16];
                for (i, coef) in coefs.iter_mut().enumerate() {
                    *coef = i16_at(adpcm + i * 2)?;
                }
                let mut hist1 = i16_at(adpcm + 34)? as i32;
                let mut hist2 = i16_at(adpcm + 36)? as i32;
                let mut decoded = Vec::with_capacity(sample_count);
                for frame in encoded.chunks_exact(8) {
                    let header = frame[0];
                    let predictor = (header >> 4) as usize;
                    let scale = 1i32 << (header & 0xf);
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
                        decoded.push(value as i16);
                        if decoded.len() == sample_count {
                            break;
                        }
                    }
                }
                decoded
            }
            _ => return Err(format!("unsupported BFWAV codec {codec}")),
        };
        channels.push(samples);
    }
    Ok(DecodedAudio {
        channels,
        sample_rate,
        looping,
        loop_start,
    })
}

pub fn to_wav(data: &[u8]) -> Result<Vec<u8>, String> {
    let decoded = decode(data)?;
    pcm_to_wav(&decoded)
}

pub fn pcm_to_wav(decoded: &DecodedAudio) -> Result<Vec<u8>, String> {
    let channel_count = decoded.channels.len();
    if channel_count == 0 {
        return Err("audio has no channels".into());
    }
    let sample_count = decoded.channels[0].len();
    if decoded.channels.iter().any(|v| v.len() != sample_count) {
        return Err("audio channels have different lengths".into());
    }
    let data_size = sample_count * channel_count * 2;
    let mut out = BinaryWriter::with_endian(Endian::Little);
    out.write_bytes(b"RIFF");
    out.write_u32(36u32 + data_size as u32);
    out.write_bytes(b"WAVEfmt ");
    out.write_u32(16);
    out.write_u16(1);
    out.write_u16(channel_count as u16);
    out.write_u32(decoded.sample_rate);
    out.write_u32(decoded.sample_rate * channel_count as u32 * 2);
    out.write_u16((channel_count * 2) as u16);
    out.write_u16(16);
    out.write_bytes(b"data");
    out.write_u32(data_size as u32);
    for sample in 0..sample_count {
        for channel in &decoded.channels {
            out.write_i16(channel[sample]);
        }
    }
    Ok(out.into_inner())
}

pub fn decode_source(path: &Path) -> Result<DecodedAudio, String> {
    let file = File::open(path).map_err(|e| format!("failed to open {}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|v| v.to_str()) {
        hint.with_extension(extension);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("unsupported audio source: {e}"))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or("audio source has no default track")?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let mut channels: Vec<Vec<i16>> = Vec::new();
    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(48_000);
    loop {
        let packet = match format.next_packet() {
            Ok(v) => v,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.to_string()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(v) => v,
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        };
        sample_rate = decoded.spec().rate;
        let count = decoded.spec().channels.count();
        if channels.is_empty() {
            channels.resize_with(count, Vec::new);
        }
        if channels.len() != count {
            return Err("audio channel count changed while decoding".into());
        }
        let mut samples = SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec());
        samples.copy_interleaved_ref(decoded);
        for frame in samples.samples().chunks_exact(count) {
            for (channel, &sample) in frame.iter().enumerate() {
                channels[channel].push(sample);
            }
        }
    }
    if channels.is_empty() {
        return Err("audio source decoded no samples".into());
    }
    Ok(DecodedAudio {
        channels,
        sample_rate,
        looping: false,
        loop_start: 0,
    })
}

pub fn encode_pcm16(audio: &DecodedAudio) -> Result<Vec<u8>, String> {
    if audio.channels.is_empty() || audio.channels.len() > 32 {
        return Err("unsupported channel count".into());
    }
    let samples = audio.channels[0].len();
    if audio.channels.iter().any(|v| v.len() != samples) {
        return Err("audio channels have different lengths".into());
    }
    let channels = audio.channels.len();
    let table_size = 4 + channels * 8;
    let channel_info_size = channels * 16;
    let info_size = align(8 + 20 + table_size + channel_info_size, 0x20);
    let header_size = 0x40;
    let info_at = header_size;
    let data_at = info_at + info_size;
    let channel_bytes = samples * 2;
    let channel_stride = align(channel_bytes, 0x20);
    let data_size = 8 + channel_stride * channels;
    let file_size = data_at + data_size;
    let mut out = BinaryWriter::with_endian(Endian::Big);
    out.write_zeros(file_size);
    out.seek(0);
    out.write_bytes(b"FWAV");
    out.write_bytes(b"\xfe\xff");
    out.write_u16(header_size as u16);
    out.write_u32(0x0001_0200);
    out.write_u32(file_size as u32);
    out.write_u16(2);
    out.write_u16_at(0x14, 0x7000);
    out.write_u32_at(0x18, info_at as u32);
    out.write_u32_at(0x1c, info_size as u32);
    out.write_u16_at(0x20, 0x7001);
    out.write_u32_at(0x24, data_at as u32);
    out.write_u32_at(0x28, data_size as u32);
    out.seek(info_at);
    out.write_bytes(b"INFO");
    out.write_u32(info_size as u32);
    let stream = info_at + 8;
    out.write_u8_at(stream, 1);
    out.write_u8_at(stream + 1, audio.looping as u8);
    out.write_u32_at(stream + 4, audio.sample_rate);
    out.write_u32_at(stream + 8, audio.loop_start);
    out.write_u32_at(stream + 12, samples as u32);
    out.write_u32_at(stream + 16, audio.loop_start);
    let table = stream + 20;
    out.write_u32_at(table, channels as u32);
    let infos = table + table_size;
    for channel in 0..channels {
        let reference = table + 4 + channel * 8;
        out.write_u16_at(reference, 0x7100);
        out.write_u32_at(reference + 4, (infos + channel * 16 - table) as u32);
        let info = infos + channel * 16;
        out.write_u16_at(info, 0x1f00);
        out.write_u32_at(info + 4, (channel * channel_stride) as u32);
        out.write_u32_at(info + 12, u32::MAX);
    }
    out.seek(data_at);
    out.write_bytes(b"DATA");
    out.write_u32(data_size as u32);
    for (channel, values) in audio.channels.iter().enumerate() {
        out.seek(data_at + 8 + channel * channel_stride);
        for &value in values {
            out.write_i16(value);
        }
    }
    let out = out.into_inner();
    if out.len() != file_size {
        return Err("BFWAV layout mismatch".into());
    }
    Ok(out)
}
