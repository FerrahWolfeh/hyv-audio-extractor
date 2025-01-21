use std::convert::TryInto;
use std::io::{Cursor, Read, Seek, Result as IoResult, Error as IoError, ErrorKind};
use byteorder::{LittleEndian, ReadBytesExt};

#[derive(Debug)]
pub struct WEMHeader { // This entire thing is 12 bytes long
    pub magic: [char; 4], // Usually is RIFF or RIFX, but Star Rail doesn't use RIFX, thankfully.
    pub file_size: u32, // This most of the time is completely wrong, so we need to correct this value manually.
    pub wave: [char; 4], // WAVE marker
}

#[derive(Debug, Default)]
pub struct RIFFChunk<T> {
    pub r#type: [char; 4],
    pub size: u32,
    pub chunk_data: T
}

impl <T> RIFFChunk<T> {
    pub fn new(r#type: [char; 4], size: u32, chunk_data: T) -> Self {
        RIFFChunk {
            r#type,
            size,
            chunk_data
        }
    }
}

#[derive(Debug, Default)]
pub struct FMTChunk {
    pub format_tag: u16,
    pub channels: u16,
    pub samples_per_sec: u32,
    pub avg_bitrate: u32,
    pub block_size: u16,
    pub bits_per_sample: u16,
    pub extra_size: u16,
    pub remainder_data: Vec<u8>,
}

#[derive(Debug)]
pub struct CUEChunk {
    pub cue_count: u32,
}

#[derive(Debug)]
pub struct JUNKChunk {
    pub junk: Vec<u8>, // Use Vec<u8> to store the junk data
}

#[derive(Debug, Default)]
pub struct DataChunk {
    pub data: Vec<u8>, // Use a Vec<u8> for variable-length data
}

#[derive(Debug)]
pub struct WEMFile {
    pub header: WEMHeader,
    pub fmt_chunk: RIFFChunk<FMTChunk>,
    pub other_chunks: Vec<RIFFChunk<Chunk>>, // Use an enum to represent different chunk types
    pub data: RIFFChunk<DataChunk>,
}

#[derive(Debug)]
pub enum Chunk {
    FMT(FMTChunk),
    JUNK(JUNKChunk),
    CUE(CUEChunk),
    // Add other chunk types as needed
}

impl WEMFile {
    pub fn from_read<R: Read + Seek>(mut reader: R) -> IoResult<Self> {
        // Read the WEMHeader
        let mut header_bytes = [0u8; 12];
        reader.read_exact(&mut header_bytes)?;
        let mut header = WEMHeader {
            magic: [
                header_bytes[0] as char,
                header_bytes[1] as char,
                header_bytes[2] as char,
                header_bytes[3] as char,
            ],
            file_size: 4, // for now, we assume 4 bytes in size because this is almost always wrong.
            wave: [
                header_bytes[8] as char,
                header_bytes[9] as char,
                header_bytes[10] as char,
                header_bytes[11] as char,
            ],
        };

        if header.magic != ['R', 'I', 'F', 'F'] && header.magic != ['R', 'I', 'F', 'X'] {
            return Err(IoError::new(ErrorKind::InvalidData, "Invalid RIFF/RIFX marker"));
        }

        if header.wave != ['W', 'A', 'V', 'E'] {
            return Err(IoError::new(ErrorKind::InvalidData, "Invalid WAVE marker"));
        }

        let mut other_chunks = Vec::new();
        let mut data_chunk_data = RIFFChunk::default();
        let mut fmt_chunk = RIFFChunk::default();

        loop {
            let mut chunk_type_bytes = [0u8; 4];
            if reader.read_exact(&mut chunk_type_bytes).is_err() {
                break; // No more data to read
            }
            let chunk_type_str = std::str::from_utf8(&chunk_type_bytes)
                .map_err(|e| IoError::new(ErrorKind::InvalidData, e))?;

            let mut chunk_size_bytes = [0u8; 4];
            reader.read_exact(&mut chunk_size_bytes)?;
            let chunk_size = u32::from_le_bytes(chunk_size_bytes);

            header.file_size += chunk_size;

            match chunk_type_str {
                "fmt " => {
                    let mut fmt_bytes = vec![0u8; chunk_size as usize];
                    
                    reader.read_exact(&mut fmt_bytes)?;
                    
                    let mut fmt_cursor = Cursor::new(&fmt_bytes);
                    let format_tag = fmt_cursor.read_u16::<LittleEndian>()?;
                    let channels = fmt_cursor.read_u16::<LittleEndian>()?;
                    let samples_per_sec = fmt_cursor.read_u32::<LittleEndian>()?;
                    let avg_bytes_per_sec = fmt_cursor.read_u32::<LittleEndian>()?;
                    let block_align = fmt_cursor.read_u16::<LittleEndian>()?;
                    let bits_per_sample = fmt_cursor.read_u16::<LittleEndian>()?;
                    let size = fmt_cursor.read_u16::<LittleEndian>()?;

                    let mut remainder_data = vec![0u8; (chunk_size - (2 + 2 + 4 + 4 + 2 + 2 + 2)) as usize];
                    
                    fmt_cursor.read_exact(&mut remainder_data)?;

                    fmt_chunk = RIFFChunk::new(['f', 'm', 't', ' '], chunk_size + 8, FMTChunk {
                        format_tag,
                        channels,
                        samples_per_sec,
                        avg_bitrate: avg_bytes_per_sec,
                        block_size: block_align,
                        bits_per_sample,
                        extra_size: size,
                        remainder_data
                    });
                }
                "JUNK" => {
                    let mut junk_data = vec![0u8; chunk_size as usize];
                    reader.read_exact(&mut junk_data)?;
                    other_chunks.push(RIFFChunk::new(['J', 'U', 'N', 'K'], chunk_size + 8, Chunk::JUNK(JUNKChunk { junk: junk_data })));
                }
                "cue " => {
                    let mut cue_bytes = vec![0u8; chunk_size as usize];
                    reader.read_exact(&mut cue_bytes)?;
                    let cue_count = u32::from_le_bytes(cue_bytes[0..4].try_into().unwrap());
                    other_chunks.push(RIFFChunk::new(['c', 'u', 'e', ' '], chunk_size + 8, Chunk::CUE(CUEChunk { cue_count })));
                }
                "data" => {
                    let mut data = vec![0u8; chunk_size as usize];
                    reader.read_exact(&mut data)?;
                    data_chunk_data = RIFFChunk::new([ 'd', 'a', 't', 'a'], chunk_size + 8, DataChunk { data });
                }
                _ => {
                    // Skip unknown chunks
                    reader.seek(std::io::SeekFrom::Current(chunk_size as i64))?;
                }
            }
        }

        Ok(WEMFile {
            header,
            fmt_chunk,
            other_chunks,
            data: data_chunk_data,
        })
    }
}