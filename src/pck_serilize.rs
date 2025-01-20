use std::convert::TryInto;
use std::io::{Cursor, Read, Seek, Result as IoResult, Error as IoError, ErrorKind};
use byteorder::{LittleEndian, ReadBytesExt};

#[derive(Debug)]
pub struct WEMHeader {
    pub ckid: [char; 4],
    pub ck_size: u32,
    pub waveid: u32,
}

#[derive(Debug)]
pub struct RIFFChunk {
    pub r#type: [char; 4],
    pub size: u32,
}

#[derive(Debug)]
pub struct FMTChunk {
    pub format_tag: u16,
    pub channels: u16,
    pub samples_per_sec: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub size: u16,
    pub valid_bits_per_sample: Option<u16>,
    pub channel_mask: Option<u32>,
    pub guid: Option<[u8; 16]>, // Represent GUID as byte array
}

#[derive(Debug)]
pub struct CUEChunk {
    pub cue_count: u32,
}

#[derive(Debug)]
pub struct JUNKChunk {
    pub junk: Vec<u8>, // Use Vec<u8> to store the junk data
}

#[derive(Debug)]
pub struct DataChunk {
    pub data: Vec<u8>, // Use a Vec<u8> for variable-length data
}

#[derive(Debug)]
pub struct WEMFile {
    pub header: WEMHeader,
    pub chunks: Vec<Chunk>, // Use an enum to represent different chunk types
    pub data: DataChunk,
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
        // Read the RIFF/RIFX chunk header
        let mut riff_header_bytes = [0u8; 8];
        reader.read_exact(&mut riff_header_bytes)?;

        let riff_type = [
            riff_header_bytes[0] as char,
            riff_header_bytes[1] as char,
            riff_header_bytes[2] as char,
            riff_header_bytes[3] as char,
        ];
        let riff_size = u32::from_le_bytes(riff_header_bytes[4..8].try_into().unwrap());

        if riff_type != ['R', 'I', 'F', 'F'] && riff_type != ['R', 'I', 'F', 'X'] {
            return Err(IoError::new(ErrorKind::InvalidData, "Invalid RIFF/RIFX header"));
        }

        // Read the WEMHeader
        let mut header_bytes = [0u8; 12];
        reader.read_exact(&mut header_bytes)?;
        let header = WEMHeader {
            ckid: [
                header_bytes[0] as char,
                header_bytes[1] as char,
                header_bytes[2] as char,
                header_bytes[3] as char,
            ],
            ck_size: u32::from_le_bytes(header_bytes[4..8].try_into().unwrap()),
            waveid: u32::from_le_bytes(header_bytes[8..12].try_into().unwrap()),
        };

        if header.ckid != ['W', 'A', 'V', 'E'] {
            return Err(IoError::new(ErrorKind::InvalidData, "Invalid WAVE header"));
        }

        // Validate the overall size
        // The RIFF chunk size excludes the RIFF header itself (8 bytes),
        // and the WEM header size is 12 bytes.
        if riff_size < 4 || header.ck_size != riff_size - 4 {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                format!(
                    "RIFF chunk size mismatch: Expected {}, got {}",
                    header.ck_size + 4,
                    riff_size
                ),
            ));
        }

        let mut chunks = Vec::new();
        let mut data_chunk_data = Vec::new();

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

            match chunk_type_str {
                "fmt " => {
                    let mut fmt_bytes = vec![0u8; chunk_size as usize];
                    reader.read_exact(&mut fmt_bytes)?;
                    let mut fmt_cursor = Cursor::new(&fmt_bytes);
                    let format_tag = fmt_cursor.read_u16::<LittleEndian>()?;
                    let channels = fmt_cursor.read_u16::<LittleEndian>()?;
                    let samples_per_sec = fmt_cursor.read_u32::<LittleEndian>()?;
                    let avg_bytes_per_sec = fmt_cursor.read_u32::<LittleEndian>()?;
                    let blockalign = fmt_cursor.read_u16::<LittleEndian>()?;
                    let bits_per_sample = fmt_cursor.read_u16::<LittleEndian>()?;

                    let mut valid_bits_per_sample = None;
                    let mut channel_mask = None;
                    let mut guid = None;

                    if chunk_size >= 18 {
                        valid_bits_per_sample = Some(fmt_cursor.read_u16::<LittleEndian>()?);
                    }
                    if chunk_size >= 22 {
                        channel_mask = Some(fmt_cursor.read_u32::<LittleEndian>()?);
                    }
                    if chunk_size >= 38 {
                        let mut guid_bytes = [0u8; 16];
                        fmt_cursor.read_exact(&mut guid_bytes)?;
                        guid = Some(guid_bytes);
                    }

                    chunks.push(Chunk::FMT(FMTChunk {
                        format_tag,
                        channels,
                        samples_per_sec,
                        avg_bytes_per_sec,
                        block_align: blockalign,
                        bits_per_sample,
                        size: chunk_size as u16,
                        valid_bits_per_sample,
                        channel_mask,
                        guid,
                    }));
                }
                "JUNK" => {
                    let mut junk_data = vec![0u8; chunk_size as usize];
                    reader.read_exact(&mut junk_data)?;
                    chunks.push(Chunk::JUNK(JUNKChunk { junk: junk_data }));
                }
                "cue " => {
                    let mut cue_bytes = vec![0u8; chunk_size as usize];
                    reader.read_exact(&mut cue_bytes)?;
                    let cue_count = u32::from_le_bytes(cue_bytes[0..4].try_into().unwrap());
                    chunks.push(Chunk::CUE(CUEChunk { cue_count }));
                }
                "data" => {
                    let mut data = vec![0u8; chunk_size as usize];
                    reader.read_exact(&mut data)?;
                    data_chunk_data.extend_from_slice(&data);
                }
                _ => {
                    // Skip unknown chunks
                    reader.seek(std::io::SeekFrom::Current(chunk_size as i64))?;
                }
            }
        }

        Ok(WEMFile {
            header,
            chunks,
            data: DataChunk { data: data_chunk_data },
        })
    }
}