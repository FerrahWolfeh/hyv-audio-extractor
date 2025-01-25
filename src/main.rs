use std::fs::{create_dir_all, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use bytesize::ByteSize;
use owo_colors::OwoColorize;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = Path::new(&args[1]);
    let file = File::open(path)?;
    let work_fname_str = path.file_name().unwrap().to_string_lossy();

    println!(
        "Opened file {:?}\n[{}] Size: {}",
        path.file_name().unwrap(),
        work_fname_str.bold().blue(),
        ByteSize::b(file.metadata().unwrap().size())
    );
    let mut reader = BufReader::new(file);

    // Read the file in chunks to find the "WAVE" marker
    let mut buffer = [0u8; 4096];
    let mut global_offset = 0;
    let mut found_files = 0;

    loop {
        reader.seek(SeekFrom::Start(global_offset))?;
        let nbytes_read = reader.read(&mut buffer)?;
        if nbytes_read == 0 {
            break;
        }
        for (offset, byte) in buffer[..nbytes_read].iter().enumerate() {
            // checkpoint is in 0x3664
            if offset < buffer.len() - 4
                && *byte == 0x57
                && buffer[offset + 1] == 0x41
                && buffer[offset + 2] == 0x56
                && buffer[offset + 3] == 0x45
            {
                let actual_file_start = global_offset - 8;

                println!(
                    "[{}] Found WAVE marker at offset {:#x} | File starts at {:#x}",
                    work_fname_str.bold().blue(),
                    global_offset,
                    actual_file_start
                );

                reader.seek(SeekFrom::Start(actual_file_start))?; // Return to start of RIFF/RIFX header

                let mut ident = [0u8; 8]; // WEM headers are exactly like WAV ones, so let's read just what we need from it
                reader.read_exact(&mut ident)?;

                // Check if the header is RIFF or RIFX
                let header_type = match &ident[..4] {
                    [0x52, 0x49, 0x46, 0x46] => "RIFF",
                    [0x52, 0x49, 0x46, 0x58] => "RIFX",
                    _ => "INVALID",
                };

                let complete_size = u32::from_le_bytes(ident[4..8].try_into().unwrap()); // The next 4 bytes after the fourcc is the assumed total size of the file. Most of the time, it's wrong.'

                let out_fname = format!("{work_fname_str}_{found_files}.wem");

                println!(
                    "[{}:{:#x}] HEADER: {header_type}, Reported Size: {}",
                    work_fname_str.bold().blue(),
                    reader.stream_position()?,
                    ByteSize::b(complete_size as u64).to_string_as(true),
                );

                reader.seek_relative(-8)?; // Starting here, we are back at the very start of the WAVE entry.

                if reader.stream_position()? != actual_file_start {
                    panic!(
                        "[{}] File position mismatch: Stream position: {:#x} | Start: {:#x}",
                        work_fname_str.bold().blue(),
                        reader.stream_position()?,
                        actual_file_start
                    )
                }

                reader.seek_relative(12)?; // Jump to fmt section

                loop {
                    let mut area_name = [0u8; 4];
                    reader.read_exact(&mut area_name)?;

                    let mut area_size_bytes = [0u8; 4];
                    reader.read_exact(&mut area_size_bytes)?;

                    let area_size = u32::from_le_bytes(area_size_bytes);

                    let current_pos = reader.stream_position()?;
                    if &area_name == b"data" {
                        // Found the "data" area
                        let stream_size = area_size;
                        let header_size = current_pos - actual_file_start;
                        let total_file_size = header_size + stream_size as u64;

                        let actual_data_size = (total_file_size - 8) as u32;

                        if complete_size != total_file_size as u32 {
                            println!(
                                "[{}:{:#x}] Size mismatch: Actual file size is: {}.",
                                work_fname_str.bold().blue(),
                                actual_file_start,
                                ByteSize::b(total_file_size).to_string_as(true),
                            );
                        }

                        // Begin allocating new file in memory
                        let mut fvec = vec![0u8; 0];
                        fvec.reserve_exact(total_file_size as usize);

                        unsafe { fvec.set_len(total_file_size as usize) }

                        // Write corrected header and data
                        reader.seek(SeekFrom::Start(actual_file_start))?;
                        reader.read_exact(&mut fvec)?;

                        // Correct RIFF size in header
                        fvec[4..8].copy_from_slice(&actual_data_size.to_le_bytes());

                        // Write the file into destination.
                        let parent_path = Path::new(&args[2]);
                        create_dir_all(parent_path)?;

                        let out_path = parent_path.join(&out_fname);

                        let mut nf = File::create(&out_path)?;

                        nf.set_len(total_file_size)?;

                        nf.write_all(&fvec)?;

                        break;
                    } else {
                        // Skip to next area if not "data"
                        reader.seek(SeekFrom::Start(current_pos + area_size as u64))?;
                    }
                }

                let stop = reader.stream_position()?;

                println!(
                    "[{}] We stopped reading at {:#x}, while the file ends at {:#x}",
                    work_fname_str.bold().blue(),
                    stop,
                    actual_file_start + complete_size as u64
                );

                found_files += 1;
                global_offset += complete_size as u64;

                break;
            }
            global_offset += 1;
        }
    }

    Ok(())
}
