use std::fs::{create_dir_all, File};
use std::io::{BufReader, Read, Seek, SeekFrom, Write, Cursor, BufWriter};
use std::path::Path;
use byteorder::ReadBytesExt;
use bytesize::ByteSize;
use hound::{SampleFormat, WavSpec, WavWriter};
use owo_colors::OwoColorize;

mod wem2wav;
mod pck_serilize;

// Import the WEMFile struct
use crate::pck_serilize::{Chunk, WEMFile};

// Constantes úteis
const BUFFER_SIZE: usize = 4096;
const WAVE_MARKER: &[u8; 4] = b"WAVE";

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let input_path = Path::new(&args[1]);
    let output_dir = Path::new(&args[2]);

    let file = File::open(input_path)?;
    let file_size = file.metadata()?.len();
    let file_name = input_path.file_name().unwrap().to_string_lossy();
    println!(
        "Opened file {:?}\n[{}] Size: {}",
        input_path.file_name().unwrap(),
        file_name.bold().blue(),
        ByteSize::b(file_size)
    );

    let mut reader = BufReader::new(file);
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut global_offset = 0;
    let mut found_files = 0;

    while let Some(offset) = find_wave_marker(&mut reader, &mut buffer, global_offset)? {
        let start_offset = offset as i64 - 8;
        println!(
            "[{}] Found WAVE marker at offset {:#x} | File starts at {:#x}",
            file_name.bold().blue(),
            offset,
            start_offset
        );

        // Instead of parsing the header naively, we now rely on WEMFile::from_read
        let output_file_name = format!("{file_name}_{found_files}.wav");
        if process_wave_file(
            &mut reader,
            start_offset,
            output_dir,
            output_file_name,
        ).is_ok() {
            found_files += 1;
            // We don't know the exact size beforehand anymore; let process_wave_file handle it.
            // For the next search, we need to advance past the extracted WEM file.
            // A potential improvement would be to get the size from WEMFile::from_read if needed.
            // For now, a simple heuristic based on the initial header size is used.
            reader.seek(SeekFrom::Start(offset + 12u64))?; // Advance past the WAVE header
            global_offset = offset + 12;
        } else {
            eprintln!(
                "[{}] Failed to parse WEM file starting at offset {:#x}",
                file_name.bold().red(),
                start_offset
            );
            // If parsing fails, advance the offset slightly to avoid getting stuck.
            reader.seek(SeekFrom::Start(offset + 1))?;
            global_offset = offset + 1;
        }
    }

    Ok(())
}

/// Procura pelo marcador "WAVE" no buffer.
fn find_wave_marker(
    reader: &mut BufReader<File>,
    buffer: &mut [u8; BUFFER_SIZE],
    start_offset: u64,
) -> std::io::Result<Option<u64>> {
    reader.seek(SeekFrom::Start(start_offset))?;
    let mut global_offset = start_offset;

    loop {
        let bytes_read = reader.read(buffer)?;
        if bytes_read == 0 {
            break;
        }

        for (offset, window) in buffer[..bytes_read].windows(4).enumerate() {
            if window == WAVE_MARKER {
                return Ok(Some(global_offset + offset as u64));
            }
        }
        global_offset += bytes_read as u64;
    }

    Ok(None)
}

