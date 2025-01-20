use std::fs::{create_dir_all, File};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

use bytesize::ByteSize;
use owo_colors::OwoColorize;

mod wem2wav;
mod pck_serilize;

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

        let (_, complete_size) =
            parse_header(&mut reader, start_offset, &file_name)?;
        let output_file_name = format!("{file_name}_{found_files}.wem");
        process_wave_file(
            &mut reader,
            start_offset,
            complete_size,
            output_dir,
            output_file_name,
        )?;

        found_files += 1;
        global_offset = offset + complete_size as u64;
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

/// Lê e valida o cabeçalho do arquivo WAVE.
fn parse_header(
    reader: &mut BufReader<File>,
    start_offset: i64,
    file_name: &str,
) -> std::io::Result<(&'static str, u32)> {
    reader.seek(SeekFrom::Start(start_offset as u64))?;
    let mut header = [0u8; 8];
    reader.read_exact(&mut header)?;

    let header_type = match &header[..4] {
        b"RIFF" => "RIFF",
        b"RIFX" => "RIFX",
        _ => panic!("Invalid WAVE header found!"),
    };

    let complete_size = u32::from_le_bytes(header[4..8].try_into().unwrap());
    println!(
        "[{}] HEADER: {header_type}, Reported Size: {}", file_name.bold().blue(),
        ByteSize::b(complete_size as u64).to_string_as(true)
    );

    Ok((header_type, complete_size))
}

/// Processa o arquivo WAVE encontrado e cria a cópia corrigida.
fn process_wave_file(
    reader: &mut BufReader<File>,
    start_offset: i64,
    complete_size: u32,
    output_dir: &Path,
    output_file_name: String,
) -> std::io::Result<()> {
    reader.seek(SeekFrom::Start(start_offset as u64))?;

    let mut data = vec![0u8; complete_size as usize];
    reader.read_exact(&mut data)?;

    // Corrigir o tamanho do cabeçalho do arquivo WAVE
    data[4..8].copy_from_slice(&(complete_size - 8).to_le_bytes());

    let output_path = output_dir.join(&output_file_name);
    create_dir_all(output_dir)?;
    let mut output_file = File::create(output_path)?;
    output_file.write_all(&data)?;

    println!(
        "Saved corrected WAVE file: {}",
        output_file_name.bold().green()
    );

    Ok(())
}