/// Processes the WAVE file by parsing it with WEMFile and saving it.
fn process_wave_file(
    reader: &mut BufReader<File>,
    start_offset: i64,
    output_dir: &Path,
    output_file_name: String,
) -> std::io::Result<()> {
    reader.seek(SeekFrom::Start(start_offset as u64))?;

    // Read enough bytes for the initial RIFF header and WEM header
    let mut initial_bytes = vec![0u8; 20]; // 8 for RIFF + 12 for WEM
    if reader.read_exact(&mut initial_bytes).is_err() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "Failed to read initial header bytes",
        ));
    }

    let mut cursor = Cursor::new(&initial_bytes);

    let riff_type = [
        cursor.read_u8()? as char,
        cursor.read_u8()? as char,
        cursor.read_u8()? as char,
        cursor.read_u8()? as char,
    ];

    if riff_type != ['R', 'I', 'F', 'F'] && riff_type != ['R', 'I', 'F', 'X'] {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid RIFF header",
        ));
    }

    // Reset the reader to the start of the WEM file
    reader.seek(SeekFrom::Start(start_offset as u64))?;

    match WEMFile::from_read(reader) {
        Ok(wem_file) => {
            let output_path = output_dir.join(&output_file_name);
            create_dir_all(output_dir)?;
            let mut output_file = File::create(&output_path)?;

            // let wav_fmt = WavSpec {
            //     channels: wem_file.fmt_chunk.channels,
            //     sample_rate: wem_file.fmt_chunk.samples_per_sec,
            //     bits_per_sample: wem_file.fmt_chunk.valid_bits_per_sample.unwrap(),
            //     sample_format: SampleFormat::Int,
            // };
            // 
            // let wav_writer = WavWriter::create(output_path, wav_fmt).unwrap();
            
            

            // Serialize the WEMFile back to bytes and write it.
            // This assumes you want to save the parsed structure.
            // If you want to save the original bytes, you'd need to adjust.
            
            let mut writer = BufWriter::new(&mut output_file);
            
            // Write RIFF header
            for &c in &wem_file.header.magic {
                writer.write_all(&[c as u8])?;
            }
            writer.write_all(&wem_file.header.file_size.to_le_bytes())?;
            
            for &c in &wem_file.header.wave {
                writer.write_all(&[c as u8])?;
            }

            let fmt_chunk = wem_file.fmt_chunk;

            for &c in &fmt_chunk.r#type {
                writer.write_all(&[c as u8])?;
            }
            
            writer.write_all(&fmt_chunk.size.to_le_bytes())?;
            
            writer.write_all(&fmt_chunk.chunk_data.format_tag.to_le_bytes())?;
            writer.write_all(&fmt_chunk.chunk_data.channels.to_le_bytes())?;
            writer.write_all(&fmt_chunk.chunk_data.samples_per_sec.to_le_bytes())?;
            writer.write_all(&fmt_chunk.chunk_data.avg_bitrate.to_le_bytes())?;
            writer.write_all(&fmt_chunk.chunk_data.block_size.to_le_bytes())?;
            writer.write_all(&fmt_chunk.chunk_data.bits_per_sample.to_le_bytes())?;
            writer.write_all(&fmt_chunk.chunk_data.extra_size.to_le_bytes())?;
            writer.write_all(&fmt_chunk.chunk_data.remainder_data)?;
            
            // 
            // 
            // for chunk in &wem_file.other_chunks {
            //     match chunk {
            //         Chunk::FMT(fmt) => {
            //             writer.write_all(b"fmt ")?;
            //             writer.write_all(&(fmt.size as u32).to_le_bytes())?;
            //             writer.write_all(&fmt.format_tag.to_le_bytes())?;
            //             writer.write_all(&fmt.channels.to_le_bytes())?;
            //             writer.write_all(&fmt.samples_per_sec.to_le_bytes())?;
            //             writer.write_all(&fmt.avg_bytes_per_sec.to_le_bytes())?;
            //             writer.write_all(&fmt.block_align.to_le_bytes())?;
            //             writer.write_all(&fmt.bits_per_sample.to_le_bytes())?;
            //             if let Some(v) = fmt.valid_bits_per_sample {
            //                 writer.write_all(&v.to_le_bytes())?;
            //             }
            //             if let Some(m) = fmt.channel_mask {
            //                 writer.write_all(&m.to_le_bytes())?;
            //             }
            //             if let Some(g) = fmt.guid {
            //                 writer.write_all(g.as_ref())?;
            //             }
            //         }
            //         Chunk::JUNK(junk) => {
            //             writer.write_all(b"JUNK")?;
            //             writer.write_all(&(junk.junk.len() as u32).to_le_bytes())?;
            //             writer.write_all(&junk.junk)?;
            //         }
            //         Chunk::CUE(cue) => {
            //             writer.write_all(b"cue ")?;
            //             writer.write_all(&cue.cue_count.to_le_bytes())?;
            //         }
            //     }
            // }

            let data_chunk = wem_file.data;

            for &c in &data_chunk.r#type {
                writer.write_all(&[c as u8])?;
            }

            writer.write_all(&data_chunk.size.to_le_bytes())?;
            
            writer.write_all(&data_chunk.chunk_data.data)?;
            
            writer.flush()?;

            println!(
                "Saved parsed WEM file: {}",
                output_file_name.bold().green()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!(
                "[{}] Error parsing WEM file at offset {:#x}: {}",
                output_file_name.bold().red(),
                start_offset,
                e
            );
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to parse WEM file",
            ))
        }
    }
}